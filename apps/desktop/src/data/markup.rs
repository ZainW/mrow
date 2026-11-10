//! What descriptions and comments are written in: GitLab- and GitHub-flavoured markdown with
//! bits of HTML. This turns the HTML the markdown parser doesn't read into markdown (`<br>`,
//! `<b>`, `<li>`, `<img>`, `<a>`), drops comments and wrappers, and splits out `<details>`
//! sections, which get their own folding view. Code fences keep their text untouched.

/// A run of markdown, or a `<details>` section with its summary and body.
#[derive(Clone, Debug, PartialEq)]
pub enum Section {
    Markdown(String),
    Details { summary: String, open: bool, body: String },
}

pub fn parse(source: &str) -> Vec<Section> {
    sections(&clean(source))
}

/// Splits out top-level `<details>` elements, matching nested ones, and leaves the rest.
fn sections(text: &str) -> Vec<Section> {
    let mut out = Vec::new();
    let push = |s: &str, out: &mut Vec<Section>| {
        if !s.trim().is_empty() {
            out.push(Section::Markdown(s.to_string()));
        }
    };
    let mut rest = text;
    while let Some(open) = find_ci(rest, "<details") {
        let Some(tag_len) = rest[open..].find('>') else { break };
        let tag_end = open + tag_len;
        push(&rest[..open], &mut out);
        let is_open = rest[open + "<details".len()..tag_end].contains("open");
        let mut depth = 1;
        let mut cursor = tag_end + 1;
        let (mut body_end, mut after) = (rest.len(), rest.len());
        // An unclosed section runs to the end, as in a browser.
        while depth > 0 {
            let Some(close) = find_ci(&rest[cursor..], "</details>").map(|c| c + cursor) else { break };
            if let Some(nested) = find_ci(&rest[cursor..close], "<details").map(|n| n + cursor) {
                depth += 1;
                cursor = nested + "<details".len();
            } else {
                depth -= 1;
                cursor = close + "</details>".len();
                if depth == 0 {
                    body_end = close;
                    after = cursor;
                }
            }
        }
        let mut body = &rest[tag_end + 1..body_end];
        let mut summary = "Details".to_string();
        if let Some(start) = find_ci(body, "<summary") {
            if let Some(end) = body[start..].find('>').map(|e| e + start) {
                if let Some(close) = find_ci(&body[end..], "</summary>").map(|c| c + end) {
                    summary = body[end + 1..close].trim().to_string();
                    body = &body[close + "</summary>".len()..];
                }
            }
        }
        out.push(Section::Details { summary: html(&summary).trim().to_string(), open: is_open, body: body.to_string() });
        rest = &rest[after..];
    }
    push(rest, &mut out);
    out
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Line endings, comments and HTML outside code fences.
pub fn clean(source: &str) -> String {
    let mut text = if source.contains("\r\n") { source.replace("\r\n", "\n") } else { source.to_string() };
    if !text.contains('<') {
        return text;
    }
    while let Some(start) = text.find("<!--") {
        match text[start..].find("-->") {
            Some(end) => text.replace_range(start..start + end + 3, ""),
            None => text.truncate(start),
        }
    }
    if !text.contains("```") && !text.contains("~~~") {
        return html(&text);
    }
    let mut output: Vec<String> = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let mut fence: Option<&str> = None;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if let Some(open) = fence {
            output.push(line.to_string());
            if trimmed.starts_with(open) {
                fence = None;
            }
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            if !prose.is_empty() {
                output.push(html(&prose.join("\n")));
            }
            prose.clear();
            fence = Some(if trimmed.starts_with("```") { "```" } else { "~~~" });
            output.push(line.to_string());
        } else {
            prose.push(line);
        }
    }
    if !prose.is_empty() {
        output.push(html(&prose.join("\n")));
    }
    output.join("\n")
}

const TAGS: [&str; 17] = ["br", "strong", "b", "em", "i", "code", "li", "ul", "ol", "p", "div", "span", "blockquote", "sub", "sup", "picture", "source"];

/// One pass over the tags: inline ones become markdown, wrappers are dropped.
pub fn html(text: &str) -> String {
    if !text.contains('<') {
        return text.to_string();
    }
    let mut output = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut cursor = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let closing = bytes.get(i + 1) == Some(&b'/');
        let name_start = i + 1 + closing as usize;
        let mut name_end = name_start;
        while name_end < bytes.len() && bytes[name_end].is_ascii_alphanumeric() {
            name_end += 1;
        }
        let name = text[name_start..name_end].to_ascii_lowercase();
        // A word boundary after the name, then anything up to '>'.
        let boundary = matches!(bytes.get(name_end), Some(b'>' | b' ' | b'/' | b'\t' | b'\n'));
        let Some(close) = (boundary && TAGS.contains(&name.as_str())).then(|| text[name_end..].find('>')).flatten() else {
            i += 1;
            continue;
        };
        output.push_str(&text[cursor..i]);
        match name.as_str() {
            "br" => output.push_str(if in_table_row(text, i) { "\u{2028}" } else { "\n" }),
            "strong" | "b" => output.push_str("**"),
            "em" | "i" => output.push('_'),
            "code" => output.push('`'),
            "li" => {
                if !closing {
                    output.push_str("\n- ")
                }
            }
            "ul" | "ol" => output.push('\n'),
            _ => {}
        }
        i = name_end + close + 1;
        cursor = i;
    }
    output.push_str(&text[cursor..]);
    let output = markdown_for_images(&output);
    links(&output)
}

/// A `<br>` in a table row breaks the line inside its cell rather than ending the row.
fn in_table_row(text: &str, at: usize) -> bool {
    let start = text[..at].rfind('\n').map_or(0, |n| n + 1);
    text[start..at].trim_start().starts_with('|')
}

/// The value of `name="…"` in a tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name).map(|a| a + from) {
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let rest = tag[at + name.len()..].trim_start();
        if before_ok {
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('"') {
                    return rest.find('"').map(|end| rest[..end].to_string());
                }
            }
        }
        from = at + name.len();
    }
    None
}

/// `<img src alt width height>` as `![alt](src){width=… height=…}`.
pub fn markdown_for_images(text: &str) -> String {
    if find_ci(text, "<img").is_none() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = find_ci(rest, "<img") {
        let after = rest.as_bytes().get(start + 4);
        let Some(end) = rest[start..].find('>').map(|e| e + start).filter(|_| matches!(after, Some(b' ' | b'>' | b'/' | b'\n'))) else {
            out.push_str(&rest[..start + 4]);
            rest = &rest[start + 4..];
            continue;
        };
        out.push_str(&rest[..start]);
        let tag = &rest[start..=end];
        if let Some(src) = attribute(tag, "src").filter(|s| !s.is_empty()) {
            let alt = attribute(tag, "alt").unwrap_or_default();
            let size: Vec<String> = ["width", "height"].iter().filter_map(|n| attribute(tag, n).map(|v| format!("{n}={v}"))).collect();
            out.push_str(&format!("![{alt}]({src})"));
            if !size.is_empty() {
                out.push_str(&format!("{{{}}}", size.join(" ")));
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// `<a href="url">text</a>` as `[text](url)`, when the text has no tags of its own.
fn links(text: &str) -> String {
    if find_ci(text, "</a>").is_none() {
        return text.to_string();
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = find_ci(rest, "<a") {
        let ok = matches!(rest.as_bytes().get(start + 2), Some(b' ' | b'\n'));
        let tag_end = rest[start..].find('>').map(|e| e + start);
        let close = find_ci(&rest[start..], "</a>").map(|c| c + start);
        match (ok, tag_end, close) {
            (true, Some(tag_end), Some(close)) if tag_end < close => {
                let tag = &rest[start..=tag_end];
                let inner = rest[tag_end + 1..close].trim();
                match attribute(tag, "href") {
                    Some(href) if !inner.contains('<') => {
                        out.push_str(&rest[..start]);
                        out.push_str(&format!("[{inner}]({href})"));
                        rest = &rest[close + 4..];
                    }
                    _ => {
                        out.push_str(&rest[..start + 2]);
                        rest = &rest[start + 2..];
                    }
                }
            }
            _ => {
                out.push_str(&rest[..start + 2]);
                rest = &rest[start + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Image sizes written as `{width=… height=…}` after an image, removed so the markdown parser
/// doesn't print them, and kept per source for the image's frame.
pub fn strip_sizes(text: &str) -> (String, Vec<(String, Option<f32>, Option<f32>)>) {
    if !text.contains("){") {
        return (text.to_string(), Vec::new());
    }
    let mut out = String::new();
    let mut sizes = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("){") {
        let Some(end) = rest[at..].find('}').map(|e| e + at) else { break };
        let attrs = &rest[at + 2..end];
        if !attrs.split_whitespace().all(|a| a.starts_with("width=") || a.starts_with("height=")) {
            out.push_str(&rest[..at + 2]);
            rest = &rest[at + 2..];
            continue;
        }
        let src = rest[..at].rfind("](").map(|s| rest[s + 2..at].to_string()).unwrap_or_default();
        let value = |n: &str| attrs.split_whitespace().find_map(|a| a.strip_prefix(n)).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok());
        sizes.push((src, value("width="), value("height=")));
        out.push_str(&rest[..=at]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    (out, sizes)
}
