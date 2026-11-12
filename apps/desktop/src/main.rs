//! Mrow: the merge requests and pull requests waiting on your review, from GitLab and GitHub,
//! drawn with Rune.
//!
//! `mrow` opens the window. `mrow --queue gitlab|github` prints a live, read-only look at a
//! forge's mapping. Rune's own flags render headlessly: `mrow --snap out.png --scene changes
//! --theme dark` (scenes use made-up data and never touch your settings). `mrow --bench <folder>`
//! times the app against a recording from the Swift app's `--bench-record` (see `bench.rs`).

mod app;
mod bench;
mod data;
mod fixtures;
mod platform;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};

use rune::{AppSpec, Chrome, Tx, Waker};

use app::{App, DetailTab};
use data::ask;
use data::forge::Forge;
use data::prefs;
use data::store::{QueueMode, Viewed};

/// Set for headless runs (snapshots, trees, scripts), which use fixtures and stay offline.
static HEADLESS: AtomicBool = AtomicBool::new(false);

fn setup(s: &mut App, waker: Waker) {
    s.store.bg.set_waker(waker.clone());
    s.images.connect(waker.clone());
    s.ask.set_waker(waker.clone());
    s.updater.set_waker(waker.clone());
    if HEADLESS.load(Ordering::Relaxed) {
        s.images.offline = std::env::var_os("MROW_ONLINE_IMAGES").is_none();
        s.ask.offline = true;
        s.ask.remembers_sessions = false;
        s.updater.offline = true;
    } else {
        platform::listen(waker);
        platform::apply_appearance();
    }
}

fn poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    s.now = clock;
    let mut changed = false;
    if let Some(mut driver) = s.bench.take() {
        changed |= driver.tick(s, clock, tx);
        s.bench = Some(driver);
    }
    if !s.started {
        s.started = true;
        if !s.store.offline && !HEADLESS.load(Ordering::Relaxed) {
            s.store.refresh();
            changed = true;
        }
    }
    if let Some(id) = platform::take_clicked() {
        tx.instant();
        s.store.set_mode(QueueMode::ToReview);
        s.store.select(Some(id));
        rune_notify::bring_to_front();
        changed = true;
    }
    let (landed, effects) = s.store.poll(clock);
    changed |= landed;
    changed |= s.settle_posts();
    changed |= s.images.poll();
    changed |= fold_viewed(s);
    if s.ask_visible {
        if let Some(item) = s.store.selected().cloned() {
            s.ask.prepare(&item);
            // A reopened chat's drafted comments need the diff to find their lines.
            if s.ask.chat(&item.id).is_some_and(|c| !c.entries.is_empty()) {
                s.store.load_details_if_needed(&item);
            }
            let agent = s.ask.agent_for(&item.id);
            s.ask.load_models(agent, false);
        }
    }
    let (asked, ask_wake) = s.ask.poll(&mut s.store);
    changed |= asked;
    if ui::ask::animating(s) {
        // The working glyph and its timer.
        tx.wake_after(std::time::Duration::from_millis(120));
        changed = true;
    }
    if let Some(at) = ask_wake {
        // The wall clock's deadline as a frame-clock time.
        tx.wake_at(clock + (at - crate::data::ask::wall()).max(0.05));
    }
    if let Some(badge) = effects.badge {
        if s.badge != Some(badge) && !HEADLESS.load(Ordering::Relaxed) {
            platform::set_badge(badge);
        }
        s.badge = Some(badge);
    }
    if !effects.notify.is_empty() {
        platform::notify(&effects.notify);
    }
    if let Some(at) = effects.wake_at {
        tx.wake_at(at);
    }
    let (updated, say, check_at) = s.updater.poll(clock, prefs::read(|p| p.auto_update));
    changed |= updated;
    if let Some(line) = say {
        s.store.show(&line);
        changed = true;
    }
    if let Some(at) = check_at {
        tx.wake_at(at);
    }
    if matches!(&s.copied, Some((_, until)) if *until <= clock) {
        s.copied = None;
        changed = true;
    }
    // The queue refreshes on its own every few minutes.
    let minutes = prefs::read(|p| p.refresh_minutes);
    if minutes > 0 && !s.store.offline {
        let every = minutes as f64 * 60.0;
        if s.next_refresh == 0.0 {
            s.next_refresh = clock + every;
        } else if clock >= s.next_refresh {
            s.store.refresh();
            s.next_refresh = clock + every;
            changed = true;
        }
        tx.wake_at(s.next_refresh);
    }
    changed
}

/// Folds the files already marked viewed, the first time a merge request's changes arrive.
fn fold_viewed(s: &mut App) -> bool {
    let Some(item) = s.store.selected().cloned() else { return false };
    if s.folded_viewed.contains(&item.id) {
        return false;
    }
    let Some(files) = s.store.diffs.get(&item.id).and_then(|l| l.value()) else { return false };
    let keys: Vec<String> = files.iter().filter(|f| s.store.viewed(f, &item) == Viewed::Yes).map(|f| format!("{}|{}", item.id, f.id())).collect();
    s.folded_viewed.insert(item.id.clone());
    let any = !keys.is_empty();
    s.collapsed.extend(keys);
    any
}

// MARK: Scenes

fn fixture(s: &mut App) {
    let (accounts, diffs, threads) = fixtures::load();
    s.store.load_fixture(accounts, diffs, threads);
    s.store.forge = Forge::Gitlab;
    s.store.tabs = vec![];
    let first = s.store.visible_items().first().map(|m| m.id.clone());
    s.store.select(first);
}

fn scene_to_review(s: &mut App) {
    fixture(s);
}

fn scene_changes(s: &mut App) {
    fixture(s);
    s.set_tab(DetailTab::Changes);
}

fn scene_discussion(s: &mut App) {
    fixture(s);
    s.set_tab(DetailTab::Discussion);
}

fn scene_tabs(s: &mut App) {
    fixture(s);
    let ids: Vec<String> = s.store.visible_items().iter().take(2).map(|m| m.id.clone()).collect();
    for id in &ids {
        s.store.open_tab(id, false);
    }
    if let Some(id) = ids.first() {
        s.store.select(Some(id.clone()));
    }
    s.details.insert(format!("{}#1", ids[0]), true);
}

fn scene_line_comment(s: &mut App) {
    scene_changes(s);
    let id = s.store.selected_id.clone().unwrap_or_default();
    let file = s.store.diffs.get(&id).and_then(|l| l.value()).map(|f| f[0].id()).unwrap_or_default();
    s.line_comment = Some(app::DraftKey::Line(id, file, 8));
}

fn scene_big_diff(s: &mut App) {
    scene_changes(s);
    let id = s.store.selected_id.clone().unwrap_or_default();
    s.store.diffs.insert(id, crate::data::store::Loadable::Loaded(std::rc::Rc::new(fixtures::big_diff(40, 300))));
}

fn scene_ask(s: &mut App) {
    fixture(s);
    s.ask_visible = true;
    let id = s.store.selected_id.clone().unwrap_or_default();
    s.ask.insert_fixture(&id, Some("/Users/robin/code/ledger".into()), vec![], ask::Phase::Idle);
}

fn scene_ask_chat(s: &mut App) {
    use ask::EntryKind as K;
    fixture(s);
    s.ask_visible = true;
    let id = s.store.selected_id.clone().unwrap_or_default();
    s.ask.insert_fixture(
        &id,
        Some("/Users/robin/code/ledger".into()),
        vec![
            (K::Question, "What could break, and where should I look first?", 60.0),
            (K::Context, "Description, 3 changed files, 2 threads", 60.0),
            (K::Thought, "The job now holds a cursor across batches. If the replica lags, a batch could miss rows written after the cursor was taken, so the export would be short without any error.", 52.0),
            (K::Tool { kind: "read".into(), done: Some(true) }, "Read src/export/job.rs (1 - 80)", 40.0),
            (K::Tool { kind: "search".into(), done: Some(true) }, "Grep \"Cursor::after\" in src", 30.0),
            (K::Answer, "Two places deserve a look first.\n\n1. **Replica lag.** `ExportJob` takes its cursor from the replica, so rows written in the last second or two can be skipped. Nothing reports it, the file is just short.\n2. **The batch size.** `Batch::rows(5_000)` is fixed; a ledger with very wide rows could still hold the connection for a while.\n\n```src/export/job.rs lines 28–31\nlet rows = replica.rows(self.ledger, self.cursor.after(), self.batch)?;\nwriter.write_all(&rows)?;\nself.cursor = rows.last_cursor();\n```\n\nI'd add a check that compares the exported count with the primary's once the job ends.", 20.0),
            (K::Question, "Is there a test for the empty ledger case?", 6.0),
            (K::Tool { kind: "read".into(), done: None }, "Read tests/export_test.rs", 2.0),
        ],
        ask::Phase::Working,
    );
}

/// An answer that drafts two comments, one on a diff line, waiting for the user to post them.
fn scene_ask_comment(s: &mut App) {
    use ask::EntryKind as K;
    fixture(s);
    s.ask_visible = true;
    let id = s.store.selected_id.clone().unwrap_or_default();
    s.ask.insert_fixture(
        &id,
        Some("/Users/robin/code/ledger".into()),
        vec![
            (K::Question, "Draft comments for the replica lag problem.", 40.0),
            (K::Context, "Description, 3 changed files, 2 threads", 40.0),
            (K::Tool { kind: "read".into(), done: Some(true) }, "Read src/export/job.rs (1 - 80)", 30.0),
            (
                K::Answer,
                "One on the cursor, and one for the whole change:\n\n```comment src/export/job.rs:15\nThe cursor comes from the replica, so rows written in the last second or two before a batch can be skipped. Could the job take its first cursor from the primary?\n```\n\n```comment\nThe export can come out short without an error when the replica lags. A check that compares the exported row count with the primary's when the job ends would catch it.\n```",
                10.0,
            ),
        ],
        ask::Phase::Idle,
    );
}

fn scene_reviewed(s: &mut App) {
    fixture(s);
    s.store.set_mode(QueueMode::Reviewed);
}

fn scene_done(s: &mut App) {
    fixture(s);
    s.store.set_mode(QueueMode::Done);
}

fn scene_github(s: &mut App) {
    fixture(s);
    s.store.switch_forge(Forge::Github);
}

fn scene_empty(s: &mut App) {
    s.store.offline = true;
}

fn scene_no_sidebar(s: &mut App) {
    fixture(s);
    s.sidebar_visible = false;
}

fn scene_settings(s: &mut App) {
    fixture(s);
    s.settings_open = true;
}

fn scene_update(s: &mut App) {
    fixture(s);
    s.updater.insert_fixture("0.2.0");
    s.settings_open = true;
}

fn scene_toast(s: &mut App) {
    fixture(s);
    s.store.show("Copied maya/batched-exports");
}

const SCENES: &[(&str, fn(&mut App))] = &[
    ("to-review", scene_to_review),
    ("changes", scene_changes),
    ("discussion", scene_discussion),
    ("tabs", scene_tabs),
    ("line-comment", scene_line_comment),
    ("reviewed", scene_reviewed),
    ("done", scene_done),
    ("github", scene_github),
    ("empty", scene_empty),
    ("no-sidebar", scene_no_sidebar),
    ("settings", scene_settings),
    ("toast", scene_toast),
    ("update", scene_update),
    ("big-diff", scene_big_diff),
    ("ask", scene_ask),
    ("ask-chat", scene_ask_chat),
    ("ask-comment", scene_ask_comment),
    ("bench", bench::scene),
];

/// `--queue gitlab|github`: a live, read-only look at what a forge's mapping makes of your queue.
fn dump_queue(name: &str) -> i32 {
    let Some(forge) = Forge::from_raw(&name.to_lowercase()) else {
        println!("unknown forge {name}; use gitlab or github");
        return 2;
    };
    let queue = match forge.queue(14) {
        Ok(q) => q,
        Err(e) => {
            println!("error: {e}");
            return 1;
        }
    };
    println!("{} as {} (@{})", forge.name(), queue.me.name, queue.me.username);
    println!("open: {}, done: {}", queue.open.len(), queue.done.len());
    for item in queue.open.iter().chain(&queue.done) {
        let reviewers: Vec<String> = item.reviewers.iter().map(|r| format!("{}:{:?}", r.person.username, r.state)).collect();
        println!(
            "  {} [{}] mine={:?} asked={} by={} guess={} +{}/-{} files={} ci={} merge={} reviewers=[{}]",
            item.full_reference(),
            item.state,
            item.my_state,
            item.requested_at.to_rfc3339(),
            item.requested_by.as_ref().map_or("?", |p| p.username.as_str()),
            item.requested_at_is_guess,
            item.additions,
            item.deletions,
            item.file_count,
            item.pipeline.map_or("-", |p| p.label()),
            item.merge_status.as_deref().unwrap_or("-"),
            reviewers.join(",")
        );
    }
    if let Some(item) = queue.open.first().or(queue.done.first()) {
        match forge.diffs(item) {
            Ok((files, refs)) => println!(
                "diffs of {}: {} files, {} lines, head={}",
                item.full_reference(),
                files.len(),
                files.iter().map(|f| f.lines.len()).sum::<usize>(),
                refs.map_or("nil".to_string(), |r| r.head_sha.chars().take(8).collect())
            ),
            Err(e) => {
                println!("error: {e}");
                return 1;
            }
        }
        match forge.discussions(item) {
            Ok(threads) => println!("threads: {}, on lines: {}", threads.len(), threads.iter().filter(|t| t.position().is_some()).count()),
            Err(e) => {
                println!("error: {e}");
                return 1;
            }
        }
    }
    0
}

/// `--ask-models [question] [--agent cursor|claude|opencode|pi]`: a live check of an agent client
/// that shares nothing: it opens a chat in an empty folder, lists the models and learns the
/// chosen model's effort levels, then asks the question if there is one.
fn dump_models() -> i32 {
    prefs::set_ephemeral(Some(prefs::get()));
    let args: Vec<String> = std::env::args().collect();
    let named = args.iter().position(|a| a == "--agent").and_then(|i| args.get(i + 1));
    let agent = match named.map(|n| ask::agents::AgentKind::from_raw(n)) {
        Some(Some(a)) => a,
        Some(None) => {
            eprintln!("--agent takes cursor, claude, opencode or pi");
            return 2;
        }
        None => ask::agents::AgentKind::Cursor,
    };
    let mut store = data::store::Store::default();
    let mut ask = ask::AskStore::default();
    ask.remembers_sessions = false;
    ask.agent = agent;
    let waker: Waker = std::sync::Arc::new(|| {});
    ask.set_waker(waker);
    ask.load_models(agent, true);
    let started = std::time::Instant::now();
    while started.elapsed().as_secs() < 90 {
        ask.poll(&mut store);
        let r = ask.roster(agent);
        if !r.loading {
            if r.models.is_empty() || r.model.is_empty() {
                break;
            }
            let model = r.model.clone();
            ask.describe(agent);
            if ask.roster(agent).efforts.contains_key(&model) {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let r = ask.roster(agent);
    if let Some(e) = &r.error {
        println!("error: {e}");
        return 1;
    }
    let featured: Vec<String> = ask::model::featured(&r.models).iter().map(|m| m.title()).collect();
    println!("{}: {} models ({:.1}s)", agent.name(), r.models.len(), started.elapsed().as_secs_f64());
    println!("featured: {}", featured.join(", "));
    match r.efforts.get(&r.model) {
        Some(e) => {
            println!("{}: effort {} = {} (of {})", r.model, e.config_id, e.current, e.values.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(", "))
        }
        None => println!("{}: no effort levels learned", r.model),
    }
    let question = args.iter().skip_while(|a| *a != "--ask-models").nth(1).filter(|q| !q.starts_with("--")).cloned();
    if let Some(question) = question {
        // A chat about nothing in particular (no merge request goes with it).
        ask.ask("selftest", &question);
        let started = std::time::Instant::now();
        let mut seen_working = false;
        while started.elapsed().as_secs() < 180 {
            ask.poll(&mut store);
            let phase = ask.chat("selftest").map(|c| c.phase);
            seen_working |= phase == Some(ask::Phase::Working);
            if seen_working && phase == Some(ask::Phase::Idle) || phase == Some(ask::Phase::Idle) && started.elapsed().as_secs() > 5 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        println!("answered in {:.1}s; permission requests declined: {}", started.elapsed().as_secs_f64(), ask::acp::agent(agent).declined());
        for e in ask.chat("selftest").map(|c| c.entries.clone()).unwrap_or_default() {
            let text: String = e.text.chars().take(160).collect();
            println!("[{:?}] {}", e.kind, text.replace('\n', " "));
        }
    }
    ask.shutdown();
    0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("mrow {}", data::update::VERSION);
        return;
    }
    if args.iter().any(|a| a == "--update") {
        std::process::exit(data::update::run_cli());
    }
    if let Some(i) = args.iter().position(|a| a == "--queue") {
        let name = args.get(i + 1).cloned().unwrap_or_default();
        std::process::exit(dump_queue(&name));
    }
    if let Some(i) = args.iter().position(|a| a == "--bench") {
        let Some(folder) = args.get(i + 1) else {
            println!("usage: mrow --bench <folder> [label]");
            std::process::exit(2);
        };
        let label = args.get(i + 2).map_or("rune", String::as_str);
        std::process::exit(bench::run(std::path::Path::new(folder), label));
    }
    if args.iter().any(|a| a == "--ask-models") {
        std::process::exit(dump_models());
    }
    let headless = args.iter().any(|a| matches!(a.as_str(), "--snap" | "--tree" | "--script" | "--list-scenes" | "--strip"));
    HEADLESS.store(headless, Ordering::Relaxed);
    if headless {
        // Snapshots draw from default settings and never write the person's.
        prefs::set_ephemeral(Some(prefs::Prefs::default()));
    } else if !rune::single_instance(if cfg!(debug_assertions) { "com.zain.mrow.debug" } else { "com.zain.mrow" }) {
        // Linux starts a process per launch: a second one hands over to the Mrow already running,
        // window closed or not, which shows its window (`on_reopen`). macOS does this itself. A
        // debug build runs beside an installed copy.
        return;
    }
    // On Linux the window reports this as its Wayland app_id / X11 WM_CLASS, so the desktop
    // matches it to packaging/linux/com.zain.mrow.desktop and its icon. Same id as the macOS bundle.
    rune::set_app_id("com.zain.mrow");
    data::update::remember_exe();
    let size = (1240.0, 820.0);
    let spec = AppSpec {
        min_size: Some((760.0, 460.0)),
        scenes: SCENES,
        fonts: ui::theme::FONTS,
        families: ui::theme::FAMILIES,
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(ui::actions::on_key),
        ..AppSpec::new("Mrow", size, App::default, ui::view)
    };
    let mut app = rune::App::new(spec)
        .chrome(Chrome { full_size_content: true, traffic_lights: Some((13.0, 13.0)), drag_regions: true })
        .standard_menus()
        // Closing the window keeps the app (and its badge) around, like Mail.
        .keep_running()
        .on_reopen(|_: &mut App, _visible: bool, _: &mut Tx| true)
        .on_quit(|s: &mut App, _: &mut Tx| s.ask.shutdown());
    for item in ui::actions::menu_items() {
        app = app.menu_item(item);
    }
    app.run();
}
