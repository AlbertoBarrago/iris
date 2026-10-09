//! The one dialog that asks before Iris leaves a mailing list.
//!
//! It takes one line or twenty, so the Unsubscribe button and the
//! assistant ask the same question in the same words. Each line names a
//! list and says what leaving it will do: a request to the sender, a
//! mail from the person's own address, or a button pressed on the
//! sender's page with the address the newsletter was sent to. Reading a
//! page takes up to twenty seconds, so a line that is still being read
//! carries a spinner and fills itself in when the read ends, and
//! Unsubscribe stays insensitive until every line has settled.
//!
//! The words are this module's, translated; [`crate::unsubscribe_page`]
//! keeps its outcomes in plain English for the log and the assistant.
//! The text half is [`line_text`], [`summary`] and [`mask`], which are
//! sentences about plain data and are tested as such.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use futures::future::{Either, select};
use gtk::glib;
use mailrs_domain::translate::gettext;


pub use mailrs_appcore::unsubscribe_lines::*;

/// The response that goes ahead. Cancel, Escape and closing the dialog
/// all answer something else, and none of them submits anything.
const GO: &str = "go";

/// Asks about `lines`, filling each of them in as `updates` says what a
/// page turned out to hold. Answers the ticked lines, each with the way
/// it settled on, or nothing when the person said no.
///
/// The caller keeps whatever else it knows about a list in the same
/// order, since the index of a line is what comes back with it.
pub async fn confirm(
    parent: &impl IsA<gtk::Widget>,
    lines: Vec<ListLine>,
    updates: async_channel::Receiver<(usize, Way)>,
) -> Option<Vec<(usize, Way)>> {
    let dialog = adw::AlertDialog::new(Some(&heading(&lines)), body(&lines).as_deref());
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let mut rows = Vec::with_capacity(lines.len());
    let ways = Rc::new(RefCell::new(Vec::with_capacity(lines.len())));
    for line in lines {
        let check = gtk::CheckButton::builder().active(true).build();
        // The row's title is the list's name, and a checkbox beside it
        // carries no words of its own, so a screen reader is told the
        // name here too.
        crate::ui::name(&check, &line.name);
        let spinner = adw::Spinner::builder()
            .visible(matches!(line.way, Way::Reading))
            .build();
        let row = adw::ActionRow::builder()
            .title(glib::markup_escape_text(&line.name))
            .subtitle(glib::markup_escape_text(&line_text(&line.way)))
            .activatable_widget(&check)
            .build();
        row.add_prefix(&check);
        row.add_suffix(&spinner);
        list.append(&row);
        rows.push(Line {
            check,
            row,
            spinner,
        });
        ways.borrow_mut().push(line.way);
    }
    dialog.set_extra_child(Some(&list));
    dialog.add_responses(&[
        ("cancel", &gettext("Cancel")),
        (GO, &gettext("Unsubscribe")),
    ]);
    dialog.set_response_appearance(GO, adw::ResponseAppearance::Suggested);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some(GO));
    dialog.set_response_enabled(GO, !anyone_reading(&ways.borrow()));

    let rows = Rc::new(rows);
    let settling = settle(dialog.clone(), Rc::clone(&rows), Rc::clone(&ways), updates);
    let answering = dialog.choose_future(Some(parent));
    futures::pin_mut!(settling, answering);
    let answer = match select(answering, settling).await {
        // The person answered while a page was still loading. The read
        // goes on until the run that started it drops the browser.
        Either::Left((answer, _)) => answer,
        Either::Right((_, answering)) => answering.await,
    };
    if answer != GO {
        return None;
    }
    let ways = std::mem::take(&mut *ways.borrow_mut());
    Some(
        ways.into_iter()
            .enumerate()
            .filter(|(at, _)| rows[*at].check.is_active())
            .collect(),
    )
}

/// One line's widgets, kept so that a page arriving late can fill the
/// line in.
struct Line {
    check: gtk::CheckButton,
    row: adw::ActionRow,
    spinner: adw::Spinner,
}

/// Fills lines in as their pages arrive, and lets Unsubscribe go
/// sensitive once none is left reading. It ends when whoever is reading
/// the pages has nothing more to send.
async fn settle(
    dialog: adw::AlertDialog,
    rows: Rc<Vec<Line>>,
    ways: Rc<RefCell<Vec<Way>>>,
    updates: async_channel::Receiver<(usize, Way)>,
) {
    while let Ok((at, way)) = updates.recv().await {
        let Some(line) = rows.get(at) else {
            continue;
        };
        line.row
            .set_subtitle(&glib::markup_escape_text(&line_text(&way)));
        line.spinner.set_visible(false);
        let mut ways = ways.borrow_mut();
        if let Some(held) = ways.get_mut(at) {
            *held = way;
        }
        dialog.set_response_enabled(GO, !anyone_reading(&ways));
    }
}

