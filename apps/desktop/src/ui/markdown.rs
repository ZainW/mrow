//! Descriptions and comments: markdown drawn with rune-kit, in the reading face, with
//! `<details>` sections that fold and pictures loaded through the forge's CLI.

use rune::{El, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Markdown, MarkdownStyle};

use crate::app::App;
use crate::data::markup::{self, Section};
use crate::ui::icons as I;
use crate::ui::images;
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{MEDIUM, Pal, SEMIBOLD, mono, serif, ui};

pub struct Doc<'a> {
    /// Names the document, so its folded sections keep their state.
    pub key: &'a str,
    pub source: &'a str,
    /// The project's web URL, for relative links and uploads.
    pub base: Option<String>,
    pub size: f32,
    pub serif: bool,
}

pub fn style(p: &Pal, k: &Kit, size: f32, reading: bool) -> MarkdownStyle {
    let face = |s: f32, w: f32| if reading { serif(s, w) } else { ui(s, w) };
    MarkdownStyle::from_kit(k)
        .text(p.text)
        .muted(p.secondary)
        .link(p.accent)
        .link_underline(p.accent.alpha(0.35))
        .inline_code_bg(p.hover)
        .code_block(p.well, p.line)
        .code_block_padding(12.0, 10.0)
        .quote(p.strong_line, 2.0, 12.0, p.secondary)
        .rule(p.line)
        .body_font(face(size, 400.0).lines(1.5))
        .heading_font(1, face((size * 1.5).round(), SEMIBOLD).lines(1.25))
        .heading_font(2, face((size * 1.3).round(), SEMIBOLD).lines(1.25))
        .heading_font(3, face((size * 1.12).round(), SEMIBOLD).lines(1.3))
        .heading_font(4, face(size, SEMIBOLD).lines(1.3))
        .code_block_font(mono((size * 0.8).round().max(12.0), 400.0).lines(1.45))
        .inline_code_font(0.86, 400.0)
        .strong_weight(SEMIBOLD)
        .list(size * 1.3, size * 0.3)
        .diff(p.added_line, p.removed_line)
}

pub fn markdown(s: &App, p: &Pal, k: &Kit, doc: Doc<'_>) -> El {
    let sections = markup::parse(doc.source);
    let mut c = col().w_full().gap(doc.size * 0.75);
    for (index, section) in sections.into_iter().enumerate() {
        match section {
            Section::Markdown(text) => c = c.child(block(s, p, k, &doc, &text)),
            Section::Details { summary, open, body } => {
                let key = format!("{}#{index}", doc.key);
                let expanded = s.details.get(&key).copied().unwrap_or(open);
                c = c.child(details(s, p, k, &doc, key, &summary, expanded, &body));
            }
        }
    }
    c
}

fn block(s: &App, p: &Pal, k: &Kit, doc: &Doc<'_>, source: &str) -> El {
    let (source, _sizes) = markup::strip_sizes(source);
    let base = doc.base.clone();
    let base_for_images = doc.base.clone();
    let loaded = images::lookup(s);
    Markdown::new(source)
        .style(style(p, k, doc.size, doc.serif))
        .selectable(true)
        .images(move |src| loaded(src, base_for_images.as_deref()))
        .on_link_tx(move |_: &mut App, url: &str, tx: &mut Tx| {
            let _ = tx.open_url(&resolve_link(url, base.as_deref()));
        })
        .build(k)
        .w_full()
}

/// A link as written, made absolute against the project's web URL.
pub fn resolve_link(url: &str, base: Option<&str>) -> String {
    if url.contains("://") || url.starts_with("mailto:") {
        return url.to_string();
    }
    let Some(base) = base else { return url.to_string() };
    if let Some(path) = url.strip_prefix('/') {
        // Root-relative: on the forge's host.
        let host_end = base.find("://").map(|s| s + 3).and_then(|s| base[s..].find('/').map(|e| e + s)).unwrap_or(base.len());
        return format!("{}/{path}", &base[..host_end]);
    }
    format!("{}/{url}", base.trim_end_matches('/'))
}

#[allow(clippy::too_many_arguments)]
fn details(s: &App, p: &Pal, k: &Kit, doc: &Doc<'_>, key: String, summary: &str, expanded: bool, body: &str) -> El {
    let size = doc.size;
    let face = if doc.serif { serif(size, MEDIUM) } else { ui(size, MEDIUM) };
    let toggle_key = key.clone();
    let head = row()
        .key(format!("details-{key}"))
        .role("button")
        .label(summary.to_string())
        .gap(7.0)
        .items_center()
        .w_full()
        .child(el().w(size * 0.7).center().child(icon(if expanded { &I::CHEVRON_DOWN } else { &I::CHEVRON_RIGHT }, size * 0.7).color(p.tertiary)))
        .child(text(summary.to_string()).font(face).color(p.text).wrap().grow(1.0).shrink().min_w(0.0))
        .on_press(move |s: &mut App| {
            let now = !expanded;
            s.details.insert(toggle_key.clone(), now);
        });
    let mut c = col().w_full().gap(size * 0.6).child(head);
    if expanded {
        let inner = Doc { key: &key, source: body, base: doc.base.clone(), size, serif: doc.serif };
        c = c.child(
            row()
                .w_full()
                .pl(size * 0.3)
                .gap(size * 0.75)
                .child(el().w(2.0).self_stretch().radius(1.0).fill(p.line))
                .child(markdown(s, p, k, inner).grow(1.0).shrink().min_w(0.0)),
        );
    }
    c
}
