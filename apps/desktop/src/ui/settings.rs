//! The settings sheet: accounts, the queue and the appearance. Changes apply as they're made;
//! the queue refreshes when the sheet closes if an account changed.

use rune::{El, PopupMenu, ScrollSpec, Tx, col, el, icon, row, text};
use rune_kit::{Checkbox, Kit, SegmentedControl, TextInput};

use crate::app::App;
use crate::data::forge::Forge;
use crate::data::prefs::{self, Appearance, Prefs};
use crate::data::update::{self, Phase};
use crate::ui::actions;
use crate::ui::components::{Chrome, ChromeButton, action_button, rule};
use crate::ui::icons as I;
use crate::ui::theme::{Pal, SEMIBOLD, mono, serif, ui};

/// The scrim and the sheet, as layers over the window.
pub fn sheet(s: &App, p: &Pal, k: &Kit) -> Vec<El> {
    let prefs = prefs::get();
    let body = col()
        .key("settings")
        .role("dialog")
        .label("Settings")
        .w(480.0)
        // Shrinks to fit the window; the sections scroll inside.
        .shrink()
        .min_h(0.0)
        .padding(24.0)
        .radius(12.0)
        .fill(p.elevated)
        .border(1.0, p.line)
        .shadow(12.0, 40.0, crate::ui::theme::shade(if p.dark { 0.5 } else { 0.18 }))
        .child(
            row()
                .w_full()
                .pb(18.0)
                .center_y()
                .child(text("Settings").font(serif(22.0, SEMIBOLD)).color(p.text))
                .child(el().grow(1.0))
                .child(ChromeButton::new("close-settings", "Close").icon(&I::CLOSE, 13.0).build(p).on_press_tx(close)),
        );
    let mut sections = col().key("settings-sections").w_full().shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false });
    for forge in Forge::ALL {
        sections = sections.child(account(s, p, k, &prefs, forge));
    }
    sections = sections.child(queue(p, k, &prefs)).child(appearance(p, k, &prefs)).child(updates(s, p, k, &prefs));
    let body = body.child(sections).modal_tx(true, |s: &mut App, tx: &mut Tx| close(s, tx));
    vec![
        el().key("settings-scrim").absolute(0.0, 0.0).w_full().h_full().fill(crate::ui::theme::shade(0.25)).a11y_hidden(),
        col().key("settings-layer").absolute(0.0, 0.0).w_full().h_full().padding(16.0).center().child(body),
    ]
}

fn close(s: &mut App, _: &mut Tx) {
    s.settings_open = false;
    if s.settings_dirty {
        s.settings_dirty = false;
        s.store.refresh();
    }
}

fn section(p: &Pal, title: &str, rows: Vec<El>) -> El {
    let mut c = col().w_full().gap(10.0).child(text(title.to_uppercase()).font(ui(10.5, SEMIBOLD).tracking(0.6)).color(p.tertiary));
    for r in rows {
        c = c.child(r);
    }
    col().w_full().child(rule(p)).child(c.py(14.0))
}

fn line(p: &Pal, label: &str, content: El) -> El {
    row()
        .w_full()
        .gap(12.0)
        .center_y()
        .child(el().w(120.0).child(text(label.to_string()).font(ui(12.5, 400.0)).color(p.secondary).single_line().self_end()).items_end())
        .child(content.shrink().min_w(0.0))
}

fn check(k: &Kit, label: &str, on: bool, f: impl Fn(&mut Prefs) + 'static, dirties: bool) -> El {
    Checkbox::new(on)
        .label(label.to_string())
        .on_toggle(move |s: &mut App| {
            prefs::update(&f);
            if dirties {
                s.settings_dirty = true;
            }
        })
        .build(k)
}

fn account(s: &App, p: &Pal, k: &Kit, prefs: &Prefs, forge: Forge) -> El {
    let enabled = prefs.is_enabled(forge);
    let mut rows = vec![line(
        p,
        "",
        check(
            k,
            &format!("Show {} reviews", forge.name()),
            enabled,
            move |p| match forge {
                Forge::Gitlab => p.gitlab_enabled = !p.gitlab_enabled,
                Forge::Github => p.github_enabled = !p.github_enabled,
            },
            true,
        ),
    )];
    if enabled {
        let account = s.store.accounts.get(&forge);
        let (signed, failed) = match account {
            Some(a) if a.me.is_some() => {
                let me = a.me.as_ref().unwrap();
                (format!("{} (@{})", me.name, me.username), false)
            }
            Some(a) if a.error.is_some() => (a.error.clone().unwrap(), true),
            _ => (format!("Not yet. Run “{}” in a terminal.", forge.cli().login_command), false),
        };
        rows.push(line(p, "Signed in as", text(signed).font(ui(12.5, 400.0)).color(if failed { p.red } else { p.secondary }).wrap().max_lines(2)));
        let (host, path) = match forge {
            Forge::Gitlab => (&s.settings.hostname, &s.settings.glab_path),
            Forge::Github => (&s.settings.github_host, &s.settings.gh_path),
        };
        rows.push(line(p, "Host", field(p, k, forge, true, host, forge.default_host().to_string())));
        let detected = forge.cli().executable().unwrap_or_else(|_| crate::data::paths::tool_dirs()[0].join(forge.cli().name).to_string_lossy().into_owned());
        rows.push(line(p, &format!("{} path", forge.cli().name), field(p, k, forge, false, path, detected)));
    }
    section(p, forge.name(), rows)
}

fn field(p: &Pal, k: &Kit, forge: Forge, host: bool, edit: &rune::TextEdit, placeholder: String) -> El {
    let input = TextInput::new(edit)
        .plain()
        .width(None)
        .height(28.0)
        .text_style(mono(12.0, 400.0))
        .placeholder(placeholder)
        .on_edit(move |s: &mut App| match (forge, host) {
            (Forge::Gitlab, true) => &mut s.settings.hostname,
            (Forge::Gitlab, false) => &mut s.settings.glab_path,
            (Forge::Github, true) => &mut s.settings.github_host,
            (Forge::Github, false) => &mut s.settings.gh_path,
        })
        .on_change(move |s: &mut App, _: &mut Tx| {
            let value = match (forge, host) {
                (Forge::Gitlab, true) => s.settings.hostname.text.clone(),
                (Forge::Gitlab, false) => s.settings.glab_path.text.clone(),
                (Forge::Github, true) => s.settings.github_host.text.clone(),
                (Forge::Github, false) => s.settings.gh_path.text.clone(),
            };
            prefs::update(|p| match (forge, host) {
                (Forge::Gitlab, true) => p.hostname = value,
                (Forge::Gitlab, false) => p.glab_path = value,
                (Forge::Github, true) => p.github_host = value,
                (Forge::Github, false) => p.gh_path = value,
            });
            s.settings_dirty = true;
        })
        .build(k);
    row().w_full().h(28.0).px(9.0).radius(7.0).fill(p.hover).center_y().child(input.grow(1.0))
}

/// A pop-up button: the current choice and a chevron, opening a native menu.
fn picker(p: &Pal, key: &'static str, options: &'static [(&'static str, &'static str)], current: &str, pick: fn(&mut Prefs, &str)) -> El {
    let title = options.iter().find(|(id, _)| *id == current).map_or(current, |(_, t)| t).to_string();
    let selected = current.to_string();
    row()
        .key(key)
        .role("button")
        .label(title.clone())
        .h(26.0)
        .pl(10.0)
        .pr(8.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .fill(p.paper)
        .border(1.0, p.strong_line)
        .hover(p.hover)
        .child(text(title).font(ui(12.5, 400.0)).color(p.text).single_line())
        .child(icon(&I::CHEVRON_UP_DOWN, 10.0).color(p.secondary))
        .on_press_tx(move |_: &mut App, tx: &mut Tx| {
            let menu = PopupMenu::picker(options.iter().copied(), &selected);
            tx.popup_menu(menu, key, move |_: &mut App, picked, _| {
                if let Some(id) = picked {
                    prefs::update(|p| pick(p, id));
                }
            });
        })
}

const REFRESH: &[(&str, &str)] = &[("0", "Never"), ("1", "Minute"), ("5", "5 minutes"), ("15", "15 minutes"), ("30", "30 minutes")];
const SORT: &[(&str, &str)] = &[("newest", "Newest request first"), ("oldest", "Waiting longest first")];

fn queue(p: &Pal, k: &Kit, prefs: &Prefs) -> El {
    let days = prefs.done_days;
    let stepper = row()
        .gap(8.0)
        .center_y()
        .child(text(format!("{days} days")).font(ui(12.5, 400.0).features("'tnum' 1")).color(p.text))
        .child(step(p, "done-less", "−", days > 1, -1))
        .child(step(p, "done-more", "+", days < 90, 1));
    section(
        p,
        "Queue",
        vec![
            line(
                p,
                "Refresh every",
                picker(p, "refresh-every", REFRESH, &prefs.refresh_minutes.to_string(), |p, id| p.refresh_minutes = id.parse().unwrap_or(5)),
            ),
            line(p, "Done looks back", stepper),
            line(p, "Sort", picker(p, "sort-order", SORT, if prefs.oldest_first { "oldest" } else { "newest" }, |p, id| p.oldest_first = id == "oldest")),
            line(p, "", check(k, "Show drafts in To review", prefs.show_drafts, |p| p.show_drafts = !p.show_drafts, false)),
            line(p, "", check(k, "Notify me about new review requests", prefs.notify, |p| p.notify = !p.notify, false)),
        ],
    )
}

fn step(p: &Pal, key: &'static str, label: &'static str, enabled: bool, by: i64) -> El {
    let b = el()
        .key(key)
        .role("button")
        .label(if by < 0 { "Fewer days" } else { "More days" })
        .size(22.0, 22.0)
        .radius(6.0)
        .center()
        .fill(p.paper)
        .border(1.0, p.strong_line)
        .child(text(label).font(ui(13.0, 500.0)).color(if enabled { p.text } else { p.tertiary }));
    if !enabled {
        return b.disabled(true);
    }
    b.hover(p.hover).on_press(move |s: &mut App| {
        prefs::update(|p| p.done_days = (p.done_days + by).clamp(1, 90));
        s.settings_dirty = true;
    })
}

fn appearance(p: &Pal, k: &Kit, prefs: &Prefs) -> El {
    let choices = [Appearance::System, Appearance::Light, Appearance::Dark];
    let labels: Vec<&str> = choices.iter().map(|a| a.name()).collect();
    let current = choices.iter().position(|a| *a == prefs.appearance);
    let control = SegmentedControl::new(&labels, current)
        .label("Theme")
        .fit()
        .on_pick(move |_: &mut App, index: usize, _: &mut Tx| {
            prefs::update(|p| p.appearance = choices[index]);
            crate::platform::apply_appearance();
        })
        .build(k);
    section(p, "Appearance", vec![line(p, "Theme", control)])
}

fn updates(s: &App, p: &Pal, k: &Kit, prefs: &Prefs) -> El {
    let u = &s.updater;
    let (status, failed) = match &u.phase {
        _ if !update::ENABLED => ("built from source".to_string(), false),
        Phase::Idle => (String::new(), false),
        Phase::Checking => ("checking…".into(), false),
        Phase::Current => ("up to date".into(), false),
        Phase::Downloading(v) => (format!("downloading {v}…"), false),
        Phase::Ready(r) => (format!("{} is ready", r.version), false),
        Phase::Failed(e) => (e.clone(), true),
    };
    let mut version = row()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(6.0)
        .center_y()
        .child(text(update::VERSION).font(ui(12.5, 400.0).features("'tnum' 1")).color(p.text).single_line());
    if !status.is_empty() {
        let tint = if failed { p.red } else { p.tertiary };
        version = version.child(text(status.clone()).font(ui(12.5, 400.0)).color(tint).truncate().shrink().min_w(0.0).help(status));
    }
    let button = if u.ready().is_some() {
        action_button(p, "restart-update", None, "Restart to Update", true, true).on_press_tx(actions::restart_to_update)
    } else {
        action_button(p, "check-updates", None, "Check Now", false, update::ENABLED && !u.busy()).on_press_tx(actions::check_for_updates)
    };
    let notes = ChromeButton::new("release-notes", "Release Notes").style(Chrome::Text).build(p).on_press_tx(actions::release_notes);
    let mut more = row().w_full().gap(8.0).center_y();
    if update::ENABLED {
        more = more.child(check(k, "Download updates automatically", prefs.auto_update, |p| p.auto_update = !p.auto_update, false));
    }
    let rows = vec![line(p, "Version", row().w_full().gap(8.0).center_y().child(version).child(button)), line(p, "", more.child(el().grow(1.0)).child(notes))];
    section(p, "Updates", rows)
}
