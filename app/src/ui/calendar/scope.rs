//! The dialog for the one question a move, a change or a delete of an
//! event asks before it is written. appcore's `event_question` words it,
//! lays out its buttons and reads the one pressed; `ask` only shows it.

use adw::prelude::*;
use mailrs_domain::calendar::Event;
use mailrs_domain::calendar::series::RepeatScope;
use mailrs_domain::translate::gettext;
pub use mailrs_appcore::event_question::*;

/// Asks `question` about `event` over `parent`. `when` is the new time
/// of a move, in words. `None` for Cancel or the dialog closing another
/// way; a move then springs back and nothing is written, also when the
/// question [`keeps`](Question::keeps).
pub async fn ask(parent: &impl IsA<gtk::Widget>, question: &Question, event: &Event, when: Option<&str>) -> Option<Answer> {
    let body = body(question, when);
    let dialog = adw::AlertDialog::builder()
        .heading(heading(question, &event.title))
        .body(body.join("\n\n"))
        .prefer_wide_layout(false)
        .build();
    for response in responses(question) {
        dialog.add_response(response.id, &response.label);
        match response.look {
            Look::Plain => {}
            Look::Suggested => dialog.set_response_appearance(response.id, adw::ResponseAppearance::Suggested),
            Look::Destructive => dialog.set_response_appearance(response.id, adw::ResponseAppearance::Destructive),
        }
    }
    dialog.set_default_response(Some(default_response(question)));
    dialog.set_close_response(CLOSE_RESPONSE);
    // The occurrences as options, and the guests choice as a check, when
    // the buttons already carry another choice, so one dialog asks all.
    let extra = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let mut options: Vec<(RepeatScope, gtk::CheckButton)> = Vec::new();
    if scope_options(question) {
        for &scope in &question.scopes {
            let option = gtk::CheckButton::with_label(&scope_label(scope));
            if let Some((_, first)) = options.first() {
                option.set_group(Some(first));
            } else {
                option.set_active(true);
            }
            extra.append(&option);
            options.push((scope, option));
        }
    }
    let send = send_check(question).then(|| {
        let send = gtk::CheckButton::with_label(&gettext("Send an update to the guests"));
        send.set_active(true);
        if !options.is_empty() {
            send.set_margin_top(6);
        }
        extra.append(&send);
        send
    });
    if extra.first_child().is_some() {
        dialog.set_extra_child(Some(&extra));
    }
    let response = dialog.choose_future(Some(parent)).await;
    let picked = options.iter().find(|(_, o)| o.is_active()).map(|(s, _)| *s);
    let send = send.as_ref().is_none_or(|s| s.is_active());
    answer(question, response.as_str(), picked, send)
}

