//! The question before Delete Forever, which the Trash's Delete asks: how
//! much goes, who erases it, and whose mail stays because its server
//! cannot. Erased mail cannot come back, so nothing erases without it.

use mailrs_domain::translate::{fill, fill_plural, gettext};

/// What the question says, and the word on its button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub heading: String,
    pub body: String,
    pub verb: String,
}

/// The question before Delete Forever. `count` is the mail that goes,
/// `erasers` the providers of the accounts it goes from, and `kept` the
/// addresses of the accounts whose server cannot erase, whose mail stays
/// in the Trash. Every count writes its own sentence: a language decides
/// for itself where the number goes and which form the noun takes beside
/// it.
pub fn erase_question(count: usize, threaded: bool, erasers: &[&str], kept: &[&str]) -> Question {
    let number = count.to_string();
    let values = [("count", number.as_str())];
    let heading = match (threaded, count) {
        (true, 1) => gettext("Delete This Conversation Forever?"),
        (true, _) => fill_plural(
            "Delete {count} Conversation Forever?",
            "Delete {count} Conversations Forever?",
            count,
            &values,
        ),
        (false, 1) => gettext("Delete This Message Forever?"),
        (false, _) => fill_plural(
            "Delete {count} Message Forever?",
            "Delete {count} Messages Forever?",
            count,
            &values,
        ),
    };
    let body = match (erasers, count) {
        ([provider], 1) => fill(
            &gettext("{provider} deletes it from every device and cannot bring it back."),
            &[("provider", *provider)],
        ),
        ([provider], _) => fill(
            &gettext("{provider} deletes them from every device and cannot bring them back."),
            &[("provider", *provider)],
        ),
        (_, 1) => gettext("The server deletes it from every device and cannot bring it back."),
        _ => gettext(
            "Each account's server deletes them from every device and cannot bring them back.",
        ),
    };
    let body = match kept {
        [] => body,
        _ => {
            let accounts = crate::protection::joined(kept);
            let stays = fill_plural(
                "Mail in {accounts} stays in the Trash, since its server cannot delete mail for good.",
                "Mail in {accounts} stays in the Trash, since their servers cannot delete mail for good.",
                kept.len(),
                &[("accounts", &accounts)],
            );
            format!("{body} {stays}")
        }
    };
    Question {
        heading,
        body,
        verb: gettext("Delete Forever"),
    }
}

/// The toast after erasing `count` conversations, or messages when the
/// list shows them one by one.
pub fn deleted_forever_message(count: usize, threaded: bool) -> String {
    let number = count.to_string();
    let values = [("count", number.as_str())];
    match (threaded, count) {
        (_, 1) => gettext("Deleted forever"),
        (true, _) => fill_plural(
            "Deleted {count} conversation forever",
            "Deleted {count} conversations forever",
            count,
            &values,
        ),
        (false, _) => fill_plural(
            "Deleted {count} message forever",
            "Deleted {count} messages forever",
            count,
            &values,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_conversation_names_the_provider_that_erases_it() {
        let question = erase_question(1, true, &["Gmail"], &[]);
        assert_eq!(question.heading, "Delete This Conversation Forever?");
        assert_eq!(question.body, "Gmail deletes it from every device and cannot bring it back.");
        assert_eq!(question.verb, "Delete Forever");
    }

    #[test]
    fn several_messages_across_providers_name_each_accounts_server() {
        let question = erase_question(3, false, &["Gmail", "Fastmail"], &[]);
        assert_eq!(question.heading, "Delete 3 Messages Forever?");
        assert_eq!(
            question.body,
            "Each account's server deletes them from every device and cannot bring them back."
        );
    }

    #[test]
    fn the_accounts_that_keep_their_mail_are_listed() {
        let question = erase_question(1, true, &["Gmail"], &["a@x.org", "b@y.org", "c@z.org"]);
        assert!(
            question.body.ends_with(
                "Mail in a@x.org, b@y.org and c@z.org stays in the Trash, since their servers \
                 cannot delete mail for good."
            ),
            "{}",
            question.body
        );
    }
}
