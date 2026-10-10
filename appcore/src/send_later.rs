//! What Send Later says, for both front ends.

use mailrs_domain::translate::gettext;

/// What the toast says after Cancel Send: where the messages went. One
/// Gmail could not take is open in a composer, and one that could not
/// even be reopened is still waiting.
pub fn cancelled_line(in_drafts: usize, reopened: usize, kept: usize) -> String {
    match (in_drafts, reopened, kept) {
        (_, _, 1..) => gettext(
            "Gmail is out of reach and Iris cannot reopen a message, so it stays in Send Later.",
        ),
        (0, 1, 0) => {
            gettext("Won't be sent. Gmail is out of reach, so the message is open for you to save.")
        }
        (0, _, 0) => gettext(
            "Won't be sent. Gmail is out of reach, so the messages are open for you to save.",
        ),
        (1, 0, 0) => gettext("Won't be sent. The message is in Drafts."),
        (_, 0, 0) => gettext("Won't be sent. The messages are in Drafts."),
        _ => gettext(
            "Won't be sent. Some are in Drafts, and the ones Gmail could not take are open for you to save.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_send_says_where_each_message_went() {
        assert_eq!(
            cancelled_line(1, 0, 0),
            "Won't be sent. The message is in Drafts."
        );
        assert_eq!(
            cancelled_line(2, 0, 0),
            "Won't be sent. The messages are in Drafts."
        );
        assert!(cancelled_line(0, 1, 0).contains("open for you to save"));
        assert!(cancelled_line(0, 2, 0).contains("messages are open"));
        assert!(cancelled_line(1, 1, 0).contains("Some are in Drafts"));
        assert!(cancelled_line(0, 0, 1).contains("stays in Send Later"));
    }
}
