//! Small pieces used across the window: the header's chrome buttons, action buttons, chips,
//! avatars and the short relative times. Measurements match the Swift app's `Components`.

use chrono::{Datelike, Utc};
use rune::{El, Icon, Tx, col, el, icon, row, text};

use crate::app::App;
use crate::data::forge::Forge;
use crate::data::models::Date;
use crate::data::models::{Person, PipelineStatus, ReviewState};
use crate::ui::icons as I;
use crate::ui::theme::{BOLD, MEDIUM, Pal, SEMIBOLD, mono, ui};

/// How a chrome button is drawn: a bare icon, a text label, or a field-like box.
#[derive(Clone, Copy, PartialEq)]
pub enum Chrome {
    Icon,
    Text,
    Field,
}

/// A borderless button for the window's chrome, with a fill on hover.
pub struct ChromeButton {
    key: String,
    icon: Option<&'static Icon>,
    icon_size: f32,
    title: String,
    style: Chrome,
    hint: Option<String>,
    on: bool,
    enabled: bool,
    spinning: bool,
    width: Option<f32>,
    pad: f32,
}

impl ChromeButton {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> ChromeButton {
        ChromeButton {
            key: key.into(),
            icon: None,
            icon_size: 14.0,
            title: title.into(),
            style: Chrome::Icon,
            hint: None,
            on: false,
            enabled: true,
            spinning: false,
            width: None,
            pad: 10.0,
        }
    }
    pub fn icon(mut self, i: &'static Icon, size: f32) -> Self {
        self.icon = Some(i);
        self.icon_size = size;
        self
    }
    pub fn style(mut self, style: Chrome) -> Self {
        self.style = style;
        self
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn spinning(mut self, spinning: bool) -> Self {
        self.spinning = spinning;
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    /// The space either side of a text button's title; 10 unless several sit in a tight row.
    pub fn pad(mut self, pad: f32) -> Self {
        self.pad = pad;
        self
    }

    pub fn build(self, p: &Pal) -> El {
        let field = self.style == Chrome::Field;
        let rest = if !self.enabled {
            p.tertiary
        } else if field || !self.on {
            p.secondary
        } else {
            p.text
        };
        let hovered = if self.enabled && !field { p.text } else { rest };
        let mut b = row().key(self.key).role("button").label(self.title.clone()).h(28.0).gap(7.0).center_y().radius(7.0).hover_group();
        if let Some(i) = self.icon {
            let mut glyph = icon(i, self.icon_size).color(rest).group_color(hovered);
            if self.spinning {
                glyph = glyph.key("spin").looping(rune::LoopSpec::rotation(1.0));
            }
            let slot = el().w(if self.style == Chrome::Icon { 28.0 } else { self.icon_size }).h(28.0).center().child(glyph);
            b = b.child(slot);
        }
        match self.style {
            Chrome::Icon => {
                b = b.w(28.0);
            }
            Chrome::Text | Chrome::Field => {
                b = b.px(self.pad).child(
                    text(self.title.clone())
                        .font(ui(13.0, if field { 400.0 } else { MEDIUM }).features("'tnum' 1"))
                        .color(rest)
                        .group_color(hovered)
                        .single_line(),
                );
                if let Some(h) = &self.hint {
                    b = b.child(el().grow(1.0).min_w(14.0)).child(text(h.clone()).font(mono(12.0, 400.0)).color(p.tertiary));
                }
            }
        }
        if let Some(w) = self.width {
            b = b.w(w);
        }
        let fill = match self.style {
            Chrome::Field => Some(p.hover),
            _ if self.on && self.enabled => Some(p.hover),
            _ => None,
        };
        if let Some(f) = fill {
            b = b.fill(f);
        }
        let tip = match self.hint {
            Some(h) => format!("{}  {h}", self.title),
            None => self.title,
        };
        b = b.help(tip);
        if self.enabled {
            b = b.hover(if field { p.selected } else { p.hover }).press(p.hover);
        } else {
            b = b.disabled(true);
        }
        b
    }
}

/// A shortcut hint written with the Mac's glyphs ("⇧⌘A", "⌘↩ to post"), as this platform labels
/// it: unchanged on macOS, "Ctrl+Shift+A" and "Ctrl+Enter to post" elsewhere, where Ctrl stands
/// in for both ⌘ and ⌃ (see `actions::COMMANDS`).
pub fn keys(mac: &str) -> String {
    if cfg!(target_os = "macos") { mac.to_string() } else { off_mac(mac) }
}

fn off_mac(mac: &str) -> String {
    let mut out = String::new();
    let mut chars = mac.chars().peekable();
    while let Some(c) = chars.next() {
        if !matches!(c, '⌃' | '⌥' | '⇧' | '⌘') {
            out.push_str(if c == '↩' { "Enter" } else { "" });
            if c != '↩' {
                out.push(c);
            }
            continue;
        }
        let (mut ctrl, mut alt, mut shift) = (false, false, false);
        let mut m = Some(c);
        while let Some(g) = m {
            match g {
                '⌃' | '⌘' => ctrl = true,
                '⌥' => alt = true,
                _ => shift = true,
            }
            m = chars.next_if(|n| matches!(n, '⌃' | '⌥' | '⇧' | '⌘'));
        }
        for (on, name) in [(ctrl, "Ctrl+"), (alt, "Alt+"), (shift, "Shift+")] {
            if on {
                out.push_str(name);
            }
        }
        match chars.next() {
            Some('↩') => out.push_str("Enter"),
            Some(',') => out.push_str(","),
            Some(k) => out.extend(k.to_uppercase()),
            None => {}
        }
    }
    out
}

/// A rounded button with an icon and title; `prominent` fills it with the accent.
pub fn action_button(p: &Pal, key: impl Into<String>, glyph: Option<&'static Icon>, title: &str, prominent: bool, enabled: bool) -> El {
    // A disabled primary button goes neutral rather than a dimmed accent, which reads as muddy.
    let prominent = prominent && enabled;
    let ink = if prominent {
        p.paper
    } else if enabled {
        p.text
    } else {
        p.tertiary
    };
    let mut b = row().key(key.into()).role("button").label(title.to_string()).h(30.0).px(12.0).gap(6.0).center_y().radius(8.0).fill(if prominent {
        p.accent
    } else {
        p.elevated
    });
    if !prominent {
        b = b.border(1.0, p.line);
    }
    if let Some(i) = glyph {
        b = b.child(icon(i, 13.0).color(ink));
    }
    b = b.child(text(title.to_string()).font(ui(13.0, SEMIBOLD)).color(ink).single_line());
    if enabled {
        let over = if prominent { p.paper.alpha(0.12) } else { p.hover };
        b.hover(over).press(over).press_scale(0.98)
    } else {
        b.disabled(true)
    }
}

/// A small rounded tag: branches, labels, states.
pub fn chip(p: &Pal, glyph: Option<&'static Icon>, label: &str, tint: rune::Color, fill: Option<rune::Color>, is_mono: bool) -> El {
    let mut c = row().h(22.0).px(8.0).gap(5.0).center_y().radius(6.0).fill(fill.unwrap_or(p.hover));
    if let Some(i) = glyph {
        c = c.child(icon(i, 11.0).color(tint));
    }
    let font = if is_mono { mono(12.0, 400.0) } else { ui(12.0, MEDIUM) };
    c.child(text(label.to_string()).font(font).color(tint).single_line())
}

/// A person's initials on a soft accent disc.
/// A person's avatar from the forge, or their initials while it loads (or without one).
pub fn avatar(s: &App, p: &Pal, person: &Person, size: f32) -> El {
    let base = el()
        .size(size, size)
        .radius(size / 2.0)
        .fill(p.accent_soft)
        .center()
        .help(person.name.clone())
        .child(text(person.initials()).font(ui((size * 0.42).round(), SEMIBOLD)).color(p.accent).single_line());
    let picture = avatar_url(person).and_then(|url| crate::ui::images::lookup(s)(&url, None)).filter(|i| i.error().is_none());
    let base = match picture {
        Some(image) => base.child(el().absolute(0.0, 0.0).size(size, size).radius(size / 2.0).pixels(image.pixels()).a11y_hidden()),
        None => base,
    };
    // The ring over the picture, as the Swift app's stroked circle.
    base.child(el().absolute(0.0, 0.0).size(size, size).radius(size / 2.0).border(1.0, p.line).inert())
}

/// An avatar's address: as given, or on the GitLab host when it's a path.
fn avatar_url(person: &Person) -> Option<String> {
    let raw = person.avatar_url.as_deref().filter(|r| !r.is_empty())?;
    if raw.starts_with("http") {
        return Some(raw.to_string());
    }
    let host = crate::data::prefs::read(|p| p.hostname.trim().to_string());
    Some(format!("https://{}{raw}", if host.is_empty() { "gitlab.com" } else { &host }))
}

/// The forge's monogram on its colour: GitLab's orange, GitHub's ink.
pub fn forge_mark(p: &Pal, forge: Forge, size: f32) -> El {
    let (fill, ink) = match forge {
        Forge::Gitlab => (rune::color::hex("#E24329").unwrap(), rune::color::hex("#FFFFFF").unwrap()),
        Forge::Github => (p.text, p.paper),
    };
    el().size(size, size)
        .radius(size * 0.28)
        .fill(fill)
        .center()
        .a11y_hidden()
        .child(text(forge.monogram()).font(ui((size * 0.42).round(), BOLD).tracking(-0.2)).color(ink).single_line())
}

pub fn pipeline_color(p: &Pal, status: PipelineStatus) -> rune::Color {
    match status {
        PipelineStatus::Success => p.green,
        PipelineStatus::Failed => p.red,
        PipelineStatus::Running | PipelineStatus::Pending => p.amber,
        PipelineStatus::Canceled | PipelineStatus::Skipped | PipelineStatus::Manual => p.tertiary,
    }
}

pub fn pipeline_dot(p: &Pal, status: PipelineStatus) -> El {
    el().size(7.0, 7.0).radius(3.5).fill(pipeline_color(p, status)).help(format!("Pipeline: {}", status.label()))
}

pub fn state_color(p: &Pal, state: ReviewState) -> rune::Color {
    match state {
        ReviewState::Approved => p.green,
        ReviewState::RequestedChanges => p.red,
        ReviewState::Unapproved => p.amber,
        ReviewState::Reviewed | ReviewState::ReviewStarted => p.blue,
        ReviewState::Unreviewed | ReviewState::Unknown => p.tertiary,
    }
}

/// The review state's mark, as the Swift app's SF Symbols drew it.
pub fn state_icon(p: &Pal, state: ReviewState, size: f32) -> El {
    let c = state_color(p, state);
    match state {
        ReviewState::Approved => filled_mark(&I::DISC, &I::SMALL_CHECK, c, p.paper, size),
        ReviewState::Unreviewed => icon(&I::DASHED_CIRCLE, size).color(c),
        ReviewState::ReviewStarted => icon(&I::LEFT_HALF_CIRCLE, size).color(c),
        ReviewState::Reviewed => icon(&I::BUBBLE_TEXT, size).color(c),
        ReviewState::RequestedChanges => icon(&I::EXCLAIM_CIRCLE, size).color(c),
        ReviewState::Unapproved => icon(&I::UNDO_CIRCLE, size).color(c),
        ReviewState::Unknown => icon(&I::PLAIN_CIRCLE, size).color(c),
    }
}

/// A filled shape with a mark knocked out of it in `ink`, like `checkmark.circle.fill`.
pub fn filled_mark(fill: &'static Icon, mark: &'static Icon, color: rune::Color, ink: rune::Color, size: f32) -> El {
    el().size(size, size).child(icon(fill, size).color(color).absolute(0.0, 0.0)).child(icon(mark, size).color(ink).absolute(0.0, 0.0))
}

/// Every count of comment threads: a bubble and the number, nothing for none. A reply you
/// haven't seen fills the bubble and turns both clay, so the state isn't carried by colour alone.
pub fn comment_count(p: &Pal, n: usize, fresh: bool) -> Option<El> {
    if n == 0 {
        return None;
    }
    let (glyph, tint) = if fresh { (&I::BUBBLE_12, p.accent) } else { (&I::BUBBLE_LINE_12, p.secondary) };
    let threads = if n == 1 { "1 thread".to_string() } else { format!("{n} threads") };
    let tip = if fresh { format!("{threads}, with a reply you haven’t seen") } else { threads };
    Some(
        row()
            .gap(3.0)
            .center_y()
            .help(tip)
            .child(icon(glyph, 12.0).color(tint))
            .child(text(n.to_string()).font(mono(12.0, MEDIUM).features("'tnum' 1")).color(tint).single_line()),
    )
}

/// A 1-point rule, snapped to whole device pixels as a CSS border is.
pub fn rule(p: &Pal) -> El {
    rune::rule(1.0, p.line).w_full()
}

/// A thin rounded bar filled to `fraction`; green when full.
pub fn progress_bar(p: &Pal, fraction: f32, width: f32) -> El {
    let f = fraction.clamp(0.0, 1.0);
    el().w(width).h(4.0).radius(2.0).fill(p.line).child(el().key("bar").w(width * f).h(4.0).radius(2.0).fill(if f >= 1.0 { p.green } else { p.accent }))
}

/// A small copy button: copies `value`, then shows a green check for a moment.
pub fn copy_button(s: &App, p: &Pal, key: &str, value: String, label: &'static str) -> El {
    let copied = s.copied(key);
    let what = key.to_string();
    row()
        .key(format!("copy-{key}"))
        .role("button")
        .label(label)
        .size(22.0, 22.0)
        .center()
        .radius(6.0)
        .hover_group()
        .hover(p.hover)
        .help(if copied { "Copied" } else { label })
        .child(icon(if copied { &I::CHECK } else { &I::COPY }, 12.0).color(if copied { p.green } else { p.tertiary }).group_color(if copied {
            p.green
        } else {
            p.text
        }))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.copy(value.clone());
            s.copied = Some((what.clone(), tx.clock() + 1.5));
            tx.wake_after(std::time::Duration::from_millis(1550));
        })
}

/// "now", "4m", "3h", "2d", then "Oct 3": short enough for a sidebar row.
pub fn short_time(date: Date) -> String {
    let seconds = (Utc::now() - date).num_seconds().max(0);
    if seconds < 60 {
        "now".into()
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h", seconds / 3600)
    } else if seconds < 86_400 * 14 {
        format!("{}d", seconds / 86_400)
    } else {
        month_day(date)
    }
}

pub fn month_day(date: Date) -> String {
    let local = date.with_timezone(&chrono::Local);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {}", MONTHS[local.month0() as usize], local.day())
}

/// "Oct 3, 2025 at 14:05", for tooltips.
pub fn long_date(date: Date) -> String {
    let local = date.with_timezone(&chrono::Local);
    format!("{}, {} at {}", month_day(date), local.year(), local.format("%H:%M"))
}

/// A column of lines of text, centred, for empty states.
pub fn message(p: &Pal, glyph: Option<&'static Icon>, lines: &[&str]) -> El {
    let mut c = col().gap(10.0).items_center().padding(24.0);
    if let Some(i) = glyph {
        c = c.child(icon(i, 20.0).color(p.secondary));
    }
    for line in lines {
        c = c.child(text(line.to_string()).font(ui(13.0, 400.0)).color(p.secondary).wrap().max_w(280.0));
    }
    c
}

#[cfg(test)]
mod tests {
    #[test]
    fn mac_key_glyphs_read_as_words_off_mac() {
        assert_eq!(super::off_mac("Approve  ⇧⌘A"), "Approve  Ctrl+Shift+A");
        assert_eq!(super::off_mac("⌘↩ to post"), "Ctrl+Enter to post");
        assert_eq!(super::off_mac("Close Tab (⌘W)"), "Close Tab (Ctrl+W)");
        assert_eq!(super::off_mac("⌃⌘S"), "Ctrl+S");
        assert_eq!(super::off_mac("Overview  ⌥⌘1"), "Overview  Ctrl+Alt+1");
        assert_eq!(super::off_mac("⌘,"), "Ctrl+,");
        assert_eq!(super::off_mac("Send  ↩    New line  ⌥↩"), "Send  Enter    New line  Alt+Enter");
    }
}
