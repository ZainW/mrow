//! What reaches outside the window: the Dock badge, the app's appearance (so native menus and
//! the title bar match the chosen theme), and notifications.

use std::sync::Mutex;

use crate::data::models::MergeRequest;

/// The merge request a clicked notification asked to show, picked up by the next poll.
static CLICKED: Mutex<Option<String>> = Mutex::new(None);

/// The waiting count on the Dock icon, or none.
pub fn set_badge(count: usize) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        use objc2_foundation::NSString;
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let tile = app.dockTile();
        let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
        tile.setBadgeLabel(label.as_deref());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = count;
}

/// Sets NSApp's appearance from the preference: System follows macOS, the others force it.
pub fn apply_appearance() {
    #[cfg(target_os = "macos")]
    {
        use crate::data::prefs::{self, Appearance};
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication};
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let name = match prefs::read(|p| p.appearance) {
            Appearance::System => None,
            Appearance::Light => Some(unsafe { NSAppearanceNameAqua }),
            Appearance::Dark => Some(unsafe { NSAppearanceNameDarkAqua }),
        };
        let appearance = name.and_then(NSAppearance::appearanceNamed);
        app.setAppearance(appearance.as_deref());
    }
}

/// Listens for clicks on Mrow's notifications; `wake` runs the frame loop to show the request.
pub fn listen(wake: rune::Waker) {
    rune_notify::on_click(move |click| {
        if let Some(id) = click.get("id") {
            *CLICKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.to_string());
            wake();
        }
    });
}

pub fn take_clicked() -> Option<String> {
    CLICKED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn notify(items: &[MergeRequest]) {
    if !rune_notify::available() {
        return;
    }
    for item in items {
        let asker = item.requested_by.as_ref().unwrap_or(&item.author);
        let n = rune_notify::Notification::new(item.id.clone(), format!("{} requested your review", asker.name))
            .body(format!("{}\n{}", item.short_reference(), item.title))
            .info("id", item.id.clone());
        rune_notify::post(&n);
    }
}
