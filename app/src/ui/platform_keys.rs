//! Shortcuts on macOS. The key tables name shortcuts with `<Control>`, as
//! they were written for Linux, and the tooltips name them in words, such
//! as "Ctrl+Shift+A". A Mac takes the same shortcuts with Command. So on
//! macOS a trigger is read with Command in Control's place, a press counts
//! Command where Linux counts Control, and the keys a tooltip names read
//! in the Mac's symbols, "⇧⌘A". The tables and the translated words stay
//! as they are, which keeps Linux unchanged and merges from upstream small.

use std::borrow::Cow;

use gtk::gdk;

const MAC: bool = cfg!(target_os = "macos");

/// The modifier a shortcut's `<Control>` stands for: Command on macOS,
/// which GDK reports as Meta, and Control elsewhere.
pub const PRIMARY: gdk::ModifierType = if MAC {
    gdk::ModifierType::META_MASK
} else {
    gdk::ModifierType::CONTROL_MASK
};

/// The modifiers that make a press some other shortcut than one taking
/// only the primary modifier and Shift. On macOS Control is one of them.
/// Off macOS Super is; on macOS GDK may report Command with Super, so it
/// is left out there.
pub fn others() -> gdk::ModifierType {
    if MAC {
        gdk::ModifierType::ALT_MASK | gdk::ModifierType::CONTROL_MASK
    } else {
        gdk::ModifierType::ALT_MASK | gdk::ModifierType::SUPER_MASK
    }
}

/// `accelerator` with its modifier for this platform, for
/// `ShortcutTrigger::parse_string` and the shortcuts dialog.
pub fn trigger(accelerator: &str) -> Cow<'_, str> {
    if MAC { mac_trigger(accelerator) } else { Cow::Borrowed(accelerator) }
}

/// `text` with the key chords it names written as this platform writes
/// them: unchanged off macOS, Mac symbols on it.
pub fn label(text: &str) -> Cow<'_, str> {
    if MAC { mac_label(text) } else { Cow::Borrowed(text) }
}

/// [`label`] for a translated tooltip, kept as the owned string it was.
pub fn tip(text: String) -> String {
    match label(&text) {
        Cow::Borrowed(_) => text,
        Cow::Owned(changed) => changed,
    }
}

fn mac_trigger(accelerator: &str) -> Cow<'_, str> {
    if accelerator.contains("<Control>") {
        Cow::Owned(accelerator.replace("<Control>", "<Meta>"))
    } else {
        Cow::Borrowed(accelerator)
    }
}

/// The modifier words a tooltip may hold, in the languages Iris speaks,
/// and the Mac symbol for each. Control becomes Command, as the shortcut
/// itself does.
const MODIFIERS: [(&str, char); 4] = [("Ctrl", '⌘'), ("Alt", '⌥'), ("Shift", '⇧'), ("Maiusc", '⇧')];

/// The order a Mac writes modifier symbols in.
const MAC_ORDER: [char; 4] = ['⌃', '⌥', '⇧', '⌘'];

/// Rewrites each chord such as "Ctrl+Alt+A" in `text` as the Mac writes
/// it, "⌥⌘A". A chord is one or more modifier words, each followed by a
/// plus, then a key word. Text that holds none comes back borrowed.
fn mac_label(text: &str) -> Cow<'_, str> {
    let mut out = String::new();
    let mut rest = text;
    let mut changed = false;
    while !rest.is_empty() {
        let at_word_start = out.chars().last().is_none_or(|c| !c.is_alphanumeric());
        if at_word_start && let Some((chord, after)) = chord(rest) {
            out.push_str(&chord);
            rest = after;
            changed = true;
            continue;
        }
        let next = rest.chars().next().expect("rest is not empty");
        out.push(next);
        rest = &rest[next.len_utf8()..];
    }
    if changed { Cow::Owned(out) } else { Cow::Borrowed(text) }
}

/// The chord `text` starts with, in Mac symbols, and the text after it.
fn chord(text: &str) -> Option<(String, &str)> {
    let mut held = Vec::new();
    let mut rest = text;
    'words: loop {
        for (word, symbol) in MODIFIERS {
            if let Some(after) = rest.strip_prefix(word).and_then(|r| r.strip_prefix('+')) {
                held.push(symbol);
                rest = after;
                continue 'words;
            }
        }
        break;
    }
    if held.is_empty() {
        return None;
    }
    let key_len: usize = rest
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .map(char::len_utf8)
        .sum();
    if key_len == 0 {
        return None;
    }
    let (key, after) = rest.split_at(key_len);
    let mut symbols: String = MAC_ORDER.iter().filter(|s| held.contains(s)).collect();
    symbols.push_str(&mac_key(key));
    Some((symbols, after))
}

/// A key's name as a Mac shows it: a letter in capitals, Enter as ↩.
fn mac_key(key: &str) -> Cow<'_, str> {
    match key {
        "Enter" | "Return" | "Invio" => Cow::Borrowed("↩"),
        "Up" => Cow::Borrowed("↑"),
        "Down" => Cow::Borrowed("↓"),
        _ if key.chars().count() == 1 => Cow::Owned(key.to_uppercase()),
        _ => Cow::Borrowed(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trigger_takes_command_for_control() {
        assert_eq!(mac_trigger("<Control><Shift>a"), "<Meta><Shift>a");
        assert_eq!(mac_trigger("e"), "e");
    }

    #[test]
    fn chords_in_a_tooltip_read_in_mac_symbols() {
        assert_eq!(mac_label("Archive (E or Ctrl+Alt+A)"), "Archive (E or ⌥⌘A)");
        assert_eq!(mac_label("Junk (Ctrl+Shift+J)"), "Junk (⇧⌘J)");
        assert_eq!(mac_label("Allega file (Ctrl+Maiusc+A)"), "Allega file (⇧⌘A)");
        assert_eq!(mac_label("Invia (Ctrl+Invio)"), "Invia (⌘↩)");
        assert_eq!(mac_label("Send (Ctrl+Enter)"), "Send (⌘↩)");
    }

    #[test]
    fn words_that_only_look_like_modifiers_stay() {
        assert_eq!(mac_label("Shift work"), "Shift work");
        assert_eq!(mac_label("Alt+"), "Alt+");
        assert_eq!(mac_label("SubCtrl+A"), "SubCtrl+A");
        assert!(matches!(mac_label("No keys here"), Cow::Borrowed(_)));
    }
}
