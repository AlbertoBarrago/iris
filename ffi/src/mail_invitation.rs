//! The invitation card above a conversation, for the Swift front end: the
//! newest invitation in the thread, opened through sync's `Invitations`
//! as the GTK thread run opens it, and worded by
//! `mailrs_appcore::invitation_card`, so both apps say the same about it.
//!
//! Opening an invitation only reads and writes the store, so each action
//! opens it again from the message rather than holding it between calls.

use std::sync::Arc;

use chrono::Local;
use mailrs_appcore::invitation_strip::{self as strip, Asked, Strip, Verdict};
use mailrs_appcore::invitation_card::{
    self as card, CalendarButton, Showing, attending, calendar_button, clash, guest_summary, joined,
    moved, nearby, news, news_tone, organizer_line, replied, said_yes, targets_of, went,
};
use mailrs_domain::invitation::{Answer, Card, Method, Scope, When};
use mailrs_domain::translate::{fill, fill_plural, gettext};
use mailrs_domain::{AccountId, EpochMillis};
use mailrs_render::format::event_when;
use mailrs_store::calendar::CalendarScope;
use mailrs_store::{accounts, bodies, messages};
use mailrs_sync::calendar::Calendar;
use mailrs_sync::invitations::Invitations;
use mailrs_sync::{Accounts, Permitted, Spot, Told, now_millis};

use crate::CoreError;
use crate::calendar::EventDetails;
use crate::mail::Mail;

/// An invitation as the card above the conversation shows it, in words.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MailInvitation {
    /// The message that carries it, which every action names.
    pub message_id: String,
    /// What this message does to an event the person already has, or
    /// that it is called off. Empty when it is plain news.
    pub news: String,
    /// `changed` or `cancelled`, the colour the news line takes.
    pub news_tone: String,
    /// The event's title, or "3 events" for a file with several.
    pub title: String,
    /// When it runs and where, on one line.
    pub when: String,
    /// How the series runs and who organizes it.
    pub meta: String,
    /// "4 of 6 said yes", empty without guests.
    pub said_yes: String,
    /// "6 guests · 4 yes, 2 awaiting", for the guest list's top.
    pub guest_summary: String,
    pub guests: Vec<InvitationGuest>,
    /// What else the person has on while it runs.
    pub clash: String,
    /// Whether it asks for an answer: a cancellation and somebody else's
    /// reply are news only.
    pub answerable: bool,
    /// Whether an answer asks if it covers this occurrence or the series.
    pub asks_scope: bool,
    /// The answer already given, `yes`, `maybe` or `no`, or empty.
    pub answer: String,
    /// Times near the asked one, for Propose New Time. Empty for an
    /// all-day event, which has no time to move.
    pub proposals: Vec<ProposedTime>,
    /// The organizer's name, for the question Pick a Time asks.
    pub organizer: String,
    /// Where the event sits on the calendar, for Show in Calendar. With
    /// none, the card offers Add to Calendar.
    pub on_calendar: Option<EventSpot>,
    /// Whether the file only describes events, which Add to Calendar puts
    /// on a calendar of the person's choosing.
    pub adds: bool,
    /// The calendars Add to Calendar offers, the account's own first.
    pub targets: Vec<AddTarget>,
    /// Each event of a file that holds several, "Title · when".
    pub events: Vec<String>,
    /// The hours around a timed request, once the calendar's copy holds
    /// them. It takes the clash line's place.
    pub strip: Option<DayStrip>,
    /// The account has a calendar but withheld the permission to read
    /// it, so the card offers Grant Access.
    pub needs_calendar_access: bool,
}

/// The hours around the event: the person's other events as blocks, this
/// one among them, and whether the hour is free.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DayStrip {
    /// "Your afternoon".
    pub heading: String,
    /// "Nothing else at that hour", or what clashes.
    pub verdict: String,
    pub free: bool,
    pub hours: Vec<StripHour>,
    pub blocks: Vec<StripBlock>,
    /// How many rows the blocks take.
    pub lanes: u32,
}

/// A whole hour on the strip, at its place along it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StripHour {
    /// From 0, the strip's left edge, to 1, its right.
    pub at: f64,
    /// "14:00".
    pub label: String,
}

/// One event on the strip, placed as fractions of its width.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StripBlock {
    pub title: String,
    pub from: f64,
    pub to: f64,
    pub lane: u32,
    /// `#rrggbb`, or empty for the accent.
    pub color: String,
    /// The invitation's own event, drawn dashed until it is answered.
    pub this: bool,
}

impl From<Strip> for DayStrip {
    fn from(strip: Strip) -> Self {
        DayStrip {
            heading: strip.heading,
            verdict: strip.verdict.words(),
            free: strip.verdict == Verdict::Free,
            hours: strip.hours.into_iter().map(|(at, label)| StripHour { at, label }).collect(),
            blocks: strip
                .blocks
                .into_iter()
                .map(|b| StripBlock {
                    title: b.title,
                    from: b.from,
                    to: b.to,
                    lane: b.lane as u32,
                    color: b.colour,
                    this: b.this,
                })
                .collect(),
            lanes: strip.lanes as u32,
        }
    }
}

/// One person on the guest list and what they said.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct InvitationGuest {
    /// "You" for the person, else the guest's name or address.
    pub name: String,
    /// "Going", "Not going", "Maybe" or "No reply yet".
    pub said: String,
}

/// A time Propose New Time offers.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProposedTime {
    /// "An Hour Later".
    pub label: String,
    pub at: i64,
}

/// Where an event sits on a calendar of the person's.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventSpot {
    pub account_id: i64,
    pub calendar: String,
    /// The event's own id, not an occurrence's.
    pub id: String,
    pub start: i64,
}

/// A calendar Add to Calendar can put a file's events on.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AddTarget {
    pub calendar: String,
    pub label: String,
    pub primary: bool,
}

/// What an action on the card did, in words.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct InvitationDone {
    /// The line under the buttons: where the answer or the proposal went.
    pub went: String,
    /// What the window says once, as a toast.
    pub toast: String,
    /// For Add to Calendar, where the first event landed, so the card
    /// turns into Show in Calendar.
    pub spot: Option<EventSpot>,
}

impl From<Spot> for EventSpot {
    fn from(spot: Spot) -> Self {
        EventSpot {
            account_id: spot.account_id,
            calendar: spot.calendar,
            id: spot.id,
            start: spot.start,
        }
    }
}

#[uniffi::export]
impl Mail {
    /// The newest invitation in the conversation, as its card shows it,
    /// or `None` when no message of the thread carries one.
    pub fn invitation(&self, account_id: i64, thread_id: String) -> Result<Option<MailInvitation>, CoreError> {
        let Some(message_id) = self.carrier(account_id, thread_id)? else {
            return Ok(None);
        };
        let Some(showing) = self.showing(account_id, &message_id)? else {
            return Ok(None);
        };
        Ok(Some(self.worded(account_id, showing)))
    }

    /// Sends `answer` (`yes`, `maybe` or `no`) to the invitation in
    /// `message_id`, for this occurrence or, with `series`, every one,
    /// with `note` for the organizer to read.
    pub fn answer_invitation(
        &self,
        account_id: i64,
        message_id: String,
        answer: String,
        series: bool,
        note: String,
    ) -> Result<InvitationDone, CoreError> {
        let Ok(answer) = answer.parse::<Answer>() else {
            return Err(CoreError::Store(format!("not an answer: {answer}")));
        };
        let (invitation, me) = self.answerable(account_id, &message_id)?;
        let organizer = invitation.organizer.as_ref().map(|who| who.display().to_string());
        let scope = if series { Scope::Series } else { Scope::Occurrence };
        let note = Some(note.trim().to_string()).filter(|n| !n.is_empty());
        let invitations = self.invitations();
        let sent = self
            .run(async move { invitations.answer(account_id, &invitation, &me, answer, scope, note, now_millis()).await })
            .map_err(|err| failed(&gettext("Could not send your reply: {reason}"), &err))?;
        if sent.told == Told::Nobody {
            return Err(CoreError::Store(gettext(
                "This invitation names no organizer, so there is nobody to reply to",
            )));
        }
        // An answer on the calendar waits in its queue, which is sent now
        // so the organizer hears at once.
        if sent.told == Told::Calendar {
            self.changed("calendar");
            self.send_calendar(account_id);
        }
        let mut toast = replied(answer, sent.told);
        if sent.needs_permission {
            toast = gettext("Iris has no permission to use this calendar. Allow it in Preferences.");
        } else if let Some(off) = &sent.api_off {
            toast = fill(
                &gettext("The {service} is switched off for Iris, so your answer went by email"),
                &[("service", &off.service)],
            );
        }
        Ok(InvitationDone {
            went: went(sent.told, organizer.as_deref()),
            toast,
            spot: None,
        })
    }

    /// Asks the organizer of the invitation in `message_id` to move it to
    /// `at`, keeping its length, for this occurrence or the series. A
    /// proposal settles nothing, so the answer stays as it was.
    pub fn propose_invitation(
        &self,
        account_id: i64,
        message_id: String,
        at: i64,
        series: bool,
    ) -> Result<InvitationDone, CoreError> {
        let (invitation, me) = self.answerable(account_id, &message_id)?;
        let organizer = invitation.organizer.as_ref().map(|who| who.display().to_string());
        // An all-day event has no time to move; the card offers none.
        let Some(when) = moved(&invitation, at) else {
            return Err(CoreError::Store(gettext("This invitation has no time to move")));
        };
        let scope = if series { Scope::Series } else { Scope::Occurrence };
        let invitations = self.invitations();
        let told = self
            .run(async move { invitations.propose(account_id, &invitation, &me, &when, scope, now_millis()).await })
            .map_err(|err| failed(&gettext("Could not send your proposal: {reason}"), &err))?;
        if told == Told::Nobody {
            return Err(CoreError::Store(gettext(
                "This invitation names no organizer, so there is nobody to ask",
            )));
        }
        Ok(InvitationDone {
            went: match &organizer {
                Some(organizer) => fill(&gettext("Proposed a new time to {organizer}"), &[("organizer", organizer)]),
                None => gettext("Proposed a new time"),
            },
            toast: gettext("New time proposed. The organizer decides."),
            spot: None,
        })
    }

    /// Adds the events of the file in `message_id` to `calendar`, or to
    /// the account's own when it is empty. `picked` names the events by
    /// their place in the file; empty adds every one.
    pub fn add_invitation(
        &self,
        account_id: i64,
        message_id: String,
        calendar: String,
        picked: Vec<u32>,
    ) -> Result<InvitationDone, CoreError> {
        let Some(showing) = self.showing(account_id, &message_id)? else {
            return Err(CoreError::Store(gettext("The event is no longer on the calendar")));
        };
        let events: Vec<_> = std::iter::once(showing.invitation)
            .chain(showing.also)
            .enumerate()
            .filter(|(at, _)| picked.is_empty() || picked.contains(&(*at as u32)))
            .map(|(_, event)| event)
            .collect();
        let target = Some(calendar).filter(|c| !c.is_empty());
        let adding = self.calendar_actions();
        let done = self
            .run(async move { adding.import(account_id, target.as_deref(), &events).await })
            .map_err(|err| failed(&gettext("Could not add to your calendar: {reason}"), &err))?;
        let added = match done {
            Permitted::Done(added) => added,
            Permitted::NeedsPermission => {
                return Err(CoreError::Store(gettext(
                    "Iris has no permission to use this calendar. Allow it in Preferences.",
                )));
            }
        };
        let Some(first) = added.spots.first().cloned() else {
            return Err(CoreError::Store(gettext(
                "Nothing in this file can be added, since its events have no id or start time",
            )));
        };
        self.changed("calendar");
        self.send_calendar(account_id);
        let count = added.spots.len();
        let went = match (added.calendar.is_empty(), count) {
            (true, 1) => gettext("Added to your calendar"),
            (true, count) => fill_plural(
                "Added {count} events to your calendar",
                "Added {count} events to your calendar",
                count,
                &[("count", &count.to_string())],
            ),
            (false, 1) => fill(&gettext("Added to {calendar}"), &[("calendar", &added.calendar)]),
            (false, count) => fill_plural(
                "Added {count} events to {calendar}",
                "Added {count} events to {calendar}",
                count,
                &[("count", &count.to_string()), ("calendar", &added.calendar)],
            ),
        };
        let toast = match added.skipped {
            0 => String::new(),
            _ => gettext("Some events were left out, since they have no id or start time"),
        };
        Ok(InvitationDone {
            went,
            toast,
            spot: Some(first.into()),
        })
    }

    /// The event at `spot` as the calendar's card shows it, for Show in
    /// Calendar. `None` once it has left the calendar.
    pub fn event_at(&self, spot: EventSpot) -> Result<Option<EventDetails>, CoreError> {
        self.event_details(spot.account_id, &spot.calendar, &spot.id, spot.start)
    }
}

impl Mail {
    fn invitations(&self) -> Invitations<crate::mail::Running> {
        Invitations::new(Arc::clone(&self.running), self.db.clone(), Arc::clone(&self.calendar))
    }

    fn calendar_actions(&self) -> Calendar<crate::mail::Running> {
        Calendar::new(Arc::clone(&self.running), self.db.clone(), Arc::clone(&self.calendar))
    }

    /// The newest message of the thread that carries an invitation.
    fn carrier(&self, account_id: AccountId, thread_id: String) -> Result<Option<String>, CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move {
            db.read(move |c| {
                for meta in messages::thread_messages(c, account_id, &thread_id)?.into_iter().rev() {
                    let carries = bodies::peek_body(c, account_id, &meta.id)?
                        .is_some_and(|body| body.calendar.is_some());
                    if carries {
                        return Ok(Some(meta.id));
                    }
                }
                Ok(None)
            })
            .await
        })?)
    }

    /// The invitation in `message_id`, opened as the GTK thread run opens
    /// it: with the news it brings and the answer already given.
    fn showing(&self, account_id: AccountId, message_id: &str) -> Result<Option<Showing>, CoreError> {
        let Some(ics) = self.stored_body(account_id, message_id)?.calendar else {
            return Ok(None);
        };
        let invitations = self.invitations();
        let id = message_id.to_string();
        let opened = self
            .run(async move { invitations.open(account_id, &id, &ics, now_millis()).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let me = self.addresses_of(account_id)?;
        Ok(opened.map(|opened| Showing {
            message_id: message_id.to_string(),
            invitation: opened.invitation,
            also: opened.also,
            change: opened.change,
            answer: opened.answer,
            me,
            on_calendar: None,
        }))
    }

    /// The invitation in `message_id` with the address an answer goes out
    /// as, or the reason it cannot be answered.
    fn answerable(
        &self,
        account_id: AccountId,
        message_id: &str,
    ) -> Result<(mailrs_domain::invitation::Invitation, mailrs_domain::Address), CoreError> {
        let showing = self
            .showing(account_id, message_id)?
            .ok_or_else(|| CoreError::Store(gettext("This invitation names no event, so there is nothing to answer")))?;
        if showing.invitation.uid.trim().is_empty() {
            return Err(CoreError::Store(gettext(
                "This invitation names no event, so there is nothing to answer",
            )));
        }
        let me = showing
            .answering_as()
            .ok_or_else(|| CoreError::Store("The account has no address.".into()))?;
        Ok((showing.invitation, me))
    }

    /// The addresses of the account, so the card finds the person among
    /// the guests.
    fn addresses_of(&self, account_id: AccountId) -> Result<Vec<String>, CoreError> {
        let db = self.db.clone();
        Ok(self
            .run(async move { db.read(accounts::list_accounts).await })?
            .into_iter()
            .filter(|a| a.id == account_id)
            .map(|a| a.email)
            .collect())
    }

    /// The card's words for `showing`, with what the calendar adds: how
    /// the series runs, where the event sits, what clashes with it, and
    /// where a file's events can go. A calendar that cannot answer leaves
    /// its part out rather than the card.
    fn worded(&self, account_id: AccountId, mut showing: Showing) -> MailInvitation {
        let invitations = self.invitations();
        let event = showing.invitation.clone();
        let cancelled = event.cancelled();
        let request = event.method == Method::Request;
        // An invitation to one occurrence carries no rule of its own, so
        // the series line comes from the calendar.
        if event.occurrence.is_some() && request && !cancelled {
            let (asked, invitation) = (self.invitations(), event.clone());
            match self.run(async move { asked.series(account_id, &invitation, now_millis()).await }) {
                Ok(Some(line)) => showing.invitation.repeats = Some(line),
                Ok(None) => {}
                Err(err) => tracing::info!(error = %err, "could not read the series"),
            }
        }
        if !cancelled {
            let (asked, invitation) = (self.invitations(), event.clone());
            match self.run(async move { asked.on_calendar(account_id, &invitation, now_millis()).await }) {
                Ok(spot) => showing.on_calendar = spot,
                Err(err) => tracing::info!(error = %err, "could not look for the event on the calendar"),
            }
        }
        // A timed request still on gets the hours around it, on the
        // occurrence Show in Calendar opens, in place of the clash line.
        let timed = event.when.as_ref().is_some_and(|when| !when.all_day());
        let strip = if request && !cancelled && timed {
            let at = showing.on_calendar.as_ref().map(|spot| spot.start);
            self.strip(account_id, &event, at).unwrap_or_else(|err| {
                tracing::info!(error = %err, "could not read the hours around the event");
                None
            })
        } else {
            None
        };
        let clashes = if strip.is_none() && showing.answer.is_none() && request && !cancelled {
            let invitation = event.clone();
            self.run(async move { invitations.busy(account_id, &invitation).await })
                .unwrap_or_else(|err| {
                    tracing::info!(error = %err, "could not read the calendar");
                    Vec::new()
                })
        } else {
            Vec::new()
        };
        let adds = card::adds(&showing);
        let targets = if adds && calendar_button(&showing) == CalendarButton::Add {
            self.targets(account_id)
        } else {
            Vec::new()
        };
        let mut card = words(showing, clashes, targets);
        card.strip = strip.map(Into::into);
        card.needs_calendar_access = !cancelled
            && self.running.services(account_id).is_some_and(|s| {
                mailrs_appcore::permission::card_offers_calendar_access(s.offers(), s.withheld())
            });
        card
    }

    /// The hours around `invitation`, from the calendar's copy, at `at`
    /// when the copy holds it there. `None` before the copy is read.
    fn strip(
        &self,
        account_id: AccountId,
        invitation: &mailrs_domain::invitation::Invitation,
        at: Option<EpochMillis>,
    ) -> Result<Option<Strip>, CoreError> {
        let Some(When::At { starts_at, ends_at }) = invitation.when else {
            return Ok(None);
        };
        // An invitation with no end runs an hour, as the clash line
        // assumes. A series keeps its length on the occurrence found.
        let length = ends_at.unwrap_or(starts_at + 3_600_000) - starts_at;
        let start = at.unwrap_or(starts_at);
        let end = start + length;
        let Some(span) = strip::window(start, &Local) else {
            return Ok(None);
        };
        let read = (span.0.min(start), span.1.max(end));
        let db = self.db.clone();
        let found = self.run(async move {
            db.read(move |c| {
                if !mailrs_store::calendar::synced(c, account_id)? {
                    return Ok(None);
                }
                let around =
                    mailrs_store::calendar::occurrences(c, &[account_id], read.0, read.1, CalendarScope::Owned)?;
                let colours: std::collections::HashMap<String, String> =
                    mailrs_store::calendar::calendars(c, account_id)?
                        .into_iter()
                        .map(|calendar| (calendar.id, calendar.color))
                        .collect();
                Ok(Some((around, colours)))
            })
            .await
        })?;
        Ok(found.map(|(around, colours)| {
            let asked = Asked {
                uid: &invitation.uid,
                start,
                end,
            };
            strip::build(&asked, span, &around, &colours, &Local)
        }))
    }

    /// The calendars of the account a file's events can go on.
    fn targets(&self, account_id: AccountId) -> Vec<AddTarget> {
        let listing = self.calendar_actions();
        match self.run(async move { listing.calendars(account_id).await }) {
            Ok(Permitted::Done(list)) => targets_of(account_id, list, None)
                .into_iter()
                .map(|t| AddTarget {
                    calendar: t.calendar,
                    label: t.label,
                    primary: t.primary,
                })
                .collect(),
            Ok(Permitted::NeedsPermission) => Vec::new(),
            Err(err) => {
                tracing::info!(error = %err, "could not list the calendars");
                Vec::new()
            }
        }
    }
}

/// The card's words for an invitation, the clashes the calendar found,
/// and the calendars a file can go on.
fn words(showing: Showing, clashes: Vec<String>, targets: Vec<AddTarget>) -> MailInvitation {
    let now = Local::now();
    let event = &showing.invitation;
    let untitled = |summary: &str| match summary.trim().is_empty() {
        true => gettext("Untitled event"),
        false => summary.to_string(),
    };
    let when = event.when.as_ref().map(|when| event_when(when, now));
    let several = !showing.also.is_empty();
    let title = match several {
        true => fill_plural(
            "{count} events",
            "{count} events",
            showing.also.len() + 1,
            &[("count", &(showing.also.len() + 1).to_string())],
        ),
        false => untitled(&event.summary),
    };
    let events = match several {
        true => std::iter::once(event)
            .chain(&showing.also)
            .map(|e| joined(&[Some(untitled(&e.summary)), e.when.as_ref().map(|w| event_when(w, now))]))
            .collect(),
        false => Vec::new(),
    };
    let guests = attending(&showing);
    let answerable = event.card() == Card::Answer;
    let proposals = match event.when {
        Some(When::At { starts_at, .. }) if answerable => nearby(starts_at)
            .into_iter()
            .map(|(label, at)| ProposedTime { label, at })
            .collect(),
        _ => Vec::new(),
    };
    MailInvitation {
        message_id: showing.message_id.clone(),
        news: news(&showing, now).unwrap_or_default(),
        news_tone: news_tone(&showing).to_string(),
        title,
        when: if several { String::new() } else { joined(&[when, event.location.clone()]) },
        meta: joined(&[event.repeats.clone(), organizer_line(event)]),
        said_yes: if guests.is_empty() { String::new() } else { said_yes(&guests) },
        guest_summary: if guests.is_empty() { String::new() } else { guest_summary(&guests) },
        guests: guests
            .iter()
            .map(|g| InvitationGuest {
                name: g.name.clone(),
                said: g.answer.map_or_else(|| gettext("No reply yet"), Answer::said),
            })
            .collect(),
        clash: clash(&clashes).unwrap_or_default(),
        answerable,
        asks_scope: answerable && event.occurrence.is_some(),
        answer: showing.answer.map(|a| a.as_str().to_string()).unwrap_or_default(),
        proposals,
        organizer: event.organizer.as_ref().map(|o| o.display().to_string()).unwrap_or_default(),
        adds: card::adds(&showing),
        on_calendar: showing.on_calendar.clone().map(Into::into),
        targets,
        events,
        strip: None,
        needs_calendar_access: false,
    }
}

/// `pattern` with the error in its `{reason}`.
fn failed(pattern: &str, err: &impl std::fmt::Display) -> CoreError {
    CoreError::Store(fill(pattern, &[("reason", &err.to_string())]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn showing(method: &str, extra: &[&str]) -> Showing {
        let mut lines = vec![
            "BEGIN:VCALENDAR".to_string(),
            format!("METHOD:{method}"),
            "BEGIN:VEVENT".to_string(),
            "UID:design@example.com".to_string(),
            "SUMMARY:Design review".to_string(),
            "DTSTART:20240610T090000Z".to_string(),
            "DTEND:20240610T100000Z".to_string(),
            "ORGANIZER;CN=Priya Raman:mailto:priya@example.com".to_string(),
        ];
        lines.extend(extra.iter().map(|l| l.to_string()));
        lines.extend(["END:VEVENT".to_string(), "END:VCALENDAR".to_string(), String::new()]);
        Showing {
            message_id: "m1".to_string(),
            invitation: mailrs_domain::invitation::read(&lines.join("\r\n")).expect("an event"),
            also: Vec::new(),
            change: None,
            answer: None,
            me: vec!["me@example.com".to_string()],
            on_calendar: None,
        }
    }

    #[test]
    fn a_request_asks_for_an_answer_and_offers_times() {
        let card = words(
            showing("REQUEST", &["ATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:me@example.com"]),
            vec!["Standup".to_string()],
            Vec::new(),
        );
        assert!(card.answerable);
        assert!(!card.asks_scope);
        assert_eq!(card.title, "Design review");
        assert_eq!(card.meta, "Priya Raman, organizer");
        assert_eq!(card.clash, "You have Standup then");
        assert_eq!(card.proposals.len(), 4);
        assert_eq!(card.guests[0].name, "You");
        assert_eq!(card.guests[0].said, "No reply yet");
        assert!(card.news.is_empty());
    }

    #[test]
    fn a_cancellation_is_news_with_nothing_to_answer() {
        let card = words(showing("CANCEL", &[]), Vec::new(), Vec::new());
        assert!(!card.answerable);
        assert!(card.proposals.is_empty());
        assert_eq!(card.news, "This meeting is canceled");
        assert_eq!(card.news_tone, "cancelled");
    }

    #[test]
    fn the_answer_given_comes_back_as_its_word() {
        let mut held = showing("REQUEST", &[]);
        held.answer = Some(Answer::Maybe);
        assert_eq!(words(held, Vec::new(), Vec::new()).answer, "maybe");
    }

    #[test]
    fn an_all_day_event_has_no_time_to_propose() {
        let mut held = showing("REQUEST", &[]);
        held.invitation.when = Some(When::Days {
            first: chrono::NaiveDate::from_ymd_opt(2024, 6, 10).expect("a day"),
            last: chrono::NaiveDate::from_ymd_opt(2024, 6, 10).expect("a day"),
        });
        assert!(words(held, Vec::new(), Vec::new()).proposals.is_empty());
    }

    #[test]
    fn a_spot_keeps_the_event_it_names() {
        let spot: EventSpot = Spot {
            account_id: 2,
            calendar: "primary".into(),
            id: "design".into(),
            start: 7,
        }
        .into();
        assert_eq!((spot.account_id, spot.id.as_str(), spot.start), (2, "design", 7));
    }

    #[test]
    fn a_strip_keeps_its_blocks_and_says_whether_the_hour_is_free() {
        let built = Strip {
            heading: "Your morning".into(),
            hours: vec![(0.2, "09:00".into())],
            blocks: vec![strip::Block {
                title: "This meeting".into(),
                from: 0.4,
                to: 0.6,
                lane: 0,
                colour: "#3366ff".into(),
                this: true,
            }],
            lanes: 1,
            verdict: Verdict::Free,
        };
        let shown: DayStrip = built.into();
        assert!(shown.free);
        assert_eq!(shown.verdict, "Nothing else at that hour");
        assert_eq!(shown.hours[0].label, "09:00");
        assert_eq!((shown.blocks[0].lane, shown.blocks[0].this), (0, true));
    }
}
