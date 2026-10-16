//! Which clock times print in: the grid's hour labels, cards, popovers,
//! the editor, search results, agenda rows, notifications and the mail
//! card all print a time of day through [`time_text`], the one function.
//! The clock is the one the system locale uses.

use std::cell::Cell;

use chrono::NaiveTime;
use mailrs_domain::calendar::clock::ClockFormat;
use mailrs_domain::translate::date_locale;

thread_local! {
    static CACHED: Cell<Option<ClockFormat>> = const { Cell::new(None) };
}

/// The clock to print times in, read once and cached the way
/// [`mailrs_domain::translate::date_locale`] caches the date's own
/// locale.
pub fn current() -> ClockFormat {
    CACHED.with(|cell| {
        cell.get().unwrap_or_else(|| {
            let format = match crate::locale_time::prefers_12_hour() {
                true => ClockFormat::Hour12,
                false => ClockFormat::Hour24,
            };
            cell.set(Some(format));
            format
        })
    })
}

/// "15:05" or "3:05 PM": `at`'s time of day in the clock [`current`]
/// names.
pub fn time_text(at: NaiveTime) -> String {
    mailrs_domain::calendar::clock::format_time(at, current(), date_locale())
}

/// Reads `text` back as a time of day, in either clock: a dropdown a
/// person may have typed into as well as picked from.
pub fn parse_time_text(text: &str) -> Option<NaiveTime> {
    mailrs_domain::calendar::clock::parse_time(text, date_locale())
}
