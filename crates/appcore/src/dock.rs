//! The Dock icon's badge: what it reads for the unread conversations in
//! every inbox. Each front end puts the label on its own Dock tile.

/// What the badge reads for `unread` conversations: nothing at none, and a
/// capped figure past what the Dock's small red disc fits.
pub fn badge_label(unread: i64) -> Option<String> {
    match unread {
        ..=0 => None,
        1..=999 => Some(unread.to_string()),
        _ => Some("999+".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::badge_label;

    #[test]
    fn no_unread_mail_leaves_the_icon_bare() {
        assert_eq!(badge_label(0), None);
    }

    #[test]
    fn the_badge_counts_up_to_what_it_fits() {
        assert_eq!(badge_label(1).as_deref(), Some("1"));
        assert_eq!(badge_label(999).as_deref(), Some("999"));
        assert_eq!(badge_label(1500).as_deref(), Some("999+"));
    }
}
