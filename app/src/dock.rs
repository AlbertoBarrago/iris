//! The Dock icon's badge on macOS: the unread count that the tray carries
//! on Linux, where the Dock has no counterpart.

/// What the badge reads for `unread` conversations: nothing at none, and a
/// capped figure past what the Dock's small red disc fits.
pub fn badge_label(unread: i64) -> Option<String> {
    match unread {
        ..=0 => None,
        1..=999 => Some(unread.to_string()),
        _ => Some("999+".to_string()),
    }
}

/// Shows `unread` on the Dock icon. Called on GTK's thread, which on macOS
/// is the main thread AppKit wants.
#[cfg(target_os = "macos")]
pub fn set_badge(unread: i64) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("the Dock badge was asked for off the main thread");
        return;
    };
    tracing::debug!(unread, "Dock badge");
    let label = badge_label(unread).map(|text| NSString::from_str(&text));
    NSApplication::sharedApplication(mtm)
        .dockTile()
        .setBadgeLabel(label.as_deref());
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
