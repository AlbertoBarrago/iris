//! Reactions: a message that answers another with one emoji. Gmail marks
//! one with a `text/vnd.google.email-reaction+json` part, RFC 9078 with a
//! `text/plain` part under `Content-Disposition: reaction`. Both name the
//! message they answer in `In-Reply-To`.

/// The media type of Gmail's reaction part.
pub const GMAIL_REACTION: &str = "text/vnd.google.email-reaction+json";

/// The only version of Gmail's reaction part there is. Gmail refuses any
/// other, and so does this reader.
const GMAIL_VERSION: u64 = 1;

/// The emoji in a Gmail reaction part, when the JSON is well formed, says
/// version 1 and holds exactly one emoji. Gmail shows anything else as a
/// plain message, and so does the app.
pub fn from_gmail_part(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value.get("version")?.as_u64()? != GMAIL_VERSION {
        return None;
    }
    one_emoji(value.get("emoji")?.as_str()?)
}

/// The JSON of a Gmail reaction part carrying `emoji`.
pub fn gmail_part(emoji: &str) -> String {
    serde_json::json!({ "version": GMAIL_VERSION, "emoji": emoji }).to_string()
}

/// `text` when it holds exactly one emoji and nothing else, with the
/// whitespace around it trimmed.
///
/// One emoji can span several code points: a skin tone, a variation
/// selector, a keycap, two regional indicators for a flag, or a family
/// joined by zero width joiners. This counts the pictures those join into
/// without a Unicode segmentation table, which is enough to tell a single
/// emoji from a word or a row of them.
pub fn one_emoji(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 64 {
        return None;
    }
    let keycap = text.contains('\u{20E3}');
    let mut pictures = 0;
    let mut joined = false;
    let mut open_flag = false;
    for c in text.chars() {
        match c {
            '\u{200D}' => joined = true,
            '\u{FE0F}' | '\u{FE0E}' | '\u{20E3}' | '\u{1F3FB}'..='\u{1F3FF}' | '\u{E0020}'..='\u{E007F}' => {}
            '\u{1F1E6}'..='\u{1F1FF}' => {
                if !open_flag {
                    pictures += 1;
                }
                open_flag = !open_flag;
            }
            '0'..='9' | '#' | '*' if keycap => pictures += 1,
            c if is_pictograph(c) => {
                if !joined {
                    pictures += 1;
                }
                joined = false;
            }
            _ => return None,
        }
    }
    (pictures == 1).then(|| text.to_string())
}

/// The blocks Unicode puts emoji in, and the older symbols that became
/// emoji there.
fn is_pictograph(c: char) -> bool {
    matches!(c,
        '\u{1F000}'..='\u{1FAFF}'
        | '\u{2600}'..='\u{27BF}'
        | '\u{2300}'..='\u{23FF}'
        | '\u{2B00}'..='\u{2BFF}'
        | '\u{2190}'..='\u{21FF}'
        | '\u{25A0}'..='\u{25FF}'
        | '\u{2934}' | '\u{2935}' | '\u{3030}' | '\u{303D}' | '\u{3297}' | '\u{3299}'
        | '\u{00A9}' | '\u{00AE}' | '\u{203C}' | '\u{2049}' | '\u{2122}' | '\u{2139}'
    )
}

/// The Message-ID `In-Reply-To` names, in angle brackets, when it names
/// exactly one, as Gmail requires of a reaction.
pub fn answered(in_reply_to: &str) -> Option<String> {
    let ids: Vec<&str> = in_reply_to
        .split('<')
        .skip(1)
        .filter_map(|rest| rest.split_once('>').map(|(id, _)| id.trim()))
        .filter(|id| !id.is_empty())
        .collect();
    match ids.as_slice() {
        [id] => Some(format!("<{id}>")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_emoji_takes_the_compound_ones_and_refuses_words_and_rows() {
        for single in ["👍", " ❤️ ", "👍🏽", "👨‍👩‍👧", "🇮🇹", "1️⃣", "☺️"] {
            assert!(one_emoji(single).is_some(), "{single:?} is one emoji");
        }
        for not_one in ["", "ok", "👍👍", "🇮🇹🇫🇷", "👍 thanks", "1", ":)"] {
            assert_eq!(one_emoji(not_one), None, "{not_one:?} is not one emoji");
        }
    }

    #[test]
    fn a_gmail_part_reads_only_as_version_one_with_one_emoji() {
        assert_eq!(from_gmail_part(r#"{"version":1,"emoji":"👍"}"#).as_deref(), Some("👍"));
        assert_eq!(from_gmail_part(&gmail_part("🙏")).as_deref(), Some("🙏"));
        assert_eq!(from_gmail_part(r#"{"version":2,"emoji":"👍"}"#), None);
        assert_eq!(from_gmail_part(r#"{"version":"1","emoji":"👍"}"#), None);
        assert_eq!(from_gmail_part(r#"{"version":1,"emoji":"👍👍"}"#), None);
        assert_eq!(from_gmail_part(r#"{"version":1}"#), None);
        assert_eq!(from_gmail_part("not json"), None);
    }

    #[test]
    fn a_reaction_answers_one_message_only() {
        assert_eq!(answered(" <a@x.example> ").as_deref(), Some("<a@x.example>"));
        assert_eq!(answered("<a@x.example> <b@x.example>"), None);
        assert_eq!(answered("a@x.example"), None);
    }
}
