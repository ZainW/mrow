//! `mrow --bench <folder> [label]`: times this app against a recording made by the Swift app's
//! `Mrow --bench-record <folder>`, measuring what the Swift app's `--bench` measures so the two
//! can be put side by side.
//!
//! The pure work (decoding, parsing, highlighting, word diffs, markdown, list filtering) runs in
//! this process. The window work runs in a headless child (`--scene bench`, which needs the
//! `profile` feature): a driver applies one change per frame (switching merge requests, opening
//! the biggest diff cold, scrolling it 240 pt a frame, opening the busiest discussion, typing in
//! the filter) and Rune's frame log times each of those frames. Results go to
//! `<folder>/results-<label>.json` in the Swift harness's format.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use rune::{TextEdit, Tx};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::app::App;
use crate::data::forge::Forge;
use crate::data::models::{Discussion, FileDiff, MergeRequest, Person};
use crate::data::store::{Account, QueueMode, Store};
use crate::data::{gitlab, markup, styler};
use crate::ui::detail;

/// Swift's `Date` counts from 2001-01-01; Unix time from 1970.
const SWIFT_EPOCH: f64 = 978_307_200.0;
/// The frame clock's rate in headless runs.
const FPS: f64 = 60.0;
const FILTER: &str = "shray";

pub struct Recording {
    me: Person,
    open: Vec<MergeRequest>,
    done: Vec<MergeRequest>,
    raw_diffs: HashMap<String, Vec<u8>>,
    threads: HashMap<String, Vec<Discussion>>,
}

impl Recording {
    pub fn load(folder: &Path) -> Result<Recording, String> {
        let raw = std::fs::read(folder.join("queue.json")).map_err(|e| format!("no recording in {}: {e}", folder.display()))?;
        let queue: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let items = |key: &str| -> Result<Vec<MergeRequest>, String> {
            let list = queue.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
            list.into_iter().map(|v| serde_json::from_value(swift_item(v)).map_err(|e| format!("{key}: {e}"))).collect()
        };
        let me: Person = serde_json::from_value(swift_keys(queue.get("me").cloned().unwrap_or(Value::Null))).map_err(|e| e.to_string())?;
        let (open, done) = (items("open")?, items("done")?);
        let mut raw_diffs = HashMap::new();
        let mut threads = HashMap::new();
        for item in open.iter().chain(&done) {
            if let Ok(raw) = std::fs::read(folder.join(format!("{}.diffs.json", key(item)))) {
                raw_diffs.insert(item.id.clone(), raw);
            }
            if let Ok(raw) = std::fs::read(folder.join(format!("{}.discussions.json", key(item)))) {
                threads.insert(item.id.clone(), gitlab::discussions_from_json(&raw).unwrap_or_default());
            }
        }
        Ok(Recording { me, open, done, raw_diffs, threads })
    }

    fn all(&self) -> impl Iterator<Item = &MergeRequest> {
        self.open.iter().chain(&self.done)
    }

    fn diffs(&self) -> HashMap<String, Vec<FileDiff>> {
        self.raw_diffs.iter().map(|(id, raw)| (id.clone(), gitlab::file_diffs_from_json(raw).unwrap_or_default())).collect()
    }

    /// Loads the recording into a store the way the app holds a refreshed queue.
    pub fn fill(&self, store: &mut Store) {
        let account = Account {
            me: Some(self.me.clone()),
            open: self.open.clone(),
            done: self.done.clone(),
            mine: vec![],
            mine_error: None,
            error: None,
            last_refreshed: Some(Utc::now()),
        };
        store.load_fixture(BTreeMap::from([(Forge::Gitlab, account)]), self.diffs(), self.threads.clone());
        store.forge = Forge::Gitlab;
        store.set_mode(QueueMode::ToReview);
    }

    /// The merge request with the most changed files: open ones first, as the Swift harness picks.
    fn biggest(&self, diffs: &HashMap<String, Vec<FileDiff>>) -> &MergeRequest {
        let count = |m: &MergeRequest| diffs.get(&m.id).map_or(0, Vec::len);
        self.open
            .iter()
            .filter(|m| count(m) > 0)
            .max_by_key(|m| count(m))
            .unwrap_or_else(|| self.all().max_by_key(|m| count(m)).expect("a recorded merge request"))
    }

    fn chattiest(&self) -> Option<&MergeRequest> {
        self.all().max_by_key(|m| self.threads.get(&m.id).map_or(0, Vec::len))
    }
}

/// The Swift harness names files by the last part of the id.
fn key(item: &MergeRequest) -> String {
    item.id.rsplit('/').next().unwrap_or(&item.iid).to_string()
}

/// A Swift-encoded `MergeRequest` as the Rust model reads it: snake_case keys, Unix dates,
/// PascalCase enums and a forge.
fn swift_item(v: Value) -> Value {
    let mut v = swift_keys(v);
    let Some(map) = v.as_object_mut() else { return v };
    for field in ["created_at", "updated_at", "requested_at", "first_requested_at", "merged_at", "closed_at"] {
        if let Some(seconds) = map.get(field).and_then(Value::as_f64) {
            let date = Utc.timestamp_millis_opt(((seconds + SWIFT_EPOCH) * 1000.0) as i64).single().unwrap_or_default();
            map.insert(field.into(), Value::String(date.to_rfc3339()));
        }
    }
    for field in ["my_state", "pipeline"] {
        if let Some(Value::String(raw)) = map.get(field) {
            let pascal = pascal(raw);
            map.insert(field.into(), Value::String(pascal));
        }
    }
    if let Some(Value::Array(reviewers)) = map.get_mut("reviewers") {
        for r in reviewers {
            if let Some(Value::String(raw)) = r.get("state") {
                let pascal = pascal(raw);
                r["state"] = Value::String(pascal);
            }
        }
    }
    map.insert("forge".into(), Value::String("gitlab".into()));
    v
}

fn swift_keys(v: Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (snake(&k), swift_keys(v))).collect::<Map<_, _>>()),
        Value::Array(list) => Value::Array(list.into_iter().map(swift_keys).collect()),
        other => other,
    }
}

fn snake(key: &str) -> String {
    if key == "webURL" {
        return "web_url".into();
    }
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// "REVIEW_STARTED" as "ReviewStarted".
fn pascal(raw: &str) -> String {
    raw.split('_')
        .map(|w| {
            let lower = w.to_ascii_lowercase();
            let mut c = lower.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

// MARK: Results

#[derive(Serialize, Deserialize)]
struct Stat {
    name: String,
    unit: String,
    samples: Vec<f64>,
}

impl Stat {
    fn sorted(&self) -> Vec<f64> {
        let mut s = self.samples.clone();
        s.sort_by(f64::total_cmp);
        s
    }
    fn median(&self) -> f64 {
        let s = self.sorted();
        s[s.len() / 2]
    }
    fn p95(&self) -> f64 {
        let s = self.sorted();
        s[(((s.len() as f64) * 0.95).ceil() as usize).saturating_sub(1).min(s.len() - 1)]
    }
    fn max(&self) -> f64 {
        self.sorted().last().copied().unwrap_or(0.0)
    }
}

fn measure(name: impl Into<String>, runs: usize, mut body: impl FnMut()) -> Stat {
    body(); // Warm up.
    let samples = (0..runs)
        .map(|_| {
            let start = Instant::now();
            body();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    Stat { name: name.into(), unit: "ms".into(), samples }
}

// MARK: Running

pub fn run(folder: &Path, label: &str) -> i32 {
    crate::data::prefs::set_ephemeral(Some(crate::data::prefs::Prefs::default()));
    let recording = match Recording::load(folder) {
        Ok(r) => r,
        Err(e) => {
            println!("error: {e}");
            return 1;
        }
    };
    let mut stats = Vec::new();

    // Pure work.
    let mut diffs = HashMap::new();
    stats.push(measure("decode + parse every diff", 7, || diffs = recording.diffs()));
    let files: Vec<&FileDiff> = diffs.values().flatten().collect();
    let lines: usize = files.iter().map(|f| f.lines.len()).sum();
    stats.push(measure(format!("highlight every diff ({} files, {lines} lines)", files.len()), 7, || {
        for file in &files {
            std::hint::black_box(crate::ui::diff::style_file(file));
        }
    }));
    stats.push(measure("  … of which lexing alone", 7, || {
        for file in &files {
            if let Some(lang) = rune_kit::syntax::lang_for_path(&file.new_path) {
                let code: Vec<&str> = file.lines.iter().map(|l| l.text.as_str()).collect();
                std::hint::black_box(rune_kit::syntax::highlight(Some(lang), &code.join("\n")));
            }
        }
    }));
    let big = synthetic_file(6000);
    let tokens: usize = crate::ui::diff::style_file(&big).iter().map(|l| l.tokens.len()).sum();
    println!("the 6,000-line diff lexes into {tokens} tokens");
    stats.push(measure("highlight one 6,000-line TypeScript diff", 7, || {
        std::hint::black_box(crate::ui::diff::style_file(&big));
    }));
    let pairs = synthetic_pairs(2000);
    stats.push(measure("word diff 2,000 long line pairs", 7, || {
        for (a, b) in &pairs {
            std::hint::black_box(styler::changed_ranges(a, b));
        }
    }));
    let texts: Vec<&str> = recording
        .all()
        .map(|m| m.description.as_str())
        .chain(recording.threads.values().flat_map(|t| t.iter().flat_map(|d| d.notes.iter().map(|n| n.body.as_str()))))
        .collect();
    stats.push(measure(format!("parse markdown ({} descriptions and notes)", texts.len()), 7, || {
        for text in &texts {
            for section in markup::parse(text) {
                match section {
                    markup::Section::Markdown(source) => {
                        std::hint::black_box(rune_kit::markdown::parse(&source));
                    }
                    markup::Section::Details { summary, body, .. } => {
                        std::hint::black_box(rune_kit::markdown::parse(&summary));
                        std::hint::black_box(rune_kit::markdown::parse(&body));
                    }
                }
            }
        }
    }));
    let mut store = Store::default();
    recording.fill(&mut store);
    stats.push(measure("sidebar sections, filtered (×200)", 7, || {
        store.filter = "e".into();
        for _ in 0..200 {
            std::hint::black_box(store.sections());
        }
        store.filter.clear();
    }));

    // The window, in a headless child that logs every frame.
    match frames(folder, label, &recording, &diffs) {
        Ok(more) => stats.extend(more),
        Err(e) => println!("frames skipped: {e}"),
    }
    stats.retain(|s| !s.samples.is_empty());
    report(&stats, label, &folder.join(format!("results-{label}.json")));
    0
}

/// What the driver does on one frame, and the stat the frame counts toward.
#[derive(Clone, Serialize, Deserialize)]
pub enum Step {
    Idle,
    /// Something in the app changed (a toast, the Ask panel's working glyph), so the view is
    /// built again over whatever is open.
    Touch,
    Show {
        id: String,
        tab: Tab,
        cold: bool,
    },
    Scroll {
        id: String,
        y: f32,
    },
    Filter(String),
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Tab {
    Overview,
    Changes,
    Discussion,
}

type Plan = Vec<(Option<String>, Step)>;

fn plan(recording: &Recording, diffs: &HashMap<String, Vec<FileDiff>>) -> Plan {
    let mut p: Plan = Vec::new();
    let show = |id: &str, tab: Tab, cold: bool| Step::Show { id: id.to_string(), tab, cold };
    let first = recording.open.first().or(recording.done.first()).map(|m| m.id.clone()).unwrap_or_default();
    p.extend((0..30).map(|_| (None, Step::Idle)));

    // Switching merge requests on the overview.
    let switch = "switch merge request (overview), per frame".to_string();
    for item in recording.all().take(24) {
        p.push((Some(switch.clone()), show(&item.id, Tab::Overview, false)));
    }

    // The biggest merge request's changes, from cold: nothing lexed yet.
    let biggest = recording.biggest(diffs);
    let files = diffs.get(&biggest.id).map_or(&[][..], Vec::as_slice);
    let shown: usize = files.iter().filter(|f| f.lines.len() <= 400).map(|f| f.lines.len()).sum();
    let open = format!("open changes of the biggest ({} files, {shown} lines shown), first frame", files.len());
    for _ in 0..5 {
        p.push((None, show(&biggest.id, Tab::Overview, false)));
        p.push((Some(open.clone()), show(&biggest.id, Tab::Changes, true)));
    }

    // Scrolling those changes, 240 pt a frame, until the page's scroll watcher says it's at the end.
    p.push((None, Step::Idle));
    for i in 1..=400 {
        p.push((Some("scroll the changes, per frame".into()), Step::Scroll { id: biggest.id.clone(), y: 240.0 * i as f32 }));
    }

    // Something else changing while those changes are open (where the scrolling stopped).
    for _ in 0..60 {
        p.push((Some("redraw the open changes after a state change, per frame".into()), Step::Touch));
    }

    // The discussion with the most threads.
    if let Some(chatty) = recording.chattiest() {
        let n = recording.threads.get(&chatty.id).map_or(0, Vec::len);
        let name = format!("open discussion of the busiest ({n} threads), first frame");
        for _ in 0..5 {
            p.push((None, show(&first, Tab::Overview, false)));
            p.push((Some(name.clone()), show(&chatty.id, Tab::Discussion, false)));
        }
    }

    // Typing in the filter.
    p.push((None, show(&first, Tab::Overview, false)));
    for _ in 0..3 {
        for count in 1..=FILTER.len() {
            p.push((Some("type in the sidebar filter, per keystroke".into()), Step::Filter(FILTER[..count].into())));
        }
        p.push((Some("type in the sidebar filter, per keystroke".into()), Step::Filter(String::new())));
    }
    p.extend((0..10).map(|_| (None, Step::Idle)));
    p
}

thread_local! {
    /// Where the open tab's page is scrolled to, and how far it goes.
    static SCROLLED: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
}

/// The open tab's scroll watcher, while the bench runs.
pub fn note_scroll(at: rune::ScrollPos) {
    SCROLLED.with(|c| c.set((at.offset[1], at.max[1])));
}

/// Runs the plan in the window, one step a frame, noting each step's frame clock.
pub struct Driver {
    plan: Plan,
    at: usize,
    scrolled: bool,
    marks: Vec<(String, f64)>,
    out: PathBuf,
}

impl Driver {
    pub fn tick(&mut self, s: &mut App, clock: f64, tx: &mut Tx) -> bool {
        let Some((name, step)) = self.plan.get(self.at).cloned() else {
            if !self.marks.is_empty() {
                let _ = std::fs::write(&self.out, serde_json::to_vec(&self.marks).unwrap_or_default());
                self.marks.clear();
            }
            return false;
        };
        if let Step::Scroll { .. } = step {
            // The page reached its end on the last frame: go on to what comes after scrolling.
            let (offset, max) = SCROLLED.with(|c| c.get());
            if max > 0.0 && offset >= max - 0.5 {
                if !self.scrolled {
                    self.scrolled = true;
                    self.marks.push(("@scrolled".into(), offset as f64));
                    self.marks.push(("@page height".into(), max as f64));
                }
                while matches!(self.plan.get(self.at), Some((_, Step::Scroll { .. }))) {
                    self.at += 1;
                }
                return self.tick(s, clock, tx);
            }
        }
        self.at += 1;
        if let Some(name) = name {
            self.marks.push((name, clock));
        }
        match step {
            Step::Idle | Step::Touch => {}
            Step::Show { id, tab, cold } => {
                if cold {
                    crate::ui::diff::forget_styles();
                }
                s.detail_tabs.insert(
                    id.clone(),
                    match tab {
                        Tab::Overview => detail::Tab::Overview,
                        Tab::Changes => detail::Tab::Changes,
                        Tab::Discussion => detail::Tab::Conversation,
                    },
                );
                if s.store.selected_id.as_deref() != Some(&id) {
                    s.store.select(Some(id));
                }
            }
            Step::Scroll { id, y } => {
                // Like a person scrolling: the page moves, nothing in the app's state changes.
                tx.scroll_to_instant(detail::scroll_key(&id, s.tab(&id)), 0.0, y);
                tx.wake_after(Duration::from_millis(16));
                return false;
            }
            Step::Filter(text) => {
                s.filter = TextEdit::new(&text);
                s.store.filter = text;
            }
        }
        tx.wake_after(Duration::from_millis(16));
        true
    }
}

/// `--scene bench`: the recording in `MROW_BENCH_DIR`, with the driver that `MROW_BENCH_MARKS`
/// asks for.
pub fn scene(s: &mut App) {
    let Some(folder) = std::env::var_os("MROW_BENCH_DIR").map(PathBuf::from) else { return };
    let recording = match Recording::load(&folder) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("bench: {e}");
            return;
        }
    };
    recording.fill(&mut s.store);
    let diffs = recording.diffs();
    s.store.select(recording.open.first().map(|m| m.id.clone()));
    if let Some(out) = std::env::var_os("MROW_BENCH_MARKS") {
        s.bench = Some(Driver { plan: plan(&recording, &diffs), at: 0, scrolled: false, marks: Vec::new(), out: out.into() });
    }
}

#[derive(Deserialize)]
struct FrameRow {
    k: Option<usize>,
    ms: Option<f64>,
    end: Option<bool>,
    resident: Option<f64>,
}

fn frames(folder: &Path, label: &str, recording: &Recording, diffs: &HashMap<String, Vec<FileDiff>>) -> Result<Vec<Stat>, String> {
    if !cfg!(feature = "profile") {
        return Err("build with `--profile perf --features profile` to time frames".into());
    }
    let steps = plan(recording, diffs).len();
    let log = folder.join(format!("frames-{label}.jsonl"));
    let marks = folder.join(format!("marks-{label}.json"));
    let at = (steps as f64 + 30.0) / FPS;
    let status = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["--scene", "bench", "--size", "1280x820", "--at", &format!("{at:.2}"), "--snap"])
        .arg(folder.join(format!("check-{label}.png")))
        .env("MROW_BENCH_DIR", folder)
        .env("MROW_BENCH_MARKS", &marks)
        .env("RUNE_FRAME_LOG", &log)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("the headless run failed ({status})"));
    }
    let rows: Vec<FrameRow> = std::fs::read_to_string(&log).map_err(|e| e.to_string())?.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let by_k: HashMap<usize, &FrameRow> = rows.iter().filter_map(|r| Some((r.k?, r))).collect();
    let marks: Vec<(String, f64)> = serde_json::from_slice(&std::fs::read(&marks).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut order: Vec<String> = Vec::new();
    let mut samples: HashMap<String, Vec<f64>> = HashMap::new();
    for (name, clock) in marks {
        if let Some(what) = name.strip_prefix('@') {
            println!("{what}: {clock:.0} pt");
            continue;
        }
        let Some(row) = by_k.get(&((clock * FPS).round() as usize)) else { continue };
        let Some(ms) = row.ms else { continue };
        if !order.contains(&name) {
            order.push(name.clone());
        }
        samples.entry(name).or_default().push(ms);
    }
    let mut stats: Vec<Stat> = order.into_iter().map(|name| Stat { samples: samples.remove(&name).unwrap_or_default(), name, unit: "ms".into() }).collect();
    if let Some(resident) = rows.iter().find(|r| r.end == Some(true)).and_then(|r| r.resident) {
        stats.push(Stat { name: "memory after the run".into(), unit: "MB".into(), samples: vec![resident / 1_048_576.0] });
    }
    Ok(stats)
}

fn report(stats: &[Stat], label: &str, to: &Path) {
    println!("\n{label}");
    for stat in stats {
        let line = if stat.unit == "MB" {
            format!("{:8.1} MB", stat.median())
        } else {
            format!("{:9.2} ms median  {:9.2} p95  {:9.2} max  (n={})", stat.median(), stat.p95(), stat.max(), stat.samples.len())
        };
        println!("  {:<78} {line}", stat.name);
    }
    if std::fs::write(to, serde_json::to_vec_pretty(stats).unwrap_or_default()).is_ok() {
        println!("wrote {}", to.display());
    }
}

/// The Swift harness's long TypeScript diff: mostly context, with edits, comments and template
/// strings spread through.
fn synthetic_file(lines: usize) -> FileDiff {
    let mut diff = format!("@@ -1,{lines} +1,{lines} @@\n");
    for i in 0..lines {
        diff += &match i % 10 {
            0 => format!(" /** Handles item {i}. Spans\n"),
            1 => "  * two lines. */\n".to_string(),
            3 => format!("-export const value{i} = compute(\"old\", {i}, {{ retries: 3 }});\n"),
            4 => format!("+export const value{i} = compute(\"new\", {}, {{ retries: 5, backoff: true }});\n", i * 2),
            6 => format!(" const message = `item ${{value{i}}} is ${{\"ready\"}}`;\n"),
            _ => format!(" function handler{i}(input: Record<string, number>): Promise<void> {{ return run(input); }}\n"),
        };
    }
    let json = serde_json::json!([{
        "old_path": "big.ts", "new_path": "big.ts", "diff": diff,
        "new_file": false, "renamed_file": false, "deleted_file": false,
    }]);
    gitlab::file_diffs_from_json(json.to_string().as_bytes()).expect("synthetic diff").remove(0)
}

fn synthetic_pairs(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            let words: Vec<String> = (0..45).map(|j| format!("word{}", (j * 7 + i) % 13)).collect();
            let mut changed = words.clone();
            changed[i % 45] = "changed".into();
            changed[(i * 3) % 45] = "edited".into();
            (words.join(" "), changed.join(" "))
        })
        .collect()
}
