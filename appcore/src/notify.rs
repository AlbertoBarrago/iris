//! Desktop notifications for new mail, and the buttons on them.
//!
//! A notification the user acts on sends a [`Request`] back over an
//! `async-channel`, which the app reads on the GTK loop. Nothing here
//! touches GTK or the store, so the routing and the state check live under
//! unit tests even though the notification daemon does not.


use mailrs_domain::{MessageMeta, Role, Target};
use mailrs_sync::{MailAction, TriageAction};
use serde::{Deserialize, Serialize};

use crate::APP_ID;
use mailrs_domain::translate::{fill, fill_plural, gettext};

/// A button a new-mail notification can carry. Clicking the body opens the
/// conversation, so that is not one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Button {
    Archive,
    MarkRead,
    Delete,
    Reply,
}

impl Button {
    /// Every button, in the order a notification shows them.
    pub const ALL: [Button; 4] = [
        Button::Archive,
        Button::MarkRead,
        Button::Delete,
        Button::Reply,
    ];

    /// The action key the daemon sends back when the button is clicked.
    pub fn key(self) -> &'static str {
        match self {
            Button::Archive => "archive",
            Button::MarkRead => "mark-read",
            Button::Delete => "delete",
            Button::Reply => "reply",
        }
    }

    pub fn label(self) -> String {
        match self {
            Button::Archive => gettext("Archive"),
            Button::MarkRead => gettext("Mark as Read"),
            Button::Delete => gettext("Delete"),
            Button::Reply => gettext("Reply"),
        }
    }

    /// The mail action the button runs, or `None` for Reply, which opens a
    /// composer instead. Every one of these goes through `MailActions`, so
    /// it batches per account and Undo reverses it.
    pub fn action(self) -> Option<MailAction> {
        let triage = match self {
            Button::Archive => TriageAction::Archive,
            Button::MarkRead => TriageAction::MarkRead,
            Button::Delete => TriageAction::Trash,
            Button::Reply => return None,
        };
        Some(MailAction::Triage(triage))
    }
}

/// What the user did with a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Clicked the body, which opens the conversation.
    Open,
    Button(Button),
}

/// What a notification asks the app to do, and to which message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub target: Target,
    pub choice: Choice,
}

/// Whether `button` still has work to do on `message`: Archive while it
/// sits in the inbox, Mark as Read while it is unread, Delete until it is
/// in the Trash. A notification sits on screen long after its mail moved,
/// so the app reads the store before it acts and drops the action if the
/// mail beat the user to it.
pub fn still_applies(button: Button, message: &MessageMeta) -> bool {
    match button {
        Button::Archive => message.in_role(Role::Inbox),
        Button::MarkRead => message.is_unread(),
        Button::Delete => !message.in_role(Role::Trash),
        Button::Reply => true,
    }
}

/// One notification to show: its title, its body and the message a click
/// opens, when it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub target: Option<Target>,
}

/// What to say about `messages`: one notice each for up to three, one
/// summary beyond that. With `previews` off, a notice says only how much
/// mail arrived.
pub fn notices(messages: &[MessageMeta], previews: bool) -> Vec<Notice> {
    let count = messages.len();
    let how_many = || {
        fill_plural(
            "{count} new message",
            "{count} new messages",
            count,
            &[("count", &count.to_string())],
        )
    };
    if count == 0 {
        return Vec::new();
    }
    if !previews {
        return vec![Notice {
            title: how_many(),
            body: String::new(),
            target: (count == 1).then(|| target_of(&messages[0])),
        }];
    }
    if count <= 3 {
        return messages
            .iter()
            .map(|message| {
                let sender = message
                    .from
                    .as_ref()
                    .map(|a| a.display().to_string())
                    .unwrap_or_else(|| gettext("New message"));
                let subject = if message.subject.trim().is_empty() {
                    gettext("(no subject)")
                } else {
                    message.subject.clone()
                };
                Notice {
                    title: sender,
                    body: format!("{subject}\n{}", message.snippet),
                    target: Some(target_of(message)),
                }
            })
            .collect();
    }
    let senders: Vec<String> = messages
        .iter()
        .filter_map(|m| m.from.as_ref().map(|a| a.display().to_string()))
        .take(3)
        .collect();
    vec![Notice {
        title: how_many(),
        body: fill(
            &gettext("From {senders}"),
            &[("senders", &senders.join(", "))],
        ),
        target: None,
    }]
}

/// Shows notifications for `messages`, as [`notices`] words them.
/// Clicking one sends its thread to `chosen`, as does each of `buttons`.
pub fn announce(
    messages: Vec<MessageMeta>,
    previews: bool,
    buttons: Vec<Button>,
    chosen: async_channel::Sender<Request>,
) {
    for notice in notices(&messages, previews) {
        show(
            notice.title,
            notice.body,
            notice.target,
            buttons.clone(),
            chosen.clone(),
        );
    }
}

/// The message a notification announced, not its whole thread: archiving
/// from a notification should leave the replies the reader has not seen.
pub fn target_of(message: &MessageMeta) -> Target {
    Target {
        account_id: message.account_id,
        thread_id: message.thread_id.clone(),
        message_id: Some(message.id.clone()),
    }
}

fn show(
    summary: String,
    body: String,
    target: Option<Target>,
    buttons: Vec<Button>,
    chosen: async_channel::Sender<Request>,
) {
    std::thread::spawn(move || {
        let target = target.filter(|_| takes_actions());
        // macOS shows one action button, and several fold into a menu
        // behind an "Options" button, so a click on the body, which opens
        // the conversation, is all a Mac notification offers.
        let buttons: Vec<Button> = {
            let _ = buttons;
            Vec::new()
        };
        send_as_iris();
        let mut notification = notify_rust::Notification::new();
        notification
            .appname("Iris")
            .summary(&summary)
            .body(&escape(&body))
            .icon(APP_ID);
        if target.is_some() {
            notification.action("default", &gettext("Open"));
            for button in &buttons {
                notification.action(button.key(), &button.label());
            }
        }
        match notification.show() {
            Ok(handle) => {
                let Some(target) = target else { return };
                handle.wait_for_action(|key| {
                    if let Some(choice) = choice_of(key, &buttons) {
                        let _ = chosen.send_blocking(Request { target, choice });
                    }
                });
            }
            Err(err) => tracing::warn!(error = %err, "could not show a notification"),
        }
    });
}

/// The choice an action key stands for. A daemon also reports a closed
/// notification through this path, and one closed without a click asks for
/// nothing.
fn choice_of(key: &str, buttons: &[Button]) -> Option<Choice> {
    if key == "default" {
        return Some(Choice::Open);
    }
    buttons
        .iter()
        .find(|button| button.key() == key)
        .map(|button| Choice::Button(*button))
}

/// A Mac notification reports a click on its body, which opens the
/// conversation.
fn takes_actions() -> bool {
    true
}

/// Sends notifications under Iris's bundle id, once. Without it they go
/// out under another app's name and icon, and a click brings that app
/// forward instead of Iris.
fn send_as_iris() {
    static SET: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    SET.get_or_init(|| {
        if let Err(err) = notify_rust::set_application(APP_ID) {
            tracing::warn!(error = %err, "notifications could not take Iris's name");
        }
    });
}

/// Notification bodies accept a little markup, so text must be escaped.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_routes_back_from_its_action_key() {
        for button in Button::ALL {
            assert_eq!(
                choice_of(button.key(), &Button::ALL),
                Some(Choice::Button(button)),
                "{} lost its key",
                button.label()
            );
        }
        assert_eq!(choice_of("default", &Button::ALL), Some(Choice::Open));
    }

    #[test]
    fn a_closed_notification_asks_for_nothing() {
        assert_eq!(choice_of("__closed", &Button::ALL), None);
        assert_eq!(choice_of("archive", &[]), None, "a button nobody offered");
        assert_eq!(choice_of("", &Button::ALL), None);
    }

    #[test]
    fn three_buttons_change_mail_and_reply_opens_a_composer() {
        assert_eq!(
            Button::Archive.action(),
            Some(MailAction::Triage(TriageAction::Archive))
        );
        assert_eq!(
            Button::MarkRead.action(),
            Some(MailAction::Triage(TriageAction::MarkRead))
        );
        assert_eq!(
            Button::Delete.action(),
            Some(MailAction::Triage(TriageAction::Trash))
        );
        assert_eq!(Button::Reply.action(), None);
    }

    #[test]
    fn up_to_three_messages_each_get_a_notice_and_more_get_one() {
        use mailrs_sync::fake::meta;
        let three: Vec<MessageMeta> = (0..3)
            .map(|i| meta(&format!("m{i}"), "t1", 0, &["INBOX", "UNREAD"]))
            .collect();
        let shown = notices(&three, true);
        assert_eq!(shown.len(), 3);
        assert!(shown.iter().all(|n| n.target.is_some()));
        let four: Vec<MessageMeta> = (0..4)
            .map(|i| meta(&format!("m{i}"), "t1", 0, &["INBOX", "UNREAD"]))
            .collect();
        let summary = notices(&four, true);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].target, None, "a summary opens no one message");
        let quiet = notices(&three[..1], false);
        assert_eq!((quiet.len(), quiet[0].body.as_str()), (1, ""));
        assert!(quiet[0].target.is_some(), "one message still opens on a click");
        assert!(notices(&[], true).is_empty());
    }

    #[test]
    fn a_button_stops_applying_once_its_work_is_done() {
        use mailrs_sync::fake::meta;
        let unread_in_inbox = meta("m1", "t1", 0, &["INBOX", "UNREAD"]);
        let read_elsewhere = meta("m2", "t1", 0, &["Label_1"]);
        let trashed = meta("m3", "t1", 0, &["TRASH"]);
        assert!(still_applies(Button::Archive, &unread_in_inbox));
        assert!(!still_applies(Button::Archive, &read_elsewhere));
        assert!(still_applies(Button::MarkRead, &unread_in_inbox));
        assert!(!still_applies(Button::MarkRead, &read_elsewhere));
        assert!(still_applies(Button::Delete, &read_elsewhere));
        assert!(!still_applies(Button::Delete, &trashed));
        assert!(
            still_applies(Button::Reply, &trashed),
            "answering mail somebody trashed is still their business"
        );
    }
}
