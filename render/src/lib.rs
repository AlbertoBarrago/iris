//! Mail HTML made safe to show. Both front ends render a message through
//! here, the GTK app and the SwiftUI one, so a message looks the same and
//! passes the same cleaning in either.

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
