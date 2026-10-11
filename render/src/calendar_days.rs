//! The days a calendar occurrence covers in a grid of whole days, as the
//! month view lays them out. Both front ends draw the month from it.

use chrono::{Days, NaiveDate, TimeZone};
use mailrs_domain::EpochMillis;
use mailrs_domain::calendar::Occurrence;

/// The days `o` covers, counted from the grid's `first` day, end
/// exclusive: an all-day occurrence by its own UTC dates, a timed one by
/// the wall dates in `zone` of its start and of its last instant, so a
/// meeting that ends at midnight stays on one day.
pub fn grid_days<Z: chrono::TimeZone>(o: &Occurrence, first: NaiveDate, zone: &Z) -> (i64, i64) {
    let date = |at: EpochMillis, local: bool| {
        let utc = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(at).unwrap_or_default();
        if local {
            utc.with_timezone(zone).date_naive()
        } else {
            utc.date_naive()
        }
    };
    let (start, end) = if o.event.all_day {
        let start = date(o.start, false);
        (start, date(o.end, false).max(start + Days::new(1)))
    } else {
        let start = date(o.start, true);
        (start, date(o.end.saturating_sub(1).max(o.start), true) + Days::new(1))
    };
    ((start - first).num_days(), (end - first).num_days())
}

/// The columns of `days` a span from `start` to `end`: UTC dates for an
/// all-day one, local dates for a timed one that covers whole days.
pub fn strip_columns(start: EpochMillis, end: EpochMillis, all_day: bool, days: &[NaiveDate]) -> Option<(usize, usize)> {
    if all_day {
        return all_day_columns(start, end, days);
    }
    let (first, last) = crate::calendar_layout::whole_days(start, end, &chrono::Local)?;
    date_columns(first, last, days)
}

/// Whether the strip draws `o` rather than the grid: an all-day event,
/// or a timed entry that covers whole days (`crate::calendar_layout::whole_days`).
pub fn in_strip(o: &Occurrence) -> bool {
    o.event.all_day || crate::calendar_layout::whole_days(o.start, o.end, &chrono::Local).is_some()
}

/// [`all_day_span`] for an all-day span from `start` to `end`.
fn all_day_columns(start: EpochMillis, end: EpochMillis, days: &[NaiveDate]) -> Option<(usize, usize)> {
    date_columns(utc_date(start)?, utc_date(end)?.checked_sub_days(Days::new(1))?, days)
}

/// The columns from `start_date` through `end_date` among `days`, clipped
/// to them.
fn date_columns(start_date: NaiveDate, end_date: NaiveDate, days: &[NaiveDate]) -> Option<(usize, usize)> {
    let (&first, &last) = (days.first()?, days.last()?);
    if end_date < first || start_date > last {
        return None;
    }
    let clipped_start = start_date.max(first);
    let clipped_end = end_date.min(last);
    let start_day = (clipped_start - first).num_days() as usize;
    let end_day = (clipped_end - first).num_days() as usize + 1;
    Some((start_day, end_day))
}

/// Today's column among `days` and how many hours past its midnight
/// `now` is, when today is one of them.
pub fn now_column<Z: TimeZone>(now: EpochMillis, days: &[NaiveDate], zone: &Z) -> Option<(usize, f64)> {
    // Today is the local date of now. The UTC date is a different day
    // for part of every day anywhere east or west of UTC.
    let today = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now)?
        .with_timezone(zone)
        .date_naive();
    let column = days.iter().position(|&d| d == today)?;
    let midnight = today.and_hms_opt(0, 0, 0)?;
    Some((column, crate::calendar_layout::wall_offset(now, midnight, zone)))
}

fn utc_date(at: EpochMillis) -> Option<NaiveDate> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(at).map(|d| d.date_naive())
}


/// `spans`, each stretched to run at least `min` from its own start:
/// not what the cards draw (their real length still decides that; see
/// the card's own floor), but what lane assignment reasons about, so two
/// short events close enough together that their minimum-height cards
/// would touch take separate lanes rather than sharing one and drawing
/// on top of each other.
pub fn stretch_for_lanes(
    spans: &[(EpochMillis, EpochMillis)],
    min: EpochMillis,
) -> Vec<(EpochMillis, EpochMillis)> {
    spans.iter().map(|&(start, end)| (start, end.max(start + min))).collect()
}


#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeZone, Utc};
    use mailrs_domain::calendar::Event;

    use super::*;

    fn at(d: u32, h: u32) -> EpochMillis {
        Utc.with_ymd_and_hms(2026, 10, d, h, 0, 0).unwrap().timestamp_millis()
    }

    fn occurrence(all_day: bool, start: EpochMillis, end: EpochMillis) -> Occurrence {
        Occurrence {
            account_id: 1,
            event: Arc::new(Event { all_day, start, end, ..Event::default() }),
            start,
            end,
        }
    }

    fn first() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
    }

    #[test]
    fn an_all_day_event_covers_its_own_utc_dates() {
        // 2 to 4 October, the grid starting on 28 September.
        assert_eq!(grid_days(&occurrence(true, at(2, 0), at(5, 0)), first(), &Utc), (4, 7));
    }

    #[test]
    fn a_timed_event_past_midnight_covers_both_days() {
        assert_eq!(grid_days(&occurrence(false, at(2, 22), at(3, 2)), first(), &Utc), (4, 6));
    }

    #[test]
    fn a_timed_event_ending_at_midnight_stays_on_its_day() {
        assert_eq!(grid_days(&occurrence(false, at(2, 22), at(3, 0)), first(), &Utc), (4, 5));
    }

    #[test]
    fn a_timed_event_of_no_length_still_takes_its_day() {
        assert_eq!(grid_days(&occurrence(false, at(2, 9), at(2, 9)), first(), &Utc), (4, 5));
    }

    #[test]
    fn an_event_from_before_the_grid_counts_back_from_it() {
        assert_eq!(grid_days(&occurrence(true, at(1, 0), at(3, 0)) , NaiveDate::from_ymd_opt(2026, 10, 2).unwrap(), &Utc), (-1, 1));
    }
}
