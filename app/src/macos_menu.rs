//! The macOS menu bar, and the Dock's click that reopens the window.
//!
//! GTK builds the Iris menu itself (About, Preferences, Services, Hide,
//! Quit) from the `app.about`, `app.preferences` and `app.quit` actions,
//! and shows the menu bar this adds after it: File, Edit and Window. Each
//! item runs what the window's own menu runs, so the two never disagree.
//! The window's actions live on the window, which may be closed, so the
//! app's actions here open it first.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use mailrs_domain::translate::gettext;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send, sel};
use objc2_app_kit::NSApplication;

use crate::app::App;

/// Adds the menu bar and the actions behind it. `gio` must be the
/// `gtk::Application` the macOS build runs as.
pub fn install(app: &Rc<App>, gio: &gio::Application) {
    let Some(gtk_app) = gio.downcast_ref::<gtk::Application>() else {
        tracing::warn!("the menu bar needs a gtk::Application");
        return;
    };

    // What the window's menu offers, reached from the app.
    for (name, window_action) in [
        ("about", "win.about"),
        ("preferences", "win.preferences"),
        ("add-account", "win.add-account"),
    ] {
        let action = gio::SimpleAction::new(name, None);
        let weak = Rc::downgrade(app);
        action.connect_activate(move |_, _| {
            if let Some(app) = weak.upgrade() {
                let window = app.show_window();
                if let Err(err) = window.window.activate_action(window_action, None) {
                    tracing::warn!(action = window_action, error = %err, "the menu bar could not run it");
                }
            }
        });
        gio.add_action(&action);
    }

    // Edit and Window act on whatever has the keys, GTK's or the mail page's.
    for (name, selector, gtk_action) in [
        ("undo", sel!(undo:), "text.undo"),
        ("redo", sel!(redo:), "text.redo"),
        ("cut", sel!(cut:), "clipboard.cut"),
        ("copy", sel!(copy:), "clipboard.copy"),
        ("paste", sel!(paste:), "clipboard.paste"),
        ("select-all", sel!(selectAll:), "selection.select-all"),
    ] {
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(move |_, _| edit(selector, gtk_action));
        gio.add_action(&action);
    }
    let minimize = gio::SimpleAction::new("minimize", None);
    minimize.connect_activate(|_, _| {
        if let Some(window) = active_window() {
            window.minimize();
        }
    });
    gio.add_action(&minimize);

    for (action, accels) in [
        ("app.preferences", &["<Meta>comma"][..]),
        ("app.quit", &["<Meta>q"]),
        ("app.compose", &["<Meta>n"]),
        ("app.check", &["<Meta><Shift>n"]),
        ("app.undo", &["<Meta>z"]),
        ("app.redo", &["<Meta><Shift>z"]),
        ("app.cut", &["<Meta>x"]),
        ("app.copy", &["<Meta>c"]),
        ("app.paste", &["<Meta>v"]),
        ("app.select-all", &["<Meta>a"]),
        ("app.minimize", &["<Meta>m"]),
    ] {
        gtk_app.set_accels_for_action(action, accels);
    }

    let file = gio::Menu::new();
    file.append(Some(&gettext("New Message")), Some("app.compose"));
    file.append(Some(&gettext("Check for Mail")), Some("app.check"));
    file.append(Some(&gettext("Add Account…")), Some("app.add-account"));

    let edit_menu = gio::Menu::new();
    let history = gio::Menu::new();
    history.append(Some(&gettext("Undo")), Some("app.undo"));
    history.append(Some(&gettext("Redo")), Some("app.redo"));
    edit_menu.append_section(None, &history);
    let clipboard = gio::Menu::new();
    clipboard.append(Some(&gettext("Cut")), Some("app.cut"));
    clipboard.append(Some(&gettext("Copy")), Some("app.copy"));
    clipboard.append(Some(&gettext("Paste")), Some("app.paste"));
    clipboard.append(Some(&gettext("Select All")), Some("app.select-all"));
    edit_menu.append_section(None, &clipboard);

    let window = gio::Menu::new();
    window.append(Some(&gettext("Minimize")), Some("app.minimize"));

    let bar = gio::Menu::new();
    bar.append_submenu(Some(&gettext("File")), &file);
    bar.append_submenu(Some(&gettext("Edit")), &edit_menu);
    bar.append_submenu(Some(&gettext("Window")), &window);
    gtk_app.set_menubar(Some(&bar));

    // Sparkle runs only in a bundle that carries it, the DMG's. GTK builds
    // the Iris menu from the model above a moment later, so the item goes
    // in on the next idle turn.
    if crate::sparkle::start() {
        glib::idle_add_local_once(|| crate::sparkle::add_menu_item(&gettext("Check for Updates…")));
    }

    listen_for_reopen(gio);
}

/// The Apple event a click on the Dock icon, or a second launch from
/// Launchpad or Finder, sends a running app: class 'aevt', id 'rapp'.
const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const REOPEN_APPLICATION: u32 = u32::from_be_bytes(*b"rapp");
/// The Apple event macOS sends the app that handles a URL scheme, such as
/// a `mailto:` link clicked in a browser: class 'GURL', id 'GURL', with the
/// URL as its direct object, '----'.
const INTERNET_EVENT_CLASS: u32 = u32::from_be_bytes(*b"GURL");
const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

thread_local! {
    /// The handler, kept for the life of the app: the event manager does
    /// not retain it.
    static REOPEN: std::cell::RefCell<Option<Retained<Reopener>>> = const { std::cell::RefCell::new(None) };
}

/// Answers the reopen event with GApplication's activate, which shows the
/// window, making a new one when the red button closed the last. GTK's
/// own answer counts only the windows a GtkApplication was told about, and
/// Iris's are plain windows, so it did nothing.
fn listen_for_reopen(gio: &gio::Application) {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("the reopen handler was set up off the main thread");
        return;
    };
    let reopener = Reopener::new(mtm, gio);
    unsafe {
        let manager: Retained<AnyObject> = msg_send![class!(NSAppleEventManager), sharedAppleEventManager];
        let _: () = msg_send![
            &*manager,
            setEventHandler: &*reopener,
            andSelector: sel!(handleReopen:withReplyEvent:),
            forEventClass: CORE_EVENT_CLASS,
            andEventID: REOPEN_APPLICATION
        ];
        let _: () = msg_send![
            &*manager,
            setEventHandler: &*reopener,
            andSelector: sel!(handleGetURL:withReplyEvent:),
            forEventClass: INTERNET_EVENT_CLASS,
            andEventID: GET_URL
        ];
    }
    REOPEN.with(|kept| kept.replace(Some(reopener)));
}

pub(crate) struct ReopenerIvars {
    app: glib::WeakRef<gio::Application>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IrisReopener"]
    #[ivars = ReopenerIvars]
    pub(crate) struct Reopener;

    impl Reopener {
        #[unsafe(method(handleReopen:withReplyEvent:))]
        fn handle_reopen(&self, _event: &AnyObject, _reply: &AnyObject) {
            tracing::info!("the Dock asked for the window");
            // Brings Iris in front of the app that had the screen, as a
            // click on any Mac app's Dock icon does; GTK's present alone
            // can leave the window behind it.
            NSApplication::sharedApplication(self.mtm()).activate();
            if let Some(app) = self.ivars().app.upgrade() {
                app.activate();
            }
        }

        /// A `mailto:` link opens a new message to its address, through
        /// the same app action as `iris mailto:…` on Linux.
        #[unsafe(method(handleGetURL:withReplyEvent:))]
        fn handle_get_url(&self, event: &AnyObject, _reply: &AnyObject) {
            let url: Option<Retained<objc2_foundation::NSString>> = unsafe {
                let descriptor: Option<Retained<AnyObject>> =
                    msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT];
                descriptor.and_then(|d| msg_send![&*d, stringValue])
            };
            let Some(url) = url.map(|u| u.to_string()) else { return };
            if !url.starts_with("mailto:") {
                tracing::warn!(url, "a URL Iris does not handle");
                return;
            }
            // The whole link goes through, so its subject and body do too.
            if let Some(app) = self.ivars().app.upgrade() {
                app.activate_action("compose-to", Some(&url.to_variant()));
            }
        }
    }
);

impl Reopener {
    fn new(mtm: MainThreadMarker, gio: &gio::Application) -> Retained<Reopener> {
        let app = glib::WeakRef::new();
        app.set(Some(gio));
        let this = Reopener::alloc(mtm).set_ivars(ReopenerIvars { app });
        unsafe { msg_send![super(this), init] }
    }
}

/// Runs an Edit item. The mail page is a WKWebView, which takes AppKit's
/// editing messages through the responder chain; when nothing there
/// answers, the keys are GTK's and its widget gets the matching action.
fn edit(selector: Sel, gtk_action: &str) {
    let focus = active_window().and_then(|window| GtkWindowExt::focus(&window));
    // The mail page has the keys whenever GTK's focus is on its view. GTK
    // answers the key itself, so the command goes to the page by name.
    if let Some(page) = focus.as_ref().and_then(|f| f.downcast_ref::<webkit::WebView>())
        && let Some(command) = editing_command(selector)
    {
        page.execute_editing_command(command);
        return;
    }
    if let Some(mtm) = MainThreadMarker::new() {
        let handled = unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(selector, None, None) };
        if handled {
            return;
        }
    }
    let Some(focus) = focus else {
        return;
    };
    // A widget without the action, such as a button, has nothing to edit.
    let _ = focus.activate_action(gtk_action, None::<&glib::Variant>);
}

/// The name webkit6 gives the Edit menu command `selector` stands for.
fn editing_command(selector: Sel) -> Option<&'static str> {
    [
        (sel!(copy:), "Copy"),
        (sel!(cut:), "Cut"),
        (sel!(paste:), "Paste"),
        (sel!(selectAll:), "SelectAll"),
        (sel!(undo:), "Undo"),
        (sel!(redo:), "Redo"),
    ]
    .into_iter()
    .find(|(known, _)| *known == selector)
    .map(|(_, name)| name)
}

/// Asks macOS to open `mailto:` links in Iris. macOS shows its own
/// confirmation, and the choice can be undone in any mail app's settings.
pub fn make_default_mail_app() {
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let app = objc2_foundation::NSBundle::mainBundle().bundleURL();
    let scheme = objc2_foundation::NSString::from_str("mailto");
    workspace.setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(&app, &scheme, None);
}

/// Whether a mouse button is held down right now, whichever window has it.
pub fn mouse_button_down() -> bool {
    let pressed: usize = unsafe { msg_send![class!(NSEvent), pressedMouseButtons] };
    pressed != 0
}

fn active_window() -> Option<gtk::Window> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .find(|window| window.is_active())
}
