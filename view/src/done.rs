//! What a toast says after a mail action: "Archived", "Moved 3
//! conversations to the Trash", "Flagged red". Both front ends word an
//! action's toast through here.

use mailrs_domain::translate::{fill_plural, gettext};
use mailrs_sync::{MailAction, TriageAction};

/// The colour a flag toast names.
pub fn flagged_message(color: mailrs_domain::FlagColor) -> String {
    use mailrs_domain::FlagColor;
    match color {
        FlagColor::Red => gettext("Flagged red"),
        FlagColor::Orange => gettext("Flagged orange"),
        FlagColor::Yellow => gettext("Flagged yellow"),
        FlagColor::Green => gettext("Flagged green"),
        FlagColor::Blue => gettext("Flagged blue"),
        FlagColor::Purple => gettext("Flagged purple"),
        FlagColor::Gray => gettext("Flagged gray"),
    }
}

/// The toast after an action, or `None` when the change speaks for itself.
pub fn done_message(action: &MailAction, count: usize, threaded: bool) -> Option<String> {
    let number = count.to_string();
    let values = [("count", number.as_str())];
    let action = match action {
        MailAction::Triage(action) => action,
        MailAction::Flag(color) => return color.map(flagged_message),
        MailAction::Label { .. } => return Some(gettext("Labels changed")),
        MailAction::Mute { muted } => {
            return Some(match (*muted, count > 1, threaded) {
                (true, false, _) => gettext("Muted"),
                (false, false, _) => gettext("Unmuted"),
                (true, true, true) => fill_plural(
                    "Muted {count} conversation",
                    "Muted {count} conversations",
                    count,
                    &values,
                ),
                (true, true, false) => fill_plural(
                    "Muted {count} message",
                    "Muted {count} messages",
                    count,
                    &values,
                ),
                (false, true, true) => fill_plural(
                    "Unmuted {count} conversation",
                    "Unmuted {count} conversations",
                    count,
                    &values,
                ),
                (false, true, false) => fill_plural(
                    "Unmuted {count} message",
                    "Unmuted {count} messages",
                    count,
                    &values,
                ),
            });
        }
        MailAction::Remind { .. } | MailAction::CancelReminder => return None,
        MailAction::DismissFollowUp => {
            return Some(fill_plural(
                "Dismissed {count} follow-up",
                "Dismissed {count} follow-ups",
                count,
                &values,
            ));
        }
    };
    let many = count > 1;
    Some(match (action, many, threaded) {
        (TriageAction::Archive, false, _) => gettext("Archived"),
        (TriageAction::Archive, true, true) => fill_plural(
            "Archived {count} conversation",
            "Archived {count} conversations",
            count,
            &values,
        ),
        (TriageAction::Archive, true, false) => fill_plural(
            "Archived {count} message",
            "Archived {count} messages",
            count,
            &values,
        ),
        (TriageAction::Trash, false, _) => gettext("Moved to Trash"),
        (TriageAction::Trash, true, true) => fill_plural(
            "Moved {count} conversation to Trash",
            "Moved {count} conversations to Trash",
            count,
            &values,
        ),
        (TriageAction::Trash, true, false) => fill_plural(
            "Moved {count} message to Trash",
            "Moved {count} messages to Trash",
            count,
            &values,
        ),
        (TriageAction::Junk, false, _) => gettext("Marked as junk"),
        (TriageAction::Junk, true, true) => fill_plural(
            "Marked {count} conversation as junk",
            "Marked {count} conversations as junk",
            count,
            &values,
        ),
        (TriageAction::Junk, true, false) => fill_plural(
            "Marked {count} message as junk",
            "Marked {count} messages as junk",
            count,
            &values,
        ),
        (TriageAction::Untrash | TriageAction::NotJunk, false, _) => gettext("Moved to the Inbox"),
        (TriageAction::Untrash | TriageAction::NotJunk, true, true) => fill_plural(
            "Moved {count} conversation to the Inbox",
            "Moved {count} conversations to the Inbox",
            count,
            &values,
        ),
        (TriageAction::Untrash | TriageAction::NotJunk, true, false) => fill_plural(
            "Moved {count} message to the Inbox",
            "Moved {count} messages to the Inbox",
            count,
            &values,
        ),
        (
            TriageAction::AddLabel(_) | TriageAction::RemoveLabel(_) | TriageAction::Relabel { .. },
            ..,
        ) => gettext("Labels changed"),
        _ => return None,
    })
}
