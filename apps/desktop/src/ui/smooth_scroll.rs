//! Smooth scrolling for a notched mouse wheel. Rune moves a scroll view by a wheel's line
//! steps in one jump; here each notch glides to its place instead, and notches that come
//! quickly add up into one motion. Trackpads and free-spinning wheels send point-precise deltas
//! that already move with the fingers (and AppKit's momentum), so they scroll as before.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use rune::{El, ScrollPos, Spring, Tx, WheelEvent};

use crate::app::App;

/// The glide a notch starts: quick, so the wheel still feels direct.
const NOTCH: Spring = Spring::new(0.2, 0.0);

/// A notch this soon after the last one carries on from where that one was headed, rather
/// than from where the glide has got to, so a fast spin covers its full distance.
const RUN: Duration = Duration::from_millis(250);

/// Where a scroll view is, and where its notches are taking it.
#[derive(Clone, Copy, Default)]
struct Track {
    offset: f32,
    max: f32,
    heading: Option<(f32, Instant)>,
}

thread_local! {
    static TRACKS: RefCell<HashMap<String, Track>> = RefCell::new(HashMap::new());
}

/// Makes the vertical scroll view `view`, keyed `key`, glide on mouse-wheel notches.
pub fn smooth_wheel(view: El, key: &str) -> El {
    smooth_wheel_watching(view, key, |_| {})
}

/// [`smooth_wheel`], also passing every scroll position to `watch` (the view's one
/// `on_scroll_offset` handler is taken here).
pub fn smooth_wheel_watching(view: El, key: &str, watch: impl Fn(ScrollPos) + 'static) -> El {
    let seen = key.to_string();
    let key = key.to_string();
    view.on_scroll_offset(move |at| {
        TRACKS.with(|t| {
            let mut t = t.borrow_mut();
            let track = t.entry(seen.clone()).or_default();
            track.offset = at.offset[1];
            track.max = at.max[1];
        });
        watch(at);
    })
    .on_wheel(move |_: &mut App, ev: WheelEvent, tx: &mut Tx| {
        if ev.precise || ev.dy == 0.0 || ev.mods.cmd || ev.mods.ctrl {
            return false;
        }
        let to = TRACKS.with(|t| {
            let mut t = t.borrow_mut();
            let track = t.entry(key.clone()).or_default();
            let from = match track.heading {
                Some((to, at)) if at.elapsed() < RUN => to,
                _ => track.offset,
            };
            // A positive delta moves the content down, toward the top.
            let to = (from - ev.dy).clamp(0.0, track.max);
            track.heading = Some((to, Instant::now()));
            to
        });
        tx.scroll_to_with(key.clone(), 0.0, to, NOTCH);
        true
    })
}
