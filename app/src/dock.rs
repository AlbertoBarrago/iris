//! The Dock icon's badge: the unread count of every inbox.

/// How long the badge waits for a burst of changes to end before it counts.
pub const RECOUNT_AFTER: std::time::Duration = std::time::Duration::from_millis(500);

/// Lets a burst of requests through as one. The first request in a quiet
/// spell claims the next run, and the rest ride along with it until it
/// starts.
#[derive(Default)]
pub struct Burst(std::cell::Cell<bool>);

impl Burst {
    /// Whether this request should schedule the run.
    pub fn claim(&self) -> bool {
        !self.0.replace(true)
    }

    /// The run is starting, so a request from here on needs one of its own.
    pub fn start(&self) {
        self.0.set(false);
    }
}

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
    use super::{Burst, badge_label};

    #[test]
    fn a_burst_of_changes_counts_once() {
        let burst = Burst::default();
        assert!(burst.claim());
        assert!((0..300).all(|_| !burst.claim()));
        burst.start();
        assert!(
            burst.claim(),
            "a change after the count starts counts again"
        );
    }

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
