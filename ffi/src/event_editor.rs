//! The event editor for the Swift front end: a draft of one event, new or
//! opened, kept by appcore's `calendar_draft` rules, saved or deleted
//! through the calendar copy after the same question the GTK calendar asks
//! (`event_question`) when a repeating event or its guests need one.

use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDate, TimeZone};
use chrono_tz::Tz;
use mailrs_appcore::calendar_draft::Draft;
use mailrs_appcore::event_question::{self, Look, Question};
use mailrs_domain::calendar::series::RepeatScope;
use mailrs_domain::calendar::{Calendar, Occurrence};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{AccountId, EpochMillis};
use mailrs_store::accounts;
use mailrs_store::calendar::CalendarScope;
use mailrs_sync::calendar_copy::event_change::{Ask, Edit, EventChange, Undo};
use mailrs_sync::calendar_copy::new_event_id;

use crate::CoreError;
use crate::mail::Mail;

/// What the editor shows and edits.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventForm {
    pub title: String,
    pub all_day: bool,
    /// Milliseconds. An all-day event runs from UTC midnight of its first
    /// day to UTC midnight after its last.
    pub start: i64,
    pub end: i64,
    /// `account_id/calendar id` of the calendar it goes on.
    pub calendar: String,
    pub place: String,
    pub notes: String,
}

/// A calendar a new event may go on, or an opened one move to.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CalendarChoice {
    pub key: String,
    pub name: String,
    pub color: String,
    pub account: String,
}

/// The question before a save or a delete, as the GTK dialog asks it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventQuestion {
    pub heading: String,
    pub body: Vec<String>,
    pub responses: Vec<QuestionButton>,
    /// Which occurrences of a repeating event, when the buttons already
    /// carry another choice.
    pub scopes: Vec<QuestionButton>,
    /// "Send an update to the guests" as a tick, when the buttons already
    /// choose the time.
    pub send_check: bool,
    pub default_id: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct QuestionButton {
    pub id: String,
    pub label: String,
    /// `plain`, `suggested` or `destructive`.
    pub look: String,
}

/// One event being edited.
#[derive(uniffi::Object)]
pub struct EventEditor {
    mail: Arc<Mail>,
    draft: Mutex<Draft>,
    calendars: Vec<(AccountId, String, Calendar)>,
}

#[uniffi::export]
impl Mail {
    /// An editor on the occurrence `id` starting at `starts`, as the grid
    /// named it. `None` when the occurrence is gone or someone else's
    /// event the account cannot change.
    pub fn edit_event(self: Arc<Self>, id: String, starts: i64) -> Result<Option<Arc<EventEditor>>, CoreError> {
        let Some(occurrence) = self.occurrence(&id, starts)? else {
            return Ok(None);
        };
        let calendars = self.writable_calendars()?;
        if !calendars.iter().any(|(a, _, c)| *a == occurrence.account_id && c.id == occurrence.event.calendar) {
            return Ok(None);
        }
        let rules = self.series_rules(&occurrence);
        let draft = Draft::open(&occurrence, &rules, zone());
        Ok(Some(Arc::new(EventEditor { mail: Arc::clone(&self), draft: Mutex::new(draft), calendars })))
    }

    /// An editor on a new event from `start` to `end`, on the calendar a
    /// new event goes on when nobody says: the first primary one the
    /// person can write to. `None` when no calendar takes new events.
    pub fn new_event(self: Arc<Self>, start: i64, end: i64, all_day: bool) -> Result<Option<Arc<EventEditor>>, CoreError> {
        let calendars = self.writable_calendars()?;
        let Some((account, calendar)) = mailrs_appcore::calendar_draft::default_calendar(&calendars, None) else {
            return Ok(None);
        };
        let mut draft = Draft::new(account, &calendar, start, end, zone());
        if all_day {
            let day = Local.timestamp_millis_opt(start).single().map(|t| t.date_naive()).unwrap_or_default();
            draft.land(utc_midnight(day), utc_midnight(day.succ_opt().unwrap_or(day)), true);
        }
        Ok(Some(Arc::new(EventEditor { mail: Arc::clone(&self), draft: Mutex::new(draft), calendars })))
    }
}

#[uniffi::export]
impl EventEditor {
    pub fn form(&self) -> EventForm {
        let draft = self.lock();
        EventForm {
            title: draft.title.clone(),
            all_day: draft.all_day,
            start: draft.start,
            end: draft.end,
            calendar: key(draft.account_id, &draft.calendar),
            place: draft.place.clone(),
            notes: draft.notes.clone(),
        }
    }

    /// The calendars the event may go on: every writable one for a new
    /// event, its own account's for one opened.
    pub fn calendars(&self) -> Vec<CalendarChoice> {
        let draft = self.lock();
        let emails = self.mail.emails();
        self.calendars
            .iter()
            .filter(|(account, _, _)| draft.is_new() || *account == draft.account_id)
            .map(|(account, _, calendar)| CalendarChoice {
                key: key(*account, &calendar.id),
                name: calendar.name.clone(),
                color: calendar.color.clone(),
                account: emails.iter().find(|(id, _)| id == account).map(|(_, e)| e.clone()).unwrap_or_default(),
            })
            .collect()
    }

    pub fn is_new(&self) -> bool {
        self.lock().is_new()
    }

    /// Lays what the editor holds over the draft, by the draft's own
    /// rules: all day first, then the times.
    pub fn update(&self, form: EventForm) {
        let mut draft = self.lock();
        draft.title = form.title;
        draft.set_all_day(form.all_day);
        draft.set_span(form.start, form.end);
        draft.place = form.place;
        draft.notes = form.notes;
        if let Some((account, calendar)) = form.calendar.split_once('/')
            && let Ok(account) = account.parse::<AccountId>()
            && (draft.is_new() || account == draft.account_id)
        {
            draft.account_id = account;
            draft.calendar = calendar.to_string();
        }
    }

    pub fn can_save(&self) -> bool {
        self.lock().can_save()
    }

    /// Who is asked, with their answers.
    pub fn guests(&self) -> Vec<crate::calendar::GuestRow> {
        self.lock().guests.iter().map(crate::calendar::guest_row).collect()
    }

    /// Whether the guest list can change: not on someone else's event the
    /// account is only a guest of, whose organizer keeps the list.
    pub fn guests_editable(&self) -> bool {
        self.lock().base.as_ref().is_none_or(|base| !mailrs_appcore::calendar_draft::limited(base))
    }

    /// Adds the addresses in `text`, as the Guests field holds it, and
    /// answers what the field keeps: the parts that are not addresses.
    pub fn add_guests(&self, text: String) -> String {
        self.lock().take_guests(&text)
    }

    /// Takes `email` off the guest list. The account itself stays.
    pub fn remove_guest(&self, email: String) {
        self.lock().guests.retain(|g| g.me || !g.email.eq_ignore_ascii_case(&email));
    }

    /// What to ask before saving, or `None` to save at once.
    pub fn save_question(&self) -> Result<Option<EventQuestion>, CoreError> {
        let draft = self.lock().clone();
        let title = draft.title.clone();
        self.question(draft.account_id, &change(&draft), &title, draft.moved().then(|| when(&draft)))
    }

    /// Saves the draft with the answer to [`save_question`], or with none
    /// when it asked nothing. Says what it did.
    pub fn save(&self, response: Option<String>, scope: Option<String>, send: bool) -> Result<String, CoreError> {
        let draft = self.lock().clone();
        let mut written = change(&draft);
        let choice = match self.answered(draft.account_id, &written, response, scope, send)? {
            None => return Ok(String::new()),
            Some((choice, keep_time)) => {
                // Keep Old Time writes the other edits at the time the
                // event had.
                if keep_time {
                    written = change(&draft.without_move());
                }
                choice
            }
        };
        self.mail.write(draft.account_id, written, choice, &gettext("Could not save the event: {reason}"))?;
        Ok(match draft.moved() {
            true => fill(&gettext("Moved “{title}”"), &[("title", draft.title.trim())]),
            false => String::new(),
        })
    }

    /// What to ask before deleting the event, or `None` to delete at once.
    pub fn delete_question(&self) -> Result<Option<EventQuestion>, CoreError> {
        let draft = self.lock().clone();
        let Some(occurrence) = draft.occurrence.clone() else { return Ok(None) };
        self.question(draft.account_id, &EventChange::Remove(occurrence), &draft.title, None)
    }

    pub fn delete(&self, response: Option<String>, scope: Option<String>, send: bool) -> Result<String, CoreError> {
        let draft = self.lock().clone();
        let Some(occurrence) = draft.occurrence.clone() else { return Ok(String::new()) };
        let removal = EventChange::Remove(occurrence);
        let Some((choice, _)) = self.answered(draft.account_id, &removal, response, scope, send)? else {
            return Ok(String::new());
        };
        self.mail.write(draft.account_id, removal, choice, &gettext("Could not delete the event: {reason}"))?;
        Ok(fill(&gettext("Deleted “{title}”"), &[("title", draft.title.trim())]))
    }
}

impl EventEditor {
    fn lock(&self) -> std::sync::MutexGuard<'_, Draft> {
        self.draft.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn question(
        &self,
        account: AccountId,
        change: &EventChange,
        title: &str,
        when: Option<String>,
    ) -> Result<Option<EventQuestion>, CoreError> {
        let asked = self.mail.calendar.ask_before(account, change).map_err(|err| CoreError::Store(err.to_string()))?;
        let Ask::Question(question) = asked else { return Ok(None) };
        Ok(Some(EventQuestion {
            heading: event_question::heading(&question, title),
            body: event_question::body(&question, when.as_deref()),
            responses: event_question::responses(&question)
                .into_iter()
                .map(|r| QuestionButton { id: r.id.into(), label: r.label, look: look(r.look).into() })
                .collect(),
            scopes: match event_question::scope_options(&question) {
                true => question
                    .scopes
                    .iter()
                    .map(|&s| QuestionButton {
                        id: event_question::scope_id(s).into(),
                        label: event_question::scope_label(s),
                        look: "plain".into(),
                    })
                    .collect(),
                false => Vec::new(),
            },
            send_check: event_question::send_check(&question),
            default_id: event_question::default_response(&question).into(),
        }))
    }

    /// The choice the change is written with: the one nobody had to be
    /// asked, or the person's answer. `None` for Cancel.
    fn answered(
        &self,
        account: AccountId,
        change: &EventChange,
        response: Option<String>,
        scope: Option<String>,
        send: bool,
    ) -> Result<Option<(mailrs_sync::calendar_copy::event_change::Choice, bool)>, CoreError> {
        let asked = self.mail.calendar.ask_before(account, change).map_err(|err| CoreError::Store(err.to_string()))?;
        match asked {
            Ask::Settled(choice) => Ok(Some((choice, false))),
            Ask::Question(question) => {
                let picked = scope.and_then(|id| scope_of(&question, &id));
                Ok(event_question::answer(&question, response.as_deref().unwrap_or("cancel"), picked, send)
                    .map(|answer| (answer.choice(), answer.keep_time)))
            }
        }
    }
}

impl Mail {
    /// Writes `change` at once, without an Undo toast to wait on.
    fn write(
        &self,
        account: AccountId,
        change: EventChange,
        choice: mailrs_sync::calendar_copy::event_change::Choice,
        failed: &str,
    ) -> Result<(), CoreError> {
        let copy = Arc::clone(&self.calendar);
        let written = self.run(async move { copy.change(account, change, choice, Undo::Skip).await });
        match written.map(|permitted| permitted.done().is_some()) {
            Ok(true) => {
                self.changed("calendar");
                Ok(())
            }
            Ok(false) => Err(CoreError::Store(gettext("Iris has no permission to use this calendar. Allow it in Preferences."))),
            Err(err) => Err(CoreError::Store(fill(failed, &[("reason", &err.to_string())]))),
        }
    }

    /// The occurrence the grid named `id`, starting at `starts`.
    fn occurrence(&self, id: &str, starts: i64) -> Result<Option<Occurrence>, CoreError> {
        let db = self.db.clone();
        let ids: Vec<i64> = self
            .run(async move { db.read(accounts::list_accounts).await })?
            .into_iter()
            .map(|a| a.id)
            .collect();
        const DAY: EpochMillis = 86_400_000;
        let copy = Arc::clone(&self.calendar);
        let found = self
            .run(async move { copy.occurrences(&ids, starts - DAY, starts + DAY, CalendarScope::All).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        Ok(found.into_iter().find(|o| o.id() == id && o.start == starts))
    }

    /// The rules of the series `o` belongs to: its own, or for a changed
    /// occurrence the series row's.
    fn series_rules(&self, o: &Occurrence) -> Vec<String> {
        let Some(series) = o.event.series.clone() else {
            return o.event.rules.clone();
        };
        let (account, calendar, copy) = (o.account_id, o.event.calendar.clone(), Arc::clone(&self.calendar));
        self.run(async move { copy.event(account, &calendar, &series).await })
            .ok()
            .flatten()
            .map(|e| e.rules)
            .unwrap_or_default()
    }

    /// Every calendar the person can write to, with its account's address,
    /// primary ones first.
    fn writable_calendars(&self) -> Result<Vec<(AccountId, String, Calendar)>, CoreError> {
        let emails = self.emails();
        let ids: Vec<i64> = emails.iter().map(|(id, _)| *id).collect();
        let copy = Arc::clone(&self.calendar);
        let listed = self
            .run(async move { copy.listed(&ids).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let mut out = Vec::new();
        for l in listed {
            let email = emails.iter().find(|(id, _)| *id == l.account_id).map(|(_, e)| e.clone()).unwrap_or_default();
            for calendar in l.calendars.into_iter().filter(|c| c.access.can_write() && !l.unlisted.contains(&c.id)) {
                out.push((l.account_id, email.clone(), calendar));
            }
        }
        Ok(out)
    }

    fn emails(&self) -> Vec<(AccountId, String)> {
        let db = self.db.clone();
        self.run(async move { db.read(accounts::list_accounts).await })
            .map(|all| all.into_iter().map(|a| (a.id, a.email)).collect())
            .unwrap_or_default()
    }
}

/// What saving `draft` comes to: a new event, or an edit of the occurrence
/// it opened with what the guests will notice of it.
fn change(draft: &Draft) -> EventChange {
    let edited = draft.to_event(&new_event_id(), &new_event_id());
    let (Some(occurrence), Some(before)) = (draft.occurrence.clone(), draft.before()) else {
        return EventChange::New(edited);
    };
    let rest = draft.without_move();
    let how = Edit {
        moves: draft.moved(),
        seen: mailrs_appcore::calendar_draft::reaches_guests(&before, draft),
        more_than_time: rest != before,
        rest_seen: mailrs_appcore::calendar_draft::reaches_guests(&before, &rest),
        rule_changed: draft.rule_changed(),
    };
    EventChange::Edit { occurrence, edited, how }
}

/// The new time of a moved draft, in words, for the question.
fn when(draft: &Draft) -> String {
    mailrs_render::calendar_words::span_words(draft.start, draft.end, draft.all_day, &Local)
}

fn scope_of(question: &Question, id: &str) -> Option<RepeatScope> {
    question.scopes.iter().copied().find(|&s| event_question::scope_id(s) == id)
}

fn look(look: Look) -> &'static str {
    match look {
        Look::Plain => "plain",
        Look::Suggested => "suggested",
        Look::Destructive => "destructive",
    }
}

fn key(account: AccountId, calendar: &str) -> String {
    format!("{account}/{calendar}")
}

/// The zone the Mac is set to, which a new event is written in.
fn zone() -> Tz {
    iana_zone().and_then(|name| name.parse().ok()).unwrap_or(Tz::UTC)
}

/// The Mac's zone name, read from the `/etc/localtime` link as macOS keeps
/// it (`/var/db/timezone/zoneinfo/Europe/Rome`).
fn iana_zone() -> Option<String> {
    if let Ok(zone) = std::env::var("TZ")
        && !zone.is_empty()
    {
        return Some(zone.trim_start_matches(':').to_string());
    }
    let link = std::fs::read_link("/etc/localtime").ok()?;
    let text = link.to_string_lossy();
    text.split_once("zoneinfo/").map(|(_, name)| name.to_string())
}

fn utc_midnight(day: NaiveDate) -> EpochMillis {
    day.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_macs_zone_reads_from_its_link() {
        // Every Mac links /etc/localtime into the zone database.
        assert!(zone() != Tz::UTC || iana_zone().is_none_or(|z| z == "UTC"));
    }

    #[test]
    fn no_calendar_means_no_new_event() {
        let dir = tempfile::tempdir().unwrap();
        let mail = Arc::new(Mail::open(dir.path().join("mailrs.db").display().to_string()).unwrap());
        assert!(mail.new_event(0, 3_600_000, false).unwrap().is_none());
    }
}
