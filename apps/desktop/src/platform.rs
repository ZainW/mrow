//! What reaches outside the window: the Dock badge, the app's appearance (so native menus and
//! the title bar match the chosen theme), and notifications.

use std::sync::Mutex;

use crate::data::inbox::Notice;

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

/// One banner per notice. A later one for the same merge request replaces it; a click opens it.
pub fn notify(notices: &[Notice]) {
    #[cfg(not(target_os = "linux"))]
    if !rune_notify::available() {
        return;
    }
    #[cfg(target_os = "linux")]
    linux::post(notices.to_vec());
    #[cfg(not(target_os = "linux"))]
    for notice in notices {
        let n = rune_notify::Notification::new(notice.request.clone(), notice.title.clone()).body(notice.body.clone()).info("id", notice.request.clone());
        rune_notify::post(&n);
    }
}

/// On Linux, starts listening for clicks on Mrow's banners. `raise` shows and focuses the
/// window with the activation token the click came with (empty when the desktop sent none).
#[cfg(target_os = "linux")]
pub fn watch_banners(raise: impl Fn(String) + Send + 'static) {
    linux::watch(Box::new(raise));
}

/// Banners over `org.freedesktop.Notifications` on the session bus, which every Linux
/// notification daemon speaks. rune-notify has no Linux side yet.
#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use zbus::MatchRule;
    use zbus::blocking::{Connection, MessageIterator, Proxy};
    use zbus::message::Type;
    use zbus::zvariant::Value;

    use super::{CLICKED, Notice};

    const NAME: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";
    /// The desktop file's name, so the daemon shows Mrow's name and icon.
    const APP: &str = "com.zain.mrow";

    /// The banners on screen: each daemon id and the merge request it opens.
    static SHOWN: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());

    fn bus() -> Option<&'static Connection> {
        static BUS: OnceLock<Option<Connection>> = OnceLock::new();
        BUS.get_or_init(|| Connection::session().ok()).as_ref()
    }

    fn shown() -> std::sync::MutexGuard<'static, Vec<(u32, String)>> {
        SHOWN.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Posts off the main thread: a daemon that is slow to answer must not hold up a frame.
    pub fn post(notices: Vec<Notice>) {
        std::thread::spawn(move || {
            let Some(proxy) = bus().and_then(|bus| Proxy::new(bus, NAME, PATH, NAME).ok()) else { return };
            // Daemons that draw markup read the body as markup; the rest show it as written.
            let markup = proxy.call::<_, _, Vec<String>>("GetCapabilities", &()).is_ok_and(|caps| caps.iter().any(|c| c == "body-markup"));
            for notice in notices {
                let replaces = shown().iter().find(|(_, request)| *request == notice.request).map_or(0, |(id, _)| *id);
                let body = if markup { escape(&notice.body) } else { notice.body.clone() };
                let hints = HashMap::from([("desktop-entry", Value::from(APP))]);
                let args = ("Mrow", replaces, APP, notice.title.as_str(), body.as_str(), vec!["default", "Open"], hints, -1i32);
                if let Ok(id) = proxy.call::<_, _, u32>("Notify", &args) {
                    let mut shown = shown();
                    shown.retain(|(old, _)| *old != id && *old != replaces);
                    shown.push((id, notice.request));
                }
            }
        });
    }

    fn escape(text: &str) -> String {
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    pub fn watch(raise: Box<dyn Fn(String) + Send>) {
        std::thread::spawn(move || {
            let Some(bus) = bus() else { return };
            let rule = MatchRule::builder().msg_type(Type::Signal).interface(NAME).and_then(|rule| rule.path(PATH)).map(|rule| rule.build());
            let Ok(signals) = rule.and_then(|rule| MessageIterator::for_match_rule(rule, bus, None)) else { return };
            // A daemon sends the token just before the click it belongs to.
            let mut token: Option<(u32, String)> = None;
            for message in signals.flatten() {
                let header = message.header();
                let body = message.body();
                match header.member().map(|m| m.as_str()) {
                    Some("ActivationToken") => token = body.deserialize::<(u32, String)>().ok(),
                    Some("ActionInvoked") => {
                        let Ok((id, _action)) = body.deserialize::<(u32, String)>() else { continue };
                        let Some(request) = shown().iter().find(|(shown, _)| *shown == id).map(|(_, request)| request.clone()) else { continue };
                        *CLICKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(request);
                        raise(token.take().filter(|(for_id, _)| *for_id == id).map(|(_, token)| token).unwrap_or_default());
                    }
                    Some("NotificationClosed") => {
                        if let Ok((id, _reason)) = body.deserialize::<(u32, u32)>() {
                            shown().retain(|(shown, _)| *shown != id);
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}
