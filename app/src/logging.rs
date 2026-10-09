//! Where the app's log lines go, and what a release build keeps out of them.
//!
//! Each line goes to standard error and to `~/Library/Logs/Iris/iris.log`,
//! where Console.app finds it. An app opened from the Dock has nowhere to
//! send standard error, so without the file a problem left no trace. Every
//! run adds to the file, so a second launch that hands a link to the
//! running copy leaves that copy's lines in place; past 5 MB the file
//! becomes `iris.log.1` and a new one starts. The file is the person's
//! own, and it outlives the mail. Error text from Google and
//! from sending can carry addresses, so a release build masks every email
//! address on its way out: `dana.reyes@example.com` becomes
//! `d…@example.com`. A debug build keeps them, and so does a release build
//! started with `IRIS_LOG_DETAILS=1`, for looking into a problem on
//! an installed copy.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use tracing_subscriber::fmt::MakeWriter;

/// The variable that keeps addresses in a release build's log.
const DETAILS: &str = "IRIS_LOG_DETAILS";

/// This run's log file, once `open_file` made it.
static FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

/// Where the log file goes: `~/Library/Logs/Iris`.
fn folder() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs/Iris"))
}

/// How large the log file grows before it starts again.
const MOST_BYTES: u64 = 5 * 1024 * 1024;

/// Opens the log file for this run to add to. A file that cannot be made
/// leaves the log on standard error alone.
pub fn open_file() {
    let Some(folder) = folder() else { return };
    if let Err(err) = std::fs::create_dir_all(&folder) {
        eprintln!("could not make the log folder {}: {err}", folder.display());
        return;
    }
    let path = folder.join("iris.log");
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MOST_BYTES) {
        let _ = std::fs::rename(&path, folder.join("iris.log.1"));
    }
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => {
            let _ = FILE.set(Mutex::new(file));
        }
        Err(err) => eprintln!("could not start the log file {}: {err}", path.display()),
    }
}

/// Whether this run's log keeps addresses whole.
pub fn details() -> bool {
    cfg!(debug_assertions) || std::env::var_os(DETAILS).is_some_and(|v| v != "0")
}

/// Writes each log event to standard error, masking addresses unless
/// `details` is set.
#[derive(Clone, Copy)]
pub struct Writer {
    pub details: bool,
}

impl<'a> MakeWriter<'a> for Writer {
    type Writer = Event;

    fn make_writer(&'a self) -> Event {
        Event {
            details: self.details,
            line: Vec::new(),
        }
    }
}

/// One event's text, held until the formatter is done with it so an
/// address split across two writes is still seen whole.
pub struct Event {
    details: bool,
    line: Vec<u8>,
}

impl Write for Event {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.line.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        let text = String::from_utf8_lossy(&self.line);
        let text = if self.details {
            text.into_owned()
        } else {
            mask_addresses(&text)
        };
        let _ = io::stderr().lock().write_all(text.as_bytes());
        if let Some(file) = FILE.get()
            && let Ok(mut file) = file.lock()
        {
            let _ = file.write_all(text.as_bytes());
        }
    }
}

/// `text` with the local part of each email address cut to its first
/// letter. The domain stays, since which provider failed is worth knowing
/// and names nobody.
pub fn mask_addresses(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let local = |c: char| c.is_alphanumeric() || "._%+-".contains(c);
    let domain = |c: char| c.is_alphanumeric() || ".-".contains(c);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '@' {
            let start = (0..i).rev().take_while(|&j| local(chars[j])).last();
            let end = (i + 1..chars.len())
                .take_while(|&j| domain(chars[j]))
                .last();
            if let (Some(start), Some(end)) = (start, end) {
                let host: String = chars[i + 1..=end].iter().collect();
                let host = host.trim_end_matches('.');
                if host.contains('.') && !host.starts_with('.') {
                    // The local part is already in `out`; take it back.
                    for _ in start..i {
                        out.pop();
                    }
                    out.push(chars[start]);
                    out.push('…');
                    out.push('@');
                    out.push_str(host);
                    i = i + 1 + host.chars().count();
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::mask_addresses;

    #[test]
    fn an_address_keeps_its_first_letter_and_its_domain() {
        assert_eq!(
            mask_addresses("could not start syncing account=dana.reyes@example.com"),
            "could not start syncing account=d…@example.com"
        );
    }

    #[test]
    fn every_address_in_a_line_is_masked() {
        assert_eq!(
            mask_addresses("rejected <ann+news@mail.example.org>, bo@x.co."),
            "rejected <a…@mail.example.org>, b…@x.co."
        );
    }

    #[test]
    fn text_without_an_address_is_left_alone() {
        for text in [
            "an @ on its own",
            "user@localhost",
            "handle @dana",
            "error: HTTP 403 for https://gmail.googleapis.com/gmail/v1/users/me",
            "ümlaut ünd 日本語",
        ] {
            assert_eq!(mask_addresses(text), text);
        }
    }
}
