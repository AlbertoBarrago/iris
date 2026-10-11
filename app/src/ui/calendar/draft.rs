//! The editor's draft lives in appcore (`calendar_draft`), shared with
//! Iris Next; the desktop's zone is read through GLib here.

pub use mailrs_appcore::calendar_draft::*;
use chrono_tz::Tz;

/// The zone the desktop is set to, for a new draft nothing else names one
/// for. `Tz::UTC` when GLib's own identifier does not parse as one.
pub fn local_zone() -> Tz {
    gtk::glib::TimeZone::local()
        .identifier()
        .as_str()
        .parse()
        .unwrap_or(Tz::UTC)
}

