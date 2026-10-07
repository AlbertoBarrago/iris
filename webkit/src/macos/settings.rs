use std::cell::Cell;
use std::rc::Rc;

/// The few settings WKWebView has an answer for. The rest of webkit6's
/// setters are accepted and dropped: WKWebView already keeps what they
/// turn off (developer tools, storage, media, WebGL, WebRTC) out of a page
/// loaded the way these views load theirs, or has no switch for it.
#[derive(Clone)]
pub struct Settings(Rc<Inner>);

struct Inner {
    javascript: Cell<bool>,
    javascript_markup: Cell<bool>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::new()
    }
}

macro_rules! dropped {
    ($($name:ident),* $(,)?) => {
        $(
            /// Accepted for webkit6's sake; WKWebView has no such switch.
            pub fn $name(&self, _on: bool) {}
        )*
    };
}

impl Settings {
    pub fn new() -> Settings {
        Settings(Rc::new(Inner {
            javascript: Cell::new(true),
            javascript_markup: Cell::new(true),
        }))
    }

    /// Whether any script runs at all, the app's own included.
    pub fn set_enable_javascript(&self, on: bool) {
        self.0.javascript.set(on);
    }

    /// Whether the page's own scripts run. WKWebView's
    /// `allowsContentJavaScript` is the same switch: with it off, the
    /// scripts the app injects and evaluates still run.
    pub fn set_enable_javascript_markup(&self, on: bool) {
        self.0.javascript_markup.set(on);
    }

    pub(crate) fn page_scripts(&self) -> bool {
        self.0.javascript.get() && self.0.javascript_markup.get()
    }

    dropped!(
        set_javascript_can_open_windows_automatically,
        set_enable_developer_extras,
        set_enable_html5_local_storage,
        set_enable_html5_database,
        set_enable_page_cache,
        set_enable_media,
        set_enable_mediasource,
        set_enable_encrypted_media,
        set_enable_webaudio,
        set_enable_webgl,
        set_enable_webrtc,
        set_enable_fullscreen,
        set_enable_back_forward_navigation_gestures,
        set_allow_file_access_from_file_urls,
        set_allow_universal_access_from_file_urls,
        set_enable_smooth_scrolling,
        set_auto_load_images,
        set_enable_write_console_messages_to_stdout,
    );
}
