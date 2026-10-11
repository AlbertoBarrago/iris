//! Importing an OpenPGP key or an S/MIME certificate from a file the
//! person picks in Preferences, and the dialog that says what came in.
//!
//! The snap keeps a GnuPG home of its own, so nothing in `~/.gnupg`
//! reaches it; this is how its keys get there. The import goes into
//! whatever home gpg and gpgsm already use, `GNUPGHOME` when it is set.
//! The text half, [`report`], is sentences about plain data and is tested
//! as such.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use mailrs_domain::translate::gettext;
use mailrs_pgp::gnupg::Import;
use mailrs_smime::SmimeError;

use crate::app::App;
pub use mailrs_appcore::key_import::*;

/// The Import button for `kind`, for the end of its row in Preferences.
/// It asks for a file, imports it, says what came in over `over`, and
/// calls `imported` after anything came in, so the rows can say what
/// the keyring holds now.
pub fn button(
    app: &Rc<App>,
    kind: Kind,
    over: &adw::PreferencesDialog,
    imported: impl Fn() + 'static,
) -> gtk::Button {
    // A child label rather than the button's own, so the name set below
    // reaches a screen reader: both rows' buttons read "Import…". The
    // spinner shares a stack with it, which keeps the button's width.
    let stack = gtk::Stack::new();
    stack.add_named(&gtk::Label::new(Some(&gettext("Import…"))), Some("label"));
    stack.add_named(&adw::Spinner::new(), Some("spinner"));
    let button = gtk::Button::builder()
        .child(&stack)
        .valign(gtk::Align::Center)
        .build();
    super::name(
        &button,
        &match kind {
            Kind::Pgp => gettext("Import OpenPGP Keys"),
            Kind::Smime => gettext("Import S/MIME Certificates"),
        },
    );
    // The dialog holds this button, so the button holds the dialog weakly.
    let (app, over, imported) = (Rc::downgrade(app), over.downgrade(), Rc::new(imported));
    button.connect_clicked(move |button| {
        let (Some(app), Some(over)) = (app.upgrade(), over.upgrade()) else {
            return;
        };
        let (imported, button) = (Rc::clone(&imported), button.clone());
        glib::spawn_future_local(async move {
            let window = over.root().and_downcast::<gtk::Window>();
            // The file is read only once the person picks it.
            let Ok(file) = chooser(kind).open_future(window.as_ref()).await else {
                return;
            };
            let named = file
                .basename()
                .map(|name| name.display().to_string())
                .unwrap_or_default();
            match file.load_contents_future().await {
                Ok((bytes, _)) => {
                    attempt(app, kind, Rc::new(bytes.to_vec()), over, button, imported).await;
                }
                Err(err) => {
                    let said = report(kind, Err(unreadable(&named, err.message())));
                    dialog(&said).present(Some(&over));
                }
            }
        });
    });
    button
}

/// The file chooser for `kind`, filtered to the files that hold one,
/// with every file a choice away for a key saved under another name.
fn chooser(kind: Kind) -> gtk::FileDialog {
    let (title, name, suffixes): (String, String, &[&str]) = match kind {
        Kind::Pgp => (
            gettext("Import OpenPGP Keys"),
            gettext("OpenPGP Keys"),
            &["asc", "gpg", "pgp", "key"],
        ),
        Kind::Smime => (
            gettext("Import S/MIME Certificates"),
            gettext("Certificates"),
            &["p12", "pfx", "pem", "crt", "cer"],
        ),
    };
    let keys = gtk::FileFilter::new();
    keys.set_name(Some(&name));
    for suffix in suffixes {
        keys.add_suffix(suffix);
    }
    let all = gtk::FileFilter::new();
    all.set_name(Some(&gettext("All Files")));
    all.add_pattern("*");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&keys);
    filters.append(&all);
    gtk::FileDialog::builder()
        .title(title)
        .modal(true)
        .filters(&filters)
        .default_filter(&keys)
        .build()
}

/// One run of the import, then the dialog that says how it went. Try
/// Again on that dialog runs it once more with the same bytes, so a
/// mistyped passphrase does not mean picking the file again.
async fn attempt(
    app: Rc<App>,
    kind: Kind,
    file: Rc<Vec<u8>>,
    over: adw::PreferencesDialog,
    button: gtk::Button,
    imported: Rc<dyn Fn()>,
) {
    loop {
        busy(&button, true);
        let outcome = run(&app, kind, &file).await;
        busy(&button, false);
        // The button went insensitive while the import ran, and took the
        // keyboard focus with it.
        button.grab_focus();
        if outcome.is_ok() {
            imported();
        }
        let said = report(kind, outcome.as_deref().map_err(Clone::clone));
        if dialog(&said).choose_future(Some(&over)).await != RETRY {
            return;
        }
    }
}

/// Imports `file` through the engine for `kind`.
async fn run(app: &App, kind: Kind, file: &Rc<Vec<u8>>) -> Result<Vec<Import>, Failure> {
    // The engines run on a blocking thread, which takes its own copy.
    let bytes = file.as_ref().clone();
    let ran = match kind {
        Kind::Pgp => app
            .core
            .gpg(move |pgp| Ok(pgp.import(&bytes)))
            .await
            .map(|found| found.map_err(|err| Failure::Other(crate::pgp::explain(&err)))),
        Kind::Smime => app
            .core
            .gpgsm(move |smime| Ok(smime.import(&bytes)))
            .await
            .map(|found| {
                found.map_err(|err| match err {
                    SmimeError::WrongPassphrase | SmimeError::NoPassphrase => {
                        Failure::Passphrase(crate::smime::explain(&err))
                    }
                    other => Failure::Other(crate::smime::explain(&other)),
                })
            }),
    };
    ran.unwrap_or_else(|err| Err(Failure::Other(err.to_string())))
}

/// Swaps the button's label for a spinner while gpg or gpgsm runs, which
/// can be as long as a pinentry stays open. The stack keeps the button at
/// the label's width.
fn busy(button: &gtk::Button, running: bool) {
    button.set_sensitive(!running);
    if let Some(stack) = button.child().and_downcast::<gtk::Stack>() {
        stack.set_visible_child_name(if running { "spinner" } else { "label" });
    }
}

/// The response that runs the import again.
const RETRY: &str = "retry";

/// The dialog that says how an import went.
fn dialog(said: &Report) -> adw::AlertDialog {
    let dialog = adw::AlertDialog::new(Some(&said.heading), Some(&said.body));
    if !said.lines.is_empty() {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        for line in &said.lines {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&line.title))
                .build();
            if let Some(subtitle) = &line.subtitle {
                row.set_subtitle(&glib::markup_escape_text(subtitle));
            }
            row.add_suffix(
                &gtk::Label::builder()
                    .label(&line.status)
                    .css_classes(["dim-label", "caption"])
                    .build(),
            );
            list.append(&row);
        }
        if said.more > 0 {
            list.append(
                &adw::ActionRow::builder()
                    .title(more_text(said.more))
                    .css_classes(["dim-label"])
                    .build(),
            );
        }
        dialog.set_extra_child(Some(&list));
    }
    if said.retry {
        dialog.add_responses(&[("close", &gettext("Close")), (RETRY, &gettext("Try Again"))]);
        dialog.set_response_appearance(RETRY, adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some(RETRY));
    } else {
        dialog.add_responses(&[("close", &gettext("Close"))]);
        dialog.set_default_response(Some("close"));
    }
    dialog.set_close_response("close");
    dialog
}
