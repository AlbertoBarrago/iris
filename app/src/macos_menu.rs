//! The macOS menu bar.
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
use objc2::runtime::Sel;
use objc2::{MainThreadMarker, sel};
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
}

/// Runs an Edit item. The mail page is a WKWebView, which takes AppKit's
/// editing messages through the responder chain; when nothing there
/// answers, the keys are GTK's and its widget gets the matching action.
fn edit(selector: Sel, gtk_action: &str) {
    if let Some(mtm) = MainThreadMarker::new() {
        let handled = unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(selector, None, None) };
        if handled {
            return;
        }
    }
    let Some(focus) = active_window().and_then(|window| GtkWindowExt::focus(&window)) else {
        return;
    };
    // A widget without the action, such as a button, has nothing to edit.
    let _ = focus.activate_action(gtk_action, None::<&glib::Variant>);
}

fn active_window() -> Option<gtk::Window> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .find(|window| window.is_active())
}
