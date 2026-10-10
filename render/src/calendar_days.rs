//! The days a calendar occurrence covers in a grid of whole days, as the
//! month view lays them out. Both front ends draw the month from it.

use chrono::{Days, NaiveDate};
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
