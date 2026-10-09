//! Mail HTML made safe to show. Both front ends render a message through
//! here, the GTK app and the SwiftUI one, so a message looks the same and
//! passes the same cleaning in either.

pub mod calendar_layout;
pub mod calendar_words;
pub mod clock;
pub mod conversation;
pub mod format;
pub mod locale_time;
pub mod quoted;
pub mod sanitize;

/// A content id as it goes into an address: letters, digits and `-._~@`
/// as they are, every other byte as `%XX`.
pub fn escape_cid(cid: &str) -> String {
    let mut out = String::with_capacity(cid.len());
    for byte in cid.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'@' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Whether `html` points at the inline image `cid` from one of its tags,
/// such as an image's `src` or a cell's `background`. The whole id has to
/// match, because `cid:logo` and `cid:logo2` name two different images.
/// Text that mentions the id, and a comment, point at nothing.
pub fn refers_to_cid(html: &str, cid: &str) -> bool {
    let needle = format!("cid:{cid}");
    let names = |value: &str| {
        value.match_indices(&needle).any(|(at, _)| {
            value[at + needle.len()..].chars().next().is_none_or(|c| {
                !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_' | '.' | '@' | '+')
            })
        })
    };
    let mut found = false;
    mailrs_mime::html::walk(html, |piece| {
        if let mailrs_mime::html::Piece::Tag(tag) = piece
            && !found
        {
            found = tag.values().any(names);
        }
    });
    found
}
