//! Cursor cites code as an empty fence headed "```200:231:src/x.ts", which its own app fills in.
//! Mrow fills it from the clone, as the clone is now.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

thread_local! {
    static FILES: RefCell<HashMap<String, Option<Rc<Vec<String>>>>> = RefCell::new(HashMap::new());
}

/// A citation fence's header: "```12:30:path" → (12, 30, path).
fn header(line: &str) -> Option<(usize, usize, &str)> {
    let rest = line.strip_prefix("```")?;
    let mut parts = rest.splitn(3, ':');
    let start: usize = parts.next()?.parse().ok()?;
    let end: usize = parts.next()?.parse().ok()?;
    let path = parts.next()?;
    if path.is_empty() || path.contains('`') {
        return None;
    }
    Some((start, end, path))
}

pub fn expand(text: &str, checkout: Option<&Path>) -> String {
    if !text.contains("```") {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((start, end, path)) = header(lines[i]) else {
            out.push(lines[i].to_string());
            i += 1;
            continue;
        };
        // The closing fence: a line that is ``` and trailing blanks.
        let Some(close) = (i + 1..lines.len()).find(|&j| lines[j].trim_end_matches([' ', '\t']) == "```") else {
            out.push(lines[i].to_string());
            i += 1;
            continue;
        };
        let end = end.max(start);
        let mut path = path.trim().to_string();
        if let Some(root) = checkout.and_then(|c| c.to_str()) {
            if let Some(rest) = path.strip_prefix(&format!("{root}/")) {
                path = rest.to_string();
            }
        }
        let mut code = if close > i + 1 { lines[i + 1..close].join("\n") + "\n" } else { String::new() };
        if code.trim().is_empty() {
            code = cited(&path, start, end, checkout).map(|c| c + "\n").unwrap_or_default();
        }
        let code = dedent(&code);
        let where_ = if start == end { format!("line {start}") } else { format!("lines {start}–{end}") };
        if code.is_empty() {
            let name = Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or(&path).to_string();
            // The reference ends its own line, as the fence it replaces did.
            out.push(format!("`{name}` · {where_}"));
            out.push(String::new());
        } else {
            out.push(format!("```{path} {where_}\n{code}```"));
        }
        i = close + 1;
    }
    out.join("\n")
}

/// Lines cut from the middle of a file keep its indentation; the block needn't.
pub fn dedent(code: &str) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    let blank = |l: &str| l.chars().all(char::is_whitespace);
    let common = lines.iter().filter(|l| !blank(l)).map(|l| l.len() - l.trim_start_matches([' ', '\t']).len()).min();
    let Some(common) = common.filter(|&c| c > 0) else { return code.to_string() };
    lines.iter().map(|l| if blank(l) { "" } else { &l[common..] }).collect::<Vec<_>>().join("\n")
}

fn cited(path: &str, start: usize, end: usize, checkout: Option<&Path>) -> Option<String> {
    if start < 1 || end < start || end - start >= 400 {
        return None;
    }
    let full = if path.starts_with('/') { Path::new(path).to_path_buf() } else { checkout?.join(path) };
    let key = full.to_string_lossy().to_string();
    let lines = FILES.with(|f| {
        f.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let meta = std::fs::metadata(&full).ok()?;
                if meta.len() >= 2_000_000 {
                    return None;
                }
                let text = std::fs::read_to_string(&full).ok()?;
                Some(Rc::new(text.split('\n').map(str::to_string).collect()))
            })
            .clone()
    })?;
    if start > lines.len() {
        return None;
    }
    Some(lines[start - 1..end.min(lines.len())].join("\n"))
}
