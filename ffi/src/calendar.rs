//! The calendar for the Swift front end: each account's calendars for the
//! sidebar, and the month grid laid out by the same functions the GTK
//! month view uses (`render::calendar_days`, `calendar_layout`). The copy
//! of every calendar is kept fresh on a timer, as the GTK app keeps it.

use std::sync::Arc;

use chrono::{Datelike, Days, Local, NaiveDate, TimeZone, Weekday};
use mailrs_appcore::calendar_drag;
use mailrs_appcore::settings::Settings;
use mailrs_domain::EpochMillis;
use mailrs_domain::calendar::{Access, Occurrence};
use mailrs_domain::calendar::week::{week_start, week_start_on_or_before};
use mailrs_domain::translate::{date_locale, gettext};
use mailrs_render::calendar_days::{self, grid_days};
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

/// The shortest a timed card is laid out as, so two short events close
/// together take separate lanes rather than drawing over each other: 25
/// minutes, a one-line card at the grid's hour height.
const SHORTEST_CARD: EpochMillis = 25 * 60 * 1000;

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
    /// The occurrence's id and start, which open its editor.
    pub id: String,
    pub starts: i64,
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
    /// Whether a drag may move it: see `calendar_drag::can_move`.
    pub movable: bool,
    /// Whether the person is a guest of someone else's event, who answers
    /// it rather than changes it.
    pub invited: bool,
    /// The person's answer to it: `yes`, `no`, `maybe`, or empty before
    /// one.
    pub my_answer: String,
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

/// A week or a day, as the time grid draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WeekView {
    /// "October 2026", or "Sep – Oct" for a week across two months.
    pub title: String,
    pub days: Vec<WeekDayHead>,
    /// All-day events and those that run from midnight to midnight, on
    /// the strip above the hours.
    pub strip: Vec<StripItem>,
    pub strip_rows: u32,
    pub timed: Vec<TimedItem>,
    /// "+N" cards where more than four events overlap.
    pub more: Vec<MoreCard>,
    /// Today's column and the hours past its midnight, for the now line.
    pub now_column: Option<u32>,
    pub now_hours: f64,
    /// The hour the grid opens scrolled to: 08:00, or the day's first
    /// event when that is earlier.
    pub first_hour: f64,
    /// The working day, in hours, which the grid leaves unshaded.
    pub work_start: f64,
    pub work_end: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WeekDayHead {
    pub date: String,
    /// "Mon", in the interface's language.
    pub weekday: String,
    pub number: u32,
    pub today: bool,
    pub working: bool,
}

/// An event on the all-day strip, across columns `start` to `end`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StripItem {
    pub id: String,
    pub title: String,
    pub color: String,
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub details: EventDetails,
}

/// A timed event's card: its column, its lane among `lanes` sharing the
/// width, and its top and bottom in hours from the day's midnight.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TimedItem {
    pub id: String,
    pub title: String,
    pub color: String,
    pub column: u32,
    pub lane: u32,
    pub lanes: u32,
    pub top: f64,
    pub bottom: f64,
    pub starts: i64,
    pub ends: i64,
    pub details: EventDetails,
}

/// The events of a crowded hour the grid has no lane left for.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MoreCard {
    pub column: u32,
    pub top: f64,
    pub bottom: f64,
    /// "+3".
    pub label: String,
    pub items: Vec<DayItem>,
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
                        let (color, details) = dress(&colors, o);
                        MonthItem {
                            id: o.id(),
                            title: shown_title(o),
                            color,
                            start: segment.start as u32,
                            end: segment.end as u32,
                            line: *line as u32,
                            squared_start: segment.squared_start,
                            squared_end: segment.squared_end,
                            bar: *bar,
                            all_day: o.event.all_day,
                            starts: o.start,
                            described: bar_words(o, &details.calendar, &Local),
                            details,
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

    /// The `days` (1 or 7) holding `date` (`YYYY-MM-DD`): a day, or the
    /// week from Preferences' first day, `locale_first` being the locale's
    /// own (0 for Monday to 6 for Sunday).
    pub fn week_view(&self, date: String, days: u32, locale_first: u32) -> Result<WeekView, CoreError> {
        let settings = Settings::load(&Settings::default_path());
        let anchor = NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|err| CoreError::Store(err.to_string()))?;
        let count = if days <= 1 { 1 } else { 7 };
        let first = if count == 1 {
            anchor
        } else {
            week_start_on_or_before(anchor, week_start(settings.week_start, weekday(locale_first)))
        };
        let dates: Vec<NaiveDate> = (0..count).map(|i| first + Days::new(i)).collect();
        let last = dates[dates.len() - 1];
        let today = Local::now().date_naive();
        let occurrences = self.occurrences_between(first, last + Days::new(1))?;
        let colors = self.calendar_colors()?;
        let look = |o: &Occurrence| dress(&colors, o);

        // The strip: all-day events, and timed ones from midnight to
        // midnight, stacked as the month stacks a week.
        let in_strip: Vec<(&Occurrence, (usize, usize))> = occurrences
            .iter()
            .filter(|o| calendar_days::in_strip(o))
            .filter_map(|o| calendar_days::strip_columns(o.start, o.end, o.event.all_day, &dates).map(|c| (o, c)))
            .collect();
        let spans: Vec<(EpochMillis, EpochMillis)> =
            in_strip.iter().map(|(_, (a, b))| (*a as EpochMillis, *b as EpochMillis)).collect();
        let (placed, _) = layout::lanes_within(&spans, usize::MAX);
        let strip: Vec<StripItem> = placed
            .iter()
            .map(|p| {
                let (o, (start, end)) = in_strip[p.index];
                let (color, details) = look(o);
                StripItem {
                    id: o.id(),
                    title: shown_title(o),
                    color,
                    start: start as u32,
                    end: end as u32,
                    line: p.lane as u32,
                    details,
                }
            })
            .collect();
        let strip_rows = placed.iter().map(|p| p.lane + 1).max().unwrap_or(0) as u32;

        // The hours: each day's timed events, clipped to it, in lanes.
        let midnight = |day: NaiveDate| {
            Local
                .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap_or_default())
                .earliest()
                .map_or(0, |at| at.timestamp_millis())
        };
        let bounds: Vec<(EpochMillis, EpochMillis)> =
            dates.iter().map(|&d| (midnight(d), midnight(d + Days::new(1)))).collect();
        let mut by_day: Vec<Vec<(usize, EpochMillis, EpochMillis)>> = vec![Vec::new(); dates.len()];
        for (index, o) in occurrences.iter().enumerate() {
            if calendar_days::in_strip(o) {
                continue;
            }
            for (column, start, end) in layout::clip_to_days(o.start, o.end, &bounds) {
                by_day[column].push((index, start, end));
            }
        }
        let mut timed = Vec::new();
        let mut more = Vec::new();
        let mut starts = Vec::new();
        for (column, (day, pieces)) in dates.iter().zip(&by_day).enumerate() {
            let wall = day.and_hms_opt(0, 0, 0).unwrap_or_default();
            let spans: Vec<(EpochMillis, EpochMillis)> = pieces.iter().map(|&(_, s, e)| (s, e)).collect();
            let (placed, hidden) = layout::lanes(&calendar_days::stretch_for_lanes(&spans, SHORTEST_CARD));
            for p in placed {
                let (index, start, end) = pieces[p.index];
                let o = &occurrences[index];
                let (color, details) = look(o);
                let top = layout::wall_offset(start, wall, &Local);
                starts.push(top);
                timed.push(TimedItem {
                    id: o.id(),
                    title: shown_title(o),
                    color,
                    column: column as u32,
                    lane: p.lane as u32,
                    lanes: p.lanes as u32,
                    top,
                    bottom: layout::wall_offset(end, wall, &Local),
                    starts: o.start,
                    ends: o.end,
                    details,
                });
            }
            for group in hidden {
                more.push(MoreCard {
                    column: column as u32,
                    top: layout::wall_offset(group.from, wall, &Local),
                    bottom: layout::wall_offset(group.to, wall, &Local),
                    label: format!("+{}", group.hidden.len()),
                    items: group
                        .hidden
                        .iter()
                        .map(|&i| {
                            let o = &occurrences[pieces[i].0];
                            let (color, details) = look(o);
                            DayItem {
                                id: o.id(),
                                title: shown_title(o),
                                color,
                                all_day: o.event.all_day,
                                starts: o.start,
                                ends: o.end,
                                details,
                            }
                        })
                        .collect(),
                });
            }
        }
        let now = calendar_days::now_column(mailrs_sync::now_millis(), &dates, &Local);
        let title = if first.month() == last.month() {
            first.format_localized(&gettext("%B %Y"), date_locale()).to_string()
        } else {
            mailrs_domain::translate::fill(
                &gettext("{first} – {last}"),
                &[
                    ("first", &first.format_localized(&gettext("%b"), date_locale()).to_string()),
                    ("last", &last.format_localized(&gettext("%b"), date_locale()).to_string()),
                ],
            )
        };
        Ok(WeekView {
            title,
            days: dates
                .iter()
                .map(|&d| WeekDayHead {
                    date: d.format("%Y-%m-%d").to_string(),
                    weekday: d.format_localized(&gettext("%a"), date_locale()).to_string(),
                    number: d.day(),
                    today: d == today,
                    working: settings.working_hours.is_working_day(d.weekday()),
                })
                .collect(),
            strip,
            strip_rows,
            timed,
            more,
            now_column: now.map(|(column, _)| column as u32),
            now_hours: now.map_or(0.0, |(_, hours)| hours),
            first_hour: layout::first_hour(&starts),
            work_start: f64::from(settings.working_hours.start_minutes) / 60.0,
            work_end: f64::from(settings.working_hours.end_minutes) / 60.0,
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
                let (color, details) = dress(&colors, o);
                DayItem {
                    id: o.id(),
                    title: shown_title(o),
                    details,
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

    /// Every calendar's account, id, colour, name and access.
    fn calendar_colors(&self) -> Result<Vec<Shade>, CoreError> {
        let ids = self.account_ids()?;
        let copy = Arc::clone(&self.calendar);
        let listed = self
            .run(async move { copy.listed(&ids).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        Ok(listed
            .into_iter()
            .flat_map(|l| {
                let account = l.account_id;
                l.calendars.into_iter().map(move |c| Shade {
                    account,
                    id: c.id,
                    color: c.color,
                    name: c.name,
                    access: c.access,
                })
            })
            .collect())
    }

    fn account_ids(&self) -> Result<Vec<i64>, CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move { db.read(accounts::list_accounts).await })?.into_iter().map(|a| a.id).collect())
    }
}

/// A calendar as the cards on it are drawn.
struct Shade {
    account: i64,
    id: String,
    color: String,
    name: String,
    access: Access,
}

/// `o`'s colour, its own or its calendar's, and its card.
fn dress(shades: &[Shade], o: &Occurrence) -> (String, EventDetails) {
    let shade = shades.iter().find(|s| s.account == o.account_id && s.id == o.event.calendar);
    let color = o.event.color.clone().or_else(|| shade.map(|s| s.color.clone())).unwrap_or_default();
    let calendar = shade.map(|s| s.name.as_str()).unwrap_or_default();
    // Whether the account offers and grants its calendar shows when the
    // move is written, which then says what is missing.
    let movable = shade.is_some_and(|s| calendar_drag::can_move(o, s.access, true, false));
    (color.clone(), details(o, color, calendar, movable))
}

/// The card for `o`, in `color`, on the calendar called `calendar`.
fn details(o: &Occurrence, color: String, calendar: &str, movable: bool) -> EventDetails {
    EventDetails {
        movable,
        invited: mailrs_appcore::calendar_draft::limited(&o.event),
        my_answer: o.event.my_answer.map(|a| a.as_str().to_string()).unwrap_or_default(),
        id: o.id(),
        starts: o.start,
        title: shown_title(o),
        color,
        when: when_words(o, &Local),
        place: o.event.place.clone(),
        calendar: calendar.to_string(),
        organizer: o.event.organizer.clone().unwrap_or_default(),
        guests: o.event.guests.iter().map(guest_row).collect(),
        conference: o.event.conference.clone().unwrap_or_default(),
        notes: mailrs_mime::notes::text(&o.event.description).trim().to_string(),
    }
}

/// One guest as a list shows them: their name, address and answer.
pub(crate) fn guest_row(guest: &mailrs_domain::calendar::Guest) -> GuestRow {
    GuestRow {
        name: guest_name_words(guest),
        email: guest.email.clone(),
        answer: guest.answer.map(|a| a.as_str().to_string()).unwrap_or_default(),
        said: guest_answer_words(guest),
        organizer: guest.organizer,
    }
}

/// An event's title, or the words for one that has none.
pub(crate) fn shown_title(o: &Occurrence) -> String {
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

#[cfg(test)]
mod week_tests {
    use super::*;

    #[test]
    fn a_week_has_seven_heads_from_its_first_day() {
        let dir = tempfile::tempdir().unwrap();
        let mail = Mail::open(dir.path().join("mailrs.db").display().to_string()).unwrap();
        let week = mail.week_view("2026-10-15".into(), 7, 0).unwrap();
        assert_eq!(week.days.len(), 7);
        assert_eq!(week.days[0].date, "2026-10-12", "a Monday week holds the 15th from the 12th");
        assert_eq!(week.first_hour, 8.0, "an empty week opens at eight");
        let day = mail.week_view("2026-10-15".into(), 1, 0).unwrap();
        assert_eq!(day.days.len(), 1);
    }
}
