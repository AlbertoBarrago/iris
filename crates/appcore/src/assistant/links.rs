//! The links the assistant writes into its answers. A conversation or an
//! event it names is a Markdown link to an `iris:` address, which opens
//! that conversation or event in the window; any other link opens in the
//! browser. The model writes these addresses itself, from the ids its tools
//! gave, so the reading here is forgiving about how it spells them and
//! strict about what it opens.

/// The scheme of the addresses Iris opens itself.
pub const SCHEME: &str = "iris";

/// What a link in an answer opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// A conversation, by the account's address and the thread id the
    /// mail tools gave.
    Conversation { account: String, thread: String },
    /// An event, by the account's address and the id `list_events` gave,
    /// which names one occurrence of a repeating event.
    Event { account: String, id: String },
    /// A page on the web, opened in the browser.
    Web(String),
    /// An `iris:` address that names nothing Iris opens, or a link of a
    /// kind no browser should be handed.
    Unknown,
}

/// Reads `url` as an answer's link. The query is split by hand rather than
/// percent-decoded whole: Graph's ids hold `+`, `/` and `=`, which a
/// form decoder would turn into a space or split on.
pub fn read(url: &str) -> Link {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once(':') else {
        return Link::Unknown;
    };
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        return Link::Web(url.to_string());
    }
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return Link::Unknown;
    }
    let rest = rest.trim_start_matches('/');
    let (kind, query) = rest.split_once('?').unwrap_or((rest, ""));
    let field = |name: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| decode(value))
            .filter(|value| !value.is_empty())
    };
    let Some(account) = field("account") else {
        return Link::Unknown;
    };
    match kind.trim_end_matches('/').to_ascii_lowercase().as_str() {
        "conversation" | "thread" => match field("thread") {
            Some(thread) => Link::Conversation { account, thread },
            None => Link::Unknown,
        },
        "event" => match field("id") {
            Some(id) => Link::Event { account, id },
            None => Link::Unknown,
        },
        _ => Link::Unknown,
    }
}

/// The address of a conversation, as the prompt shows the model.
pub fn conversation(account: &str, thread: &str) -> String {
    format!("{SCHEME}://conversation?account={account}&thread={thread}")
}

/// The address of an event, as the prompt shows the model.
pub fn event(account: &str, id: &str) -> String {
    format!("{SCHEME}://event?account={account}&id={id}")
}

/// Undoes `%XX` escapes, which a link the model or the Markdown reader
/// encoded carries, and leaves every other character as written, `+`
/// included. An escape that is not one stays as it is.
fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = bytes
                .get(i + 1..i + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_link_names_its_account_and_thread() {
        assert_eq!(
            read(&conversation("me@example.com", "18f2a")),
            Link::Conversation { account: "me@example.com".into(), thread: "18f2a".into() }
        );
    }

    #[test]
    fn graph_ids_keep_their_plus_slash_and_equals() {
        let id = "AAQkADlh+MTc5/Y2Rh=";
        assert_eq!(
            read(&conversation("a@c22.example", id)),
            Link::Conversation { account: "a@c22.example".into(), thread: id.into() }
        );
    }

    #[test]
    fn an_encoded_link_reads_the_same() {
        assert_eq!(
            read("iris://conversation?account=me%40example.com&thread=AAQk%2Bx%3D"),
            Link::Conversation { account: "me@example.com".into(), thread: "AAQk+x=".into() }
        );
    }

    #[test]
    fn an_event_link_names_one_occurrence() {
        assert_eq!(
            read(&event("me@example.com", "abc_20261022T090000Z")),
            Link::Event { account: "me@example.com".into(), id: "abc_20261022T090000Z".into() }
        );
    }

    #[test]
    fn web_links_go_to_the_browser_and_others_go_nowhere() {
        assert_eq!(read("https://example.com/a?b=c"), Link::Web("https://example.com/a?b=c".into()));
        assert_eq!(read("file:///etc/passwd"), Link::Unknown);
        assert_eq!(read("javascript:alert(1)"), Link::Unknown);
        assert_eq!(read("iris://conversation?thread=1"), Link::Unknown, "no account");
        assert_eq!(read("iris://folder?account=a@b.c&id=1"), Link::Unknown);
        assert_eq!(read("iris://event?account=a@b.c"), Link::Unknown, "no id");
        assert_eq!(read("not a link"), Link::Unknown);
    }

    /// The links the prompt shows the model are ones this reads, so the
    /// two cannot drift apart.
    #[test]
    fn the_prompt_teaches_links_this_reads() {
        let prompt = crate::assistant::SYSTEM_PROMPT;
        let examples: Vec<&str> = prompt
            .split('(')
            .filter_map(|after| after.split_once(')').map(|(link, _)| link))
            .filter(|link| link.starts_with("iris:"))
            .collect();
        assert_eq!(examples.len(), 2, "{examples:?}");
        assert!(matches!(read(examples[0]), Link::Conversation { .. }), "{}", examples[0]);
        assert!(matches!(read(examples[1]), Link::Event { .. }), "{}", examples[1]);
    }

    #[test]
    fn a_stray_percent_stays_as_written() {
        assert_eq!(
            read("iris://conversation?account=a@b.c&thread=50%off"),
            Link::Conversation { account: "a@b.c".into(), thread: "50%off".into() }
        );
    }
}
