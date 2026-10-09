//! Text for the calendar's event lines, which need the calendar's own
//! words. Everything else about dates, sizes, initials and colours lives
//! in `mailrs_render::format`, which both front ends share.

pub use mailrs_render::format::*;

use chrono::{DateTime, Datelike, Local, NaiveDate};
use mailrs_domain::EpochMillis;
use mailrs_domain::invitation::When;
use mailrs_domain::translate::{date_locale, fill, gettext};

/// When an event runs, in the reader's own time zone, in the words the
/// calendar's event popover uses ([`crate::ui::calendar::words::span_words`])
/// so an invitation card and the event agree: "Tuesday, 9 June 2026 ·
/// 15:00–16:00", "Tuesday, 14 July 2026 · All day", or "Tuesday 14 –
/// Thursday 16 July". The day is a [`long_date`], which names the year.
pub fn event_when(when: &When, now: DateTime<Local>) -> String {
    use crate::ui::calendar::words;
    match when {
        When::Days { first, last } if first == last => fill(
            &gettext("{date} · All day"),
            &[("date", &long_date(*first))],
        ),
        When::Days { first, last } => words::all_day_range_words(*first, *last),
        When::At { starts_at, ends_at } => {
            let Some(start) = local(*starts_at) else {
                return String::new();
            };
            let date = long_date(start.date_naive());
            let from = crate::clock_format::time_text(start.time());
            let Some(end) = ends_at.and_then(local) else {
                return fill(&gettext("{date} · {start}"), &[("date", &date), ("start", &from)]);
            };
            // A meeting that runs past midnight names the day it ends on.
            let until = if end.date_naive() == start.date_naive() {
                crate::clock_format::time_text(end.time())
            } else {
                fill(
                    &gettext("{date} {time}"),
                    &[
                        ("date", &short_date(end.date_naive(), now.date_naive())),
                        ("time", &crate::clock_format::time_text(end.time())),
                    ],
                )
            };
            fill(
                &gettext("{date} · {start}–{end}"),
                &[("date", &date), ("start", &from), ("end", &until)],
            )
        }
    }
}

/// The day an event falls on: "Today", "Tomorrow", a weekday within the
/// week, then the date.
fn event_day(day: NaiveDate, now: DateTime<Local>) -> String {
    let pattern = match (day - now.date_naive()).num_days() {
        0 => return gettext("Today"),
        1 => return gettext("Tomorrow"),
        -1 => return gettext("Yesterday"),
        2..=6 => gettext("%A"),
        _ if day.year() == now.year() => gettext("%A, %-d %B"),
        _ => gettext("%A, %-d %B %Y"),
    };
    day.format_localized(&pattern, date_locale()).to_string()
}

/// The start an event had before it moved, for the line that says so:
/// "Tuesday 10:00", or "Tuesday, 14 July" for an all-day one.
pub fn event_moved_from(was: EpochMillis, all_day: bool, now: DateTime<Local>) -> String {
    let Some(start) = local(was) else {
        return String::new();
    };
    let day = event_day(start.date_naive(), now);
    if all_day {
        day
    } else {
        fill(
            &gettext("{day} {time}"),
            &[("day", &day), ("time", &crate::clock_format::time_text(start.time()))],
        )
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Milliseconds for a local date and time.
    fn at_local(y: i32, m: u32, d: u32, h: u32, min: u32) -> EpochMillis {
        Local
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn an_event_reads_as_the_event_popover_words_it() {
        let now = Local.with_ymd_and_hms(2026, 9, 19, 15, 0, 0).unwrap();
        let at = |start: EpochMillis, end: Option<EpochMillis>| {
            event_when(
                &When::At {
                    starts_at: start,
                    ends_at: end,
                },
                now,
            )
        };
        assert_eq!(
            at(
                at_local(2026, 9, 19, 16, 0),
                Some(at_local(2026, 9, 19, 17, 0))
            ),
            "Saturday, 19 September 2026 · 16:00–17:00"
        );
        assert_eq!(
            at(
                at_local(2026, 9, 20, 9, 30),
                Some(at_local(2026, 9, 20, 10, 15))
            ),
            "Sunday, 20 September 2026 · 09:30–10:15"
        );
        assert_eq!(
            at(
                at_local(2026, 9, 22, 14, 0),
                Some(at_local(2026, 9, 22, 14, 45))
            ),
            "Tuesday, 22 September 2026 · 14:00–14:45"
        );
        assert_eq!(
            at(at_local(2026, 11, 3, 14, 0), None),
            "Tuesday, 3 November 2026 · 14:00"
        );
        assert_eq!(
            at(
                at_local(2027, 1, 4, 9, 0),
                Some(at_local(2027, 1, 4, 10, 0))
            ),
            "Monday, 4 January 2027 · 09:00–10:00"
        );
        // A meeting that runs past midnight names the day it ends on.
        assert_eq!(
            at(
                at_local(2026, 11, 3, 23, 0),
                Some(at_local(2026, 11, 4, 1, 0))
            ),
            "Tuesday, 3 November 2026 · 23:00–Wed, 4 Nov 01:00"
        );
    }

    #[test]
    fn an_all_day_event_says_so() {
        let now = Local.with_ymd_and_hms(2026, 9, 19, 15, 0, 0).unwrap();
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(
            event_when(
                &When::Days {
                    first: day(2026, 7, 14),
                    last: day(2026, 7, 14)
                },
                now
            ),
            "Tuesday, 14 July 2026 · All day"
        );
        assert_eq!(
            event_when(
                &When::Days {
                    first: day(2026, 7, 14),
                    last: day(2026, 7, 16)
                },
                now
            ),
            "Tuesday 14 – Thursday 16 July"
        );
        assert_eq!(
            event_when(
                &When::Days {
                    first: day(2026, 6, 30),
                    last: day(2026, 7, 2)
                },
                now
            ),
            "Tuesday 30 June – Thursday 2 July"
        );
    }

    #[test]
    fn a_moved_meeting_names_where_it_was() {
        let now = Local.with_ymd_and_hms(2026, 9, 19, 15, 0, 0).unwrap();
        assert_eq!(
            event_moved_from(at_local(2026, 9, 22, 10, 0), false, now),
            "Tuesday 10:00"
        );
        assert_eq!(
            event_moved_from(at_local(2026, 9, 22, 0, 0), true, now),
            "Tuesday"
        );
    }
}
