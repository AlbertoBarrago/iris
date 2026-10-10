//! The calendar for the Swift front end: each account's calendars for the
//! sidebar, and the month grid laid out by the same functions the GTK
//! month view uses (`render::calendar_days`, `calendar_layout`). The copy
//! of every calendar is kept fresh on a timer, as the GTK app keeps it.

use std::sync::Arc;

use chrono::{Datelike, Days, Local, NaiveDate, TimeZone, Weekday};
use mailrs_appcore::settings::Settings;
use mailrs_domain::EpochMillis;
use mailrs_domain::calendar::Occurrence;
use mailrs_domain::calendar::week::{week_start, week_start_on_or_before};
use mailrs_domain::translate::{date_locale, gettext};
use mailrs_render::calendar_days::grid_days;
use mailrs_render::calendar_layout as layout;
use mailrs_render::calendar_words::{bar_words, guest_answer_words, guest_name_words, more_count_words, when_words};
use mailrs_store::accounts;
use mailrs_store::calendar::CalendarScope;
use mailrs_sync::SyncEngine;
use mailrs_sync::calendar_copy::CalendarCopy;

use crate::CoreError;
use crate::mail::{Mail, MailListener, Running};

/// The rows of a month grid: six weeks, so every row is a whole week.
const WEEKS: usize = 6;

/// One account's calendars, for the sidebar.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CalendarAccount {
    pub account_id: i64,
    pub email: String,
    pub calendars: Vec<CalendarRow>,
}

/// One calendar: its colour and whether its events show.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CalendarRow {
    pub account_id: i64,
    pub id: String,
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    pub shown: bool,
}

/// A month as the grid draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MonthView {
    /// "October 2026".
    pub title: String,
    /// The column headings, "Mon" to "Sun" or from the week's first day.
    pub weekdays: Vec<String>,
    pub weeks: Vec<MonthWeek>,
}

/// One row of the month: its seven days, the events that fit in `rows`
/// lines, and how many each day hides behind "N more".
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MonthWeek {
    pub days: Vec<MonthDay>,
    pub items: Vec<MonthItem>,
    /// Per day: "3 more", or empty when nothing is hidden.
    pub more: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MonthDay {
    /// `YYYY-MM-DD`, for the day's own list.
    pub date: String,
    pub number: u32,
    /// The day belongs to the month shown, not one either side.
    pub in_month: bool,
    pub today: bool,
    /// A working day; the others are shaded.
    pub working: bool,
}

/// An event in a week row: a bar across several days, or a chip in one.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MonthItem {
    pub id: String,
    pub title: String,
    /// `#rrggbb`: the event's own colour, else its calendar's.
    pub color: String,
    /// Columns from the week's first day, `end` exclusive.
    pub start: u32,
    pub end: u32,
    pub line: u32,
    /// The event carries on before or after this row.
    pub squared_start: bool,
    pub squared_end: bool,
    /// Spans more than one day, drawn as a filled bar.
    pub bar: bool,
    pub all_day: bool,
    /// When it starts, in milliseconds, for the time a chip shows.
    pub starts: i64,
    /// What a screen reader hears for it.
    pub described: String,
    pub details: EventDetails,
}

/// What the event's card shows.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventDetails {
    pub title: String,
    pub color: String,
    /// "Monday 12 October, 09:00 – 10:00", or the days of an all-day one.
    pub when: String,
    pub place: String,
    pub calendar: String,
    /// The organizer, when someone else organizes it.
    pub organizer: String,
    pub guests: Vec<GuestRow>,
    /// A video call link.
    pub conference: String,
    /// The description as lines of text.
    pub notes: String,
}

/// One person asked to the event, and their answer.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GuestRow {
    /// "You", the name, or the address; "{name}, organizer" for them.
    pub name: String,
    pub email: String,
    /// `yes`, `no`, `maybe`, or empty before they answer.
    pub answer: String,
    /// "Going", "Not going", "Maybe" or "No answer yet".
    pub said: String,
    pub organizer: bool,
}

/// An event on one day, for the list "N more" opens.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DayItem {
    pub id: String,
    pub title: String,
    pub color: String,
    pub all_day: bool,
    pub starts: i64,
    pub ends: i64,
    pub details: EventDetails,
}

#[uniffi::export]
impl Mail {
    /// Each account's calendars, in the sidebar's order.
    pub fn calendar_accounts(&self) -> Result<Vec<CalendarAccount>, CoreError> {
        let db = self.db.clone();
        let all = self.run(async move { db.read(accounts::list_accounts).await })?;
        let ids: Vec<i64> = all.iter().map(|a| a.id).collect();
        let copy = Arc::clone(&self.calendar);
        let listed = self
            .run(async move { copy.listed(&ids).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        Ok(listed
            .into_iter()
            .filter(|l| !l.calendars.is_empty())
            .map(|l| CalendarAccount {
                account_id: l.account_id,
                email: all.iter().find(|a| a.id == l.account_id).map(|a| a.email.clone()).unwrap_or_default(),
                calendars: l
                    .calendars
                    .into_iter()
                    .filter(|c| !l.unlisted.contains(&c.id))
                    .map(|c| CalendarRow {
                        account_id: l.account_id,
                        id: c.id,
                        name: c.name,
                        color: c.color,
                        shown: c.shown,
                    })
                    .collect(),
            })
            .collect())
    }

    /// Shows or hides a calendar's events on this computer.
    pub fn show_calendar(&self, account_id: i64, calendar: String, shown: bool) -> Result<(), CoreError> {
        let copy = Arc::clone(&self.calendar);
        self.run(async move { copy.show_calendar(account_id, &calendar, shown).await })
            .map_err(|err| CoreError::Store(err.to_string()))
    }

    /// The month holding `year`-`month`, laid out in six week rows that
    /// each hold `rows` lines of events. `locale_first` is the locale's
    /// own first weekday, 0 for Monday to 6 for Sunday, which Preferences'
    /// Week Starts On may override.
    pub fn month_view(&self, year: i32, month: u32, locale_first: u32, rows: u32) -> Result<MonthView, CoreError> {
        let settings = Settings::load(&Settings::default_path());
        let start = week_start(settings.week_start, weekday(locale_first));
        let first_of_month = NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| CoreError::Store("There is no such month.".into()))?;
        let first = week_start_on_or_before(first_of_month, start);
        let days: Vec<NaiveDate> = (0..(WEEKS * 7) as u64).map(|i| first + Days::new(i)).collect();
        let today = Local::now().date_naive();
        let occurrences = self.occurrences_between(first, first + Days::new((WEEKS * 7) as u64))?;
        let colors = self.calendar_colors()?;

        let mut sorted: Vec<&Occurrence> = occurrences.iter().collect();
        sorted.sort_by_key(|o| (!o.event.all_day, o.start));
        let mut weeks: Vec<Vec<(&Occurrence, bool, layout::Segment)>> = vec![Vec::new(); WEEKS];
        for o in sorted {
            let (from, to) = grid_days(o, first, &Local);
            let bar = to - from > 1;
            for segment in layout::week_segments(from, to, WEEKS) {
                weeks[segment.week].push((o, bar, segment));
            }
        }

        let rows = (rows as usize).max(1);
        let laid = weeks
            .iter()
            .enumerate()
            .map(|(week, entries)| {
                let spans: Vec<(EpochMillis, EpochMillis)> =
                    entries.iter().map(|(_, _, s)| (s.start as EpochMillis, s.end as EpochMillis)).collect();
                let (placed, _) = layout::lanes_within(&spans, usize::MAX);
                let lines: Vec<usize> = placed.iter().map(|p| p.lane).collect();
                let columns: Vec<(usize, usize)> = entries.iter().map(|(_, _, s)| (s.start, s.end)).collect();
                let (shown, more) = layout::fold_week(&columns, &lines, rows);
                let items = entries
                    .iter()
                    .zip(&lines)
                    .zip(shown)
                    .filter(|(_, shown)| *shown)
                    .map(|(((o, bar, segment), line), _)| {
                        let (color, calendar) = colors
                            .iter()
                            .find(|(account, id, _, _)| *account == o.account_id && *id == o.event.calendar)
                            .map(|(_, _, color, name)| (color.clone(), name.clone()))
                            .unwrap_or_default();
                        MonthItem {
                            id: o.id(),
                            title: shown_title(o),
                            color: o.event.color.clone().unwrap_or(color.clone()),
                            start: segment.start as u32,
                            end: segment.end as u32,
                            line: *line as u32,
                            squared_start: segment.squared_start,
                            squared_end: segment.squared_end,
                            bar: *bar,
                            all_day: o.event.all_day,
                            starts: o.start,
                            described: bar_words(o, &calendar, &Local),
                            details: details(o, o.event.color.clone().unwrap_or(color.clone()), &calendar),
                        }
                    })
                    .collect();
                MonthWeek {
                    days: days[week * 7..week * 7 + 7]
                        .iter()
                        .map(|&day| MonthDay {
                            date: day.format("%Y-%m-%d").to_string(),
                            number: day.day(),
                            in_month: day.month() == month,
                            today: day == today,
                            working: settings.working_hours.is_working_day(day.weekday()),
                        })
                        .collect(),
                    items,
                    more: more.iter().map(|&n| if n == 0 { String::new() } else { more_count_words(n) }).collect(),
                }
            })
            .collect();
        Ok(MonthView {
            title: first_of_month.format_localized(&gettext("%B %Y"), date_locale()).to_string(),
            weekdays: days[..7]
                .iter()
                .map(|d| d.format_localized(&gettext("%a"), date_locale()).to_string())
                .collect(),
            weeks: laid,
        })
    }

    /// Every event on `date` (`YYYY-MM-DD`), all-day ones first, for the
    /// list a crowded day opens.
    pub fn day_events(&self, date: String) -> Result<Vec<DayItem>, CoreError> {
        let day = NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|err| CoreError::Store(err.to_string()))?;
        let mut found = self.occurrences_between(day, day + Days::new(1))?;
        // The read overlaps the day by instant; an all-day event of the day
        // before reaches past local midnight, so keep only what the grid
        // puts on this day.
        found.retain(|o| {
            let (from, to) = grid_days(o, day, &Local);
            from <= 0 && to > 0
        });
        found.sort_by_key(|o| (!o.event.all_day, o.start));
        let colors = self.calendar_colors()?;
        Ok(found
            .iter()
            .map(|o| {
                let (color, calendar) = colors
                    .iter()
                    .find(|(account, id, _, _)| *account == o.account_id && *id == o.event.calendar)
                    .map(|(_, _, color, name)| (color.clone(), name.clone()))
                    .unwrap_or_default();
                let color = o.event.color.clone().unwrap_or(color);
                DayItem {
                    id: o.id(),
                    title: shown_title(o),
                    details: details(o, color.clone(), &calendar),
                    color,
                    all_day: o.event.all_day,
                    starts: o.start,
                    ends: o.end,
                }
            })
            .collect())
    }
}

impl Mail {
    /// The shown calendars' occurrences from local midnight of `from` to
    /// local midnight of `to`.
    fn occurrences_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Occurrence>, CoreError> {
        let midnight = |day: NaiveDate| {
            Local
                .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap_or_default())
                .earliest()
                .map_or(0, |at| at.timestamp_millis())
        };
        let (start, end) = (midnight(from), midnight(to));
        let db = self.db.clone();
        let ids: Vec<i64> = self
            .run(async move { db.read(accounts::list_accounts).await })?
            .into_iter()
            .map(|a| a.id)
            .collect();
        let copy = Arc::clone(&self.calendar);
        self.run(async move { copy.occurrences(&ids, start, end, CalendarScope::Shown).await })
            .map_err(|err| CoreError::Store(err.to_string()))
    }

    /// Every calendar's account, id, colour and name.
    fn calendar_colors(&self) -> Result<Vec<(i64, String, String, String)>, CoreError> {
        Ok(self
            .calendar_accounts()?
            .into_iter()
            .flat_map(|a| a.calendars)
            .map(|c| (c.account_id, c.id, c.color, c.name))
            .collect())
    }
}

/// The card for `o`, in `color`, on the calendar called `calendar`.
fn details(o: &Occurrence, color: String, calendar: &str) -> EventDetails {
    EventDetails {
        title: shown_title(o),
        color,
        when: when_words(o, &Local),
        place: o.event.place.clone(),
        calendar: calendar.to_string(),
        organizer: o.event.organizer.clone().unwrap_or_default(),
        guests: o
            .event
            .guests
            .iter()
            .map(|guest| GuestRow {
                name: guest_name_words(guest),
                email: guest.email.clone(),
                answer: guest.answer.map(|a| a.as_str().to_string()).unwrap_or_default(),
                said: guest_answer_words(guest),
                organizer: guest.organizer,
            })
            .collect(),
        conference: o.event.conference.clone().unwrap_or_default(),
        notes: mailrs_mime::notes::text(&o.event.description).trim().to_string(),
    }
}

/// An event's title, or the words for one that has none.
fn shown_title(o: &Occurrence) -> String {
    if o.event.title.trim().is_empty() {
        gettext("Untitled event")
    } else {
        o.event.title.clone()
    }
}

/// 0 for Monday to 6 for Sunday, as Swift hands the locale's first day.
fn weekday(index: u32) -> Weekday {
    Weekday::try_from((index % 7) as u8).unwrap_or(Weekday::Mon)
}

/// How often the calendar copy is ticked; `CalendarCopy` decides which
/// accounts are actually due, as the GTK app's 15-second timer does.
const REFRESH_EVERY: std::time::Duration = std::time::Duration::from_secs(15);

/// Keeps the calendar copy fresh while `engine` runs, and tells the
/// window when events or the list of calendars changed.
pub(crate) async fn keep_fresh(
    copy: Arc<CalendarCopy<Running>>,
    running: Arc<Running>,
    engine: Arc<SyncEngine>,
    db: mailrs_store::Db,
    listener: Arc<dyn MailListener>,
) {
    let mut every = tokio::time::interval(REFRESH_EVERY);
    loop {
        every.tick().await;
        if !running.current().is_some_and(|now| Arc::ptr_eq(&now, &engine)) {
            return;
        }
        let Ok(ids) = db.read(accounts::list_accounts).await else { continue };
        let ids: Vec<i64> = ids.into_iter().map(|a| a.id).collect();
        match copy.refresh_due(&ids, mailrs_sync::now_millis(), true).await {
            Ok(refreshed) if refreshed.events > 0 || refreshed.calendars_changed => listener.changed("calendar".into()),
            Ok(_) => {}
            Err(err) => tracing::warn!(%err, "could not read the calendars"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swift_counts_weekdays_from_monday() {
        assert_eq!(weekday(0), Weekday::Mon);
        assert_eq!(weekday(6), Weekday::Sun);
    }

    #[test]
    fn a_month_has_six_whole_weeks() {
        let dir = tempfile::tempdir().unwrap();
        let mail = Mail::open(dir.path().join("mailrs.db").display().to_string()).unwrap();
        let view = mail.month_view(2026, 10, 0, 3).unwrap();
        assert_eq!(view.weeks.len(), 6);
        assert!(view.weeks.iter().all(|w| w.days.len() == 7 && w.more.len() == 7));
        // October 2026 starts on a Thursday; a Monday grid opens on 28 September.
        assert_eq!(view.weeks[0].days[0].date, "2026-09-28");
        assert!(!view.weeks[0].days[0].in_month);
        assert_eq!(view.weekdays.len(), 7);
    }
}
