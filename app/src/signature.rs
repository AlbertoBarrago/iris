//! Formatted signatures: the HTML a company hands its people to put under
//! their mail, with a logo, certification badges, social icons and a
//! table holding them in place. Read as text and written back out through
//! Markdown, such a signature loses every picture and runs its cells
//! together, so a formatted signature travels as a block of HTML of its
//! own, beside the writer's words and never inside the editor.
//!
//! [`clean`] is the one door in: whatever is pasted or imported goes
//! through it before it is kept, and again before it goes out. It keeps
//! what lays a signature out (tables, inline styles, pictures, links) and
//! drops what runs code, submits data, or styles the message around it.

use std::borrow::Cow;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::Path;

use ammonia::{Builder, UrlRelative};
use base64::Engine;
use mailrs_mime::html::{Piece, walk};

use crate::compose::OutgoingAttachment;

/// Tags a signature lays itself out with, beyond ammonia's defaults.
const EXTRA_TAGS: [&str; 4] = ["font", "center", "span", "div"];

/// Attributes that place and color things. `class` stays out: the
/// stylesheet it would point at is dropped, and in a reply it could pick
/// up the rules of the message being quoted.
const LAYOUT_ATTRIBUTES: [&str; 16] = [
    "style",
    "align",
    "valign",
    "width",
    "height",
    "bgcolor",
    "color",
    "border",
    "dir",
    "face",
    "size",
    "cellpadding",
    "cellspacing",
    "colspan",
    "rowspan",
    "nowrap",
];

/// The largest picture a signature may carry inside itself. A badge or a
/// logo is a few kilobytes; anything bigger is a mistake that would ride
/// along on every message.
const MAX_PICTURE: u64 = 512 * 1024;

/// `html` as a signature may hold it: layout kept, scripts, forms,
/// stylesheets and comments gone, links opening a new page. An empty
/// string when nothing visible is left.
pub fn clean(html: &str) -> String {
    clean_with(html, UrlRelative::Deny, |_| None)
}

/// [`clean`], with the pictures `html` names by a path, relative to `dir`
/// or as a `file:` address, read in as `data:` addresses. This is how a
/// signature saved as an `.htm` file next to a folder of pictures, the way
/// Outlook keeps them, comes in whole.
pub fn import(html: &str, dir: &Path) -> String {
    // Ammonia judges a relative address before the filter sees it, so the
    // first pass lets them through to be read in, and the second drops
    // whatever could not be.
    let dir = dir.to_path_buf();
    let read_in = clean_with(html, UrlRelative::PassThrough, move |src| {
        local_picture(src, &dir)
    });
    clean(&read_in)
}

fn clean_with(
    html: &str,
    relative: UrlRelative<'static>,
    rewrite: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
) -> String {
    let mut builder = Builder::default();
    builder
        .add_tags(&EXTRA_TAGS)
        .add_clean_content_tags(&["style", "title"])
        .add_generic_attributes(&LAYOUT_ATTRIBUTES)
        .url_schemes(HashSet::from(["http", "https", "mailto", "tel", "cid", "data"]))
        .url_relative(relative)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true)
        .attribute_filter(move |element, attribute, value| match (element, attribute) {
            ("img", "src") => Some(rewrite(value).map_or(Cow::Borrowed(value), Cow::Owned)),
            _ => Some(Cow::Borrowed(value)),
        });
    let cleaned = builder.clean(html).to_string();
    match visible(&cleaned) {
        true => cleaned.trim().to_string(),
        false => String::new(),
    }
}

/// Whether `html` shows anything: words or a picture.
fn visible(html: &str) -> bool {
    let mut seen = false;
    walk(html, |piece| match piece {
        Piece::Text(words) => seen |= !words.trim().is_empty(),
        Piece::Tag(tag) => seen |= tag.name == "img" && !tag.closing,
    });
    seen
}

/// The picture `src` names on this computer, as a `data:` address. `None`
/// for an address on the web, one already inline, and a file that is
/// missing, too large, or not a picture.
fn local_picture(src: &str, dir: &Path) -> Option<String> {
    let src = src.trim();
    let lower = src.to_ascii_lowercase();
    if ["http:", "https:", "data:", "cid:", "mailto:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        return None;
    }
    // Joined as addresses, so `%20` in a relative path reads as a space
    // the way the file writes it.
    let path = url::Url::from_directory_path(dir)
        .ok()?
        .join(src)
        .ok()?
        .to_file_path()
        .ok()?;
    let mime_type = picture_type(&path)?;
    if std::fs::metadata(&path).ok()?.len() > MAX_PICTURE {
        return None;
    }
    let data = std::fs::read(&path).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    Some(format!("data:{mime_type};base64,{encoded}"))
}

/// The media type of a picture file, by its extension.
fn picture_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// Whether `html` holds anything plain text would lose: a picture, a
/// table, a link, a color, a font or a weight. Gmail hands every
/// signature over as HTML, and one that is only lines of words is better
/// kept as text the writer can edit.
pub fn is_formatted(html: &str) -> bool {
    let mut formatted = false;
    walk(html, |piece| {
        let Piece::Tag(tag) = piece else { return };
        if tag.closing {
            return;
        }
        formatted |= matches!(
            tag.name,
            "img" | "table" | "a" | "b" | "strong" | "i" | "em" | "u" | "font" | "hr"
        ) || tag.attribute("style").is_some_and(|s| !s.trim().is_empty())
            || tag.attribute("color").is_some();
    });
    formatted
}

/// The text of an HTML file or clipboard, in whichever of the encodings
/// signatures come in: UTF-8, UTF-16 with its byte order mark, as Windows
/// writes clipboard HTML and some Outlook files, or else Windows-1252,
/// which older Outlook files are saved in.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| unit(*c)).collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(b"\xff\xfe") {
        return utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xfe\xff") {
        return utf16(rest, u16::from_be_bytes);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
    }
}

/// One byte of Windows-1252: Latin-1, but for the row of curly quotes,
/// dashes and the euro sign it puts where Latin-1 has control codes.
fn windows_1252(byte: u8) -> char {
    const ROW: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
        '\u{9d}', 'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9f => ROW[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// The signature as the text part of a message carries it.
pub fn to_text(html: &str) -> String {
    mailrs_mime::html::html_to_text(html)
}

/// `html` with each `data:` picture moved out into a part of its own and
/// named by `cid:`, which is where mail clients look for a picture inside
/// a message. Outlook and Gmail drop a `data:` address in a message
/// body, so a badge left that way never reaches the reader.
///
/// The same bytes always get the same `Content-ID`, so a draft saved and
/// sent again carries each picture once.
pub fn lift_pictures(html: &str) -> (String, Vec<OutgoingAttachment>) {
    if !html.contains("data:") {
        return (html.to_string(), Vec::new());
    }
    let mut pictures: Vec<OutgoingAttachment> = Vec::new();
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = find_data_src(rest) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let end = tail.find(['"', '\'', ' ', '>']).unwrap_or(tail.len());
        let address = &tail[..end];
        match picture_of(address) {
            Some(picture) => {
                let cid = picture.content_id.clone().unwrap_or_default();
                out.push_str("cid:");
                out.push_str(&cid);
                if !pictures.iter().any(|p| p.content_id == picture.content_id) {
                    pictures.push(picture);
                }
            }
            None => out.push_str(address),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    (out, pictures)
}

/// Where the next `data:` address in a `src` attribute starts.
fn find_data_src(html: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(found) = html[from..].find("data:") {
        let at = from + found;
        let before = html[..at].trim_end_matches(['"', '\'']);
        if before.trim_end().ends_with('=') && before[..before.len() - 1].ends_with("src") {
            return Some(at);
        }
        from = at + 5;
    }
    None
}

/// The picture a `data:` address holds, ready to go in as an inline part.
fn picture_of(address: &str) -> Option<OutgoingAttachment> {
    let (head, data) = address.strip_prefix("data:")?.split_once(',')?;
    let mime_type = head.strip_suffix(";base64")?;
    if !mime_type.starts_with("image/") {
        return None;
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    let id = format!("{:016x}", hasher.finish());
    let extension = mime_type
        .trim_start_matches("image/")
        .split('+')
        .next()
        .unwrap_or("png");
    Some(OutgoingAttachment {
        filename: format!("signature-{}.{extension}", &id[..8]),
        mime_type: mime_type.to_string(),
        data,
        content_id: Some(format!("signature-{id}@iris")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A company signature shaped like the ones that came out garbled: a
    /// phone row laid out in a table beside its icon, the VAT line, social
    /// icons as links, a logo, certification badges, and a sentence with a
    /// bold phrase. The names and numbers are made up.
    pub(crate) const COMPANY: &str = r#"<html><head><style>.MsoNormal{margin:0}</style></head><body>
<table cellpadding="0" cellspacing="0" style="font-family:Arial">
<tr><td valign="top"><img src="https://example.com/sig/phone.png" width="24" alt=""></td>
<td style="padding-left:12px">SS +39 079 000 0001<br>CA +39 070 000 0002</td></tr></table>
<p class="MsoNormal">VAT: IT00000000000 | SDI: ABCDEFG</p>
<p><a href="https://www.linkedin.com/company/example"><img src="https://example.com/sig/in.png" width="20" alt="LinkedIn"></a>
<a href="https://github.com/example" onclick="track()"><img src="https://example.com/sig/gh.png" width="20" alt="GitHub"></a></p>
<p><img src="https://example.com/sig/logo.png" width="220" alt="Example Apps"></p>
<p><img src="https://example.com/sig/iso-9001.png" width="220" alt="Certified ISO 9001"></p>
<p>Visit our <b>coworking space</b> in Sassari</p>
<script>alert(1)</script><!-- tracking -->
</body></html>"#;

    #[test]
    fn a_company_signature_keeps_its_layout_and_loses_what_runs() {
        let kept = clean(COMPANY);
        for wanted in [
            "<table",
            "cellpadding=\"0\"",
            "style=\"padding-left:12px\"",
            "src=\"https://example.com/sig/iso-9001.png\"",
            "href=\"https://github.com/example\"",
            "<b>coworking space</b>",
        ] {
            assert!(kept.contains(wanted), "{wanted} missing from {kept}");
        }
        for gone in ["<script", "alert", "onclick", "MsoNormal", "tracking", "<style"] {
            assert!(!kept.contains(gone), "{gone} left in {kept}");
        }
    }

    #[test]
    fn nothing_visible_is_no_signature() {
        assert_eq!(clean("<p> </p><script>x()</script>"), "");
        assert_eq!(clean(""), "");
        assert_ne!(clean("<img src=\"https://example.com/a.png\">"), "");
    }

    #[test]
    fn a_company_signature_reads_as_lines_of_text() {
        let text = to_text(&clean(COMPANY));
        assert!(text.contains("SS +39 079 000 0001\nCA +39 070 000 0002"), "{text}");
        assert!(text.contains("VAT: IT00000000000 | SDI: ABCDEFG"), "{text}");
        assert!(text.contains("Visit our coworking space in Sassari"), "{text}");
    }

    #[test]
    fn plain_lines_from_gmail_stay_text_and_anything_more_is_formatted() {
        assert!(!is_formatted(r#"<div dir="ltr">Dana<div>Maple &amp; Finch</div></div>"#));
        assert!(is_formatted(COMPANY));
        assert!(is_formatted("<div>Dana <b>Reyes</b></div>"));
        assert!(is_formatted(r#"<span style="color:#c00">Dana</span>"#));
    }

    #[test]
    fn signature_files_read_in_the_encodings_they_come_in() {
        assert_eq!(decode("Città".as_bytes()), "Città");
        assert_eq!(decode(b"\xef\xbb\xbfCitt\xc3\xa0"), "Città");
        assert_eq!(decode(b"Citt\xe0 \x80 \x96"), "Città € –");
        let utf16: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("Città".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode(&utf16), "Città");
    }

    #[test]
    fn inline_pictures_move_into_parts_once_each() {
        let png = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG fake");
        let html = format!(
            r#"<img src="data:image/png;base64,{png}" alt="a"><img alt="b" src='data:image/png;base64,{png}'>
<p>data: stays as words</p><img src="data:text/html;base64,PGI+">"#
        );
        let (lifted, pictures) = lift_pictures(&html);
        assert_eq!(pictures.len(), 1);
        let cid = pictures[0].content_id.clone().unwrap();
        assert_eq!(lifted.matches(&format!("cid:{cid}")).count(), 2, "{lifted}");
        assert!(lifted.contains("data: stays as words"));
        assert!(lifted.contains("data:text/html"), "only pictures move: {lifted}");
        assert_eq!(pictures[0].data, b"\x89PNG fake");
        assert_eq!(pictures[0].mime_type, "image/png");
        // The same picture gets the same name on the next build.
        assert_eq!(lift_pictures(&html).1[0].content_id, Some(cid));
    }

    #[test]
    fn an_imported_file_brings_its_pictures_along() {
        let dir = std::env::temp_dir().join(format!("iris-signature-{}", std::process::id()));
        let files = dir.join("Work_files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("logo one.png"), b"logo").unwrap();
        std::fs::write(files.join("notes.txt"), b"text").unwrap();
        let html = r#"<p><img src="Work_files/logo%20one.png"><img src="Work_files/notes.txt">
<img src="Work_files/missing.png"><img src="https://example.com/b.png"></p>"#;
        let kept = import(html, &dir);
        let logo = base64::engine::general_purpose::STANDARD.encode(b"logo");
        assert!(kept.contains(&format!("src=\"data:image/png;base64,{logo}\"")), "{kept}");
        assert!(kept.contains("src=\"https://example.com/b.png\""), "{kept}");
        // A path that is no picture, or no file, loses its source rather
        // than pointing the reader at this computer.
        assert!(!kept.contains("notes.txt") && !kept.contains("missing.png"), "{kept}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
