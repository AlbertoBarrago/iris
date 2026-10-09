//! The media type a file goes out with. GIO on macOS names files by
//! Apple's uniform type identifiers and turns them into media types
//! loosely, so a plain text file came out as `text/*`. Apple's own
//! `UTType` knows each extension's preferred media type; GIO's guess from
//! the bytes stands in when the name has no extension Apple knows.

use mailrs_appcore::file_type::by_extension;

/// The media type for `filename` holding `data`, `application/octet-stream`
/// when nothing knows better.
pub fn mime_type(filename: &str, data: &[u8]) -> String {
    by_extension(filename)
        .or_else(|| {
            let (guess, _) = gtk::gio::content_type_guess(Some(filename), data);
            gtk::gio::content_type_get_mime_type(&guess)
                .map(|m| m.to_string())
                // A wildcard is GIO giving up, not an answer.
                .filter(|m| !m.ends_with("/*"))
        })
        .unwrap_or_else(|| "application/octet-stream".into())
}

#[cfg(test)]
mod tests {
    use super::mime_type;

    #[test]
    fn common_files_get_their_exact_media_type() {
        assert_eq!(mime_type("plan.txt", b"hello"), "text/plain");
        assert_eq!(mime_type("scan.pdf", b"%PDF-1.7"), "application/pdf");
        assert_eq!(mime_type("photo.JPG", b""), "image/jpeg");
        assert_eq!(mime_type("invite.ics", b"BEGIN:VCALENDAR"), "text/calendar");
    }

    #[test]
    fn a_file_nothing_recognizes_goes_as_bytes() {
        assert_eq!(mime_type("blob.zzqx", &[0, 159, 146, 150]), "application/octet-stream");
    }
}
