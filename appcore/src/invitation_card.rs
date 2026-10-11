//! The words on an invitation's card: the news line, the organizer and
//! the guests, the clash, the times to propose, and what an answer says
//! after it went. Both front ends draw the card from these, so the GTK
//! window and Iris word an invitation alike.

use chrono::{DateTime, Days, Local, TimeDelta};
use mailrs_domain::invitation::{Answer, Card, Invitation, Method, When};
use mailrs_domain::translate::{fill, fill_plural, gettext};
use mailrs_domain::{AccountId, Address, EpochMillis};
use mailrs_render::calendar_words;
use mailrs_render::format::event_moved_from;
use mailrs_sync::{Change, Spot, Told};

/// A calendar the card offers to add a file's events to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTo {
    pub account_id: AccountId,
    /// The provider's id for the calendar.
    pub calendar: String,
    /// What the picker says: the calendar's name, with the account's
    /// address after it when the person has more than one.
    pub label: String,
    /// The account's own calendar, which the picker starts on.
    pub primary: bool,
}

/// The calendars of one account a file's events can go on: the ones it
/// can write to, its own first. `account` is added to each label when
/// the person has several accounts.
pub fn targets_of(
    account_id: AccountId,
    calendars: Vec<mailrs_domain::calendar::Calendar>,
    account: Option<&str>,
) -> Vec<AddTo> {
    let mut found: Vec<AddTo> = calendars
        .into_iter()
        .filter(|calendar| calendar.access.can_write())
        .map(|calendar| AddTo {
            account_id,
            label: match account {
                Some(account) => fill(
                    &gettext("{calendar} ({account})"),
                    &[("calendar", &calendar.name), ("account", account)],
                ),
                None => calendar.name.clone(),
            },
            primary: calendar.primary,
            calendar: calendar.id,
        })
        .collect();
    found.sort_by_key(|target| !target.primary);
    found
}

/// One invitation as the card shows it.
#[derive(Clone)]
pub struct Showing {
    /// The message that carries the invitation, which the card sits in.
    pub message_id: String,
    pub invitation: Invitation,
    /// The other events of the same file, for a card that adds them.
    pub also: Vec<Invitation>,
    pub change: Option<Change>,
    /// The answer the user already sent, if any. It wins over the guest
    /// list the organizer sent, which was written before the user answered.
    pub answer: Option<Answer>,
    /// The addresses of the account the message arrived in, so the card can
    /// find the user among the guests and call them "You".
    pub me: Vec<String>,
    /// Where the event sits in the calendar's copy on this computer, once
    /// the thread run has looked. `None` until then, and for an event the
    /// copy lacks.
    pub on_calendar: Option<Spot>,
}

impl Showing {
    /// The address an answer goes out as: the one the invitation reached,
    /// under the name the organizer put on the guest list, so a reply
    /// matches the guest it answers for. An invitation that lists none of
    /// the account's addresses answers as the account itself.
    pub fn answering_as(&self) -> Option<Address> {
        let guest = self.invitation.me(&self.me);
        Some(Address {
            name: guest.and_then(|guest| guest.who.name.clone()),
            email: match guest {
                Some(guest) => guest.who.email.clone(),
                None => self.me.first()?.clone(),
            },
        })
    }
}

/// One line of the guest list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attending {
    pub name: String,
    pub answer: Option<Answer>,
}

/// Whether a block on the strip draws dashed: the calendar's sign for a
/// meeting the person has not answered, so only this meeting, and only
/// until an answer exists.
pub fn dashed(this: bool, answer: Option<Answer>) -> bool {
    this && answer.is_none()
}

/// The answers in the order the card offers them.
pub const ANSWERS: [Answer; 3] = [Answer::Yes, Answer::Maybe, Answer::No];

/// What goes between two parts of one line on the card.
pub const DOT: &str = " · ";

/// Whether an answer that names `uid` belongs to the invitation on
/// screen, `on_screen` being the UID the card holds and `None` meaning no
/// invitation is up. An answer to one invitation must never land on
/// another, and an invitation the organizer gave no UID answers for
/// nothing, since there is no telling it from the next one.
pub fn still_showing(on_screen: Option<&str>, uid: &str) -> bool {
    !uid.trim().is_empty() && on_screen == Some(uid)
}

/// Whether the card adds the events of a file to a calendar rather
/// than answering an invitation.
pub fn adds(showing: &Showing) -> bool {
    showing.invitation.card() == Card::Add
}

/// Which calendar button the card offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarButton {
    /// Show in Calendar: the event is on a calendar the app shows.
    Show,
    /// Add to Calendar, which hands the `.ics` to the desktop.
    Add,
}

pub fn calendar_button(showing: &Showing) -> CalendarButton {
    match showing.on_calendar {
        Some(_) => CalendarButton::Show,
        None => CalendarButton::Add,
    }
}

/// The times the propose list offers, each the same meeting moved whole.
/// A day and a week go through the local calendar rather than through
/// arithmetic on the instant, so the hour stays put across a clock change.
pub fn nearby(starts_at: EpochMillis) -> Vec<(String, EpochMillis)> {
    let Some(start) = DateTime::from_timestamp_millis(starts_at).map(|at| at.with_timezone(&Local))
    else {
        return Vec::new();
    };
    [
        (
            gettext("Half an Hour Later"),
            Some(start + TimeDelta::minutes(30)),
        ),
        (gettext("An Hour Later"), Some(start + TimeDelta::hours(1))),
        (
            gettext("Same Time Tomorrow"),
            start.checked_add_days(Days::new(1)),
        ),
        (
            gettext("Same Time Next Week"),
            start.checked_add_days(Days::new(7)),
        ),
    ]
    .into_iter()
    .filter_map(|(label, at)| Some((label, at?.timestamp_millis())))
    .collect()
}

/// "You have Design crit then", for an event the user already has while
/// this one runs. Two clashes name both; more than two name the first and
/// count the rest, since the point is that the hour is taken.
pub fn clash(busy: &[String]) -> Option<String> {
    Some(match busy {
        [] => return None,
        [one] => fill(&gettext("You have {event} then"), &[("event", one)]),
        [one, two] => fill(
            &gettext("You have {event} and {other} then"),
            &[("event", one), ("other", two)],
        ),
        [one, rest @ ..] => fill_plural(
            "You have {event} and {count} more then",
            "You have {event} and {count} more then",
            rest.len(),
            &[("event", one), ("count", &rest.len().to_string())],
        ),
    })
}

/// "Priya Raman, organizer", or nothing when the organizer is missing.
pub fn organizer_line(event: &Invitation) -> Option<String> {
    let who = event.organizer.as_ref()?;
    Some(match event.method {
        Method::Reply => fill(
            &gettext("Reply to the invitation from {organizer}"),
            &[("organizer", who.display())],
        ),
        // The calendar's popover words its people line the same way.
        _ => fill(&gettext("{name}, organizer"), &[("name", who.display())]),
    })
}

/// The parts of one line on the card that have words, with a dot
/// between each two.
pub fn joined(parts: &[Option<String>]) -> String {
    parts
        .iter()
        .flatten()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(DOT)
}

/// "4 of 6 said yes", which opens the guest list, in the words the
/// calendar's event popover uses for the same event.
pub fn said_yes(guests: &[Attending]) -> String {
    calendar_words::said_yes_words(guests.iter().map(|g| g.answer))
}

/// The guest list as the card shows it: the user's own row reads "You"
/// and carries the answer they sent, which the organizer's copy of the
/// list predates.
pub fn attending(showing: &Showing) -> Vec<Attending> {
    let me = showing
        .invitation
        .me(&showing.me)
        .map(|guest| guest.who.email.clone());
    showing
        .invitation
        .guests
        .iter()
        .map(|guest| {
            let mine = me.as_deref() == Some(guest.who.email.as_str());
            Attending {
                name: if mine {
                    gettext("You")
                } else {
                    guest.who.display().to_string()
                },
                answer: if mine {
                    showing.answer.or(guest.answer)
                } else {
                    guest.answer
                },
            }
        })
        .collect()
}

/// "4 guests · 2 yes, 1 maybe, 1 awaiting".
pub fn guest_summary(guests: &[Attending]) -> String {
    let count = |wanted: Option<Answer>| guests.iter().filter(|g| g.answer == wanted).count();
    let (yes, no, maybe) = (
        count(Some(Answer::Yes)),
        count(Some(Answer::No)),
        count(Some(Answer::Maybe)),
    );
    let waiting = count(None);
    let mut parts = Vec::new();
    if yes > 0 {
        parts.push(fill_plural(
            "{count} yes",
            "{count} yes",
            yes,
            &[("count", &yes.to_string())],
        ));
    }
    if no > 0 {
        parts.push(fill_plural(
            "{count} no",
            "{count} no",
            no,
            &[("count", &no.to_string())],
        ));
    }
    if maybe > 0 {
        parts.push(fill_plural(
            "{count} maybe",
            "{count} maybe",
            maybe,
            &[("count", &maybe.to_string())],
        ));
    }
    if waiting > 0 {
        parts.push(fill_plural(
            "{count} awaiting",
            "{count} awaiting",
            waiting,
            &[("count", &waiting.to_string())],
        ));
    }
    let all = guests.len();
    let counted = fill_plural(
        "{count} guest",
        "{count} guests",
        all,
        &[("count", &all.to_string())],
    );
    fill(
        &gettext("{guests} · {answers}"),
        &[("guests", &counted), ("answers", &parts.join(", "))],
    )
}

/// The line above the card: what this message does to an event the user
/// already has, or that the organizer called it off.
pub fn news(showing: &Showing, now: chrono::DateTime<Local>) -> Option<String> {
    match showing.change {
        Some(Change::Moved { was, all_day }) => Some(fill(
            &gettext("This meeting moved from {when}"),
            &[("when", &event_moved_from(was, all_day, now))],
        )),
        Some(Change::Updated) => Some(gettext("The organizer changed this meeting")),
        Some(Change::Cancelled) => Some(gettext("The organizer canceled this meeting")),
        None if showing.invitation.cancelled() => Some(gettext("This meeting is canceled")),
        None if showing.invitation.method == Method::Reply => None,
        None => None,
    }
}

/// The colour the news line takes: a cancellation reads as a warning, an
/// update as a note.
pub fn news_tone(showing: &Showing) -> &'static str {
    match showing.change {
        Some(Change::Cancelled) => "cancelled",
        _ if showing.invitation.cancelled() => "cancelled",
        _ => "changed",
    }
}

/// The event moved to `starts_at`, keeping the length the organizer gave
/// it. An event with no end of its own is proposed as an hour, which is
/// what the calendar on the other side will draw.
pub fn moved(invitation: &Invitation, starts_at: EpochMillis) -> Option<When> {
    let Some(When::At {
        starts_at: was,
        ends_at,
    }) = invitation.when
    else {
        return None;
    };
    let length = ends_at.map_or(60 * 60 * 1_000, |ends_at| ends_at - was);
    Some(When::At {
        starts_at,
        ends_at: Some(starts_at + length),
    })
}

/// The toast an answer leaves: the answer the user gave, and that the
/// organizer now knows it.
pub fn replied(answer: Answer, told: Told) -> String {
    let said = match answer {
        Answer::Yes => gettext("Replied Yes"),
        Answer::No => gettext("Replied No"),
        Answer::Maybe => gettext("Replied Maybe"),
    };
    let pattern = match told {
        Told::Calendar => gettext("{answer}. The organizer has been told."),
        _ => gettext("{answer}. Your reply is on its way to the organizer."),
    };
    fill(&pattern, &[("answer", &said)])
}

/// The line under the buttons: where the answer went. Google files the
/// answer on the user's own calendar as well, so the two roads leave the
/// user in different places and the card says which.
pub fn went(told: Told, organizer: Option<&str>) -> String {
    match (told, organizer) {
        (Told::Calendar, _) => gettext("Answered on your calendar"),
        (_, Some(organizer)) => fill(
            &gettext("Replied by email to {organizer}"),
            &[("organizer", organizer)],
        ),
        (_, None) => gettext("Replied by email"),
    }
}

/// A file name for the event, so the calendar app shows something better
/// than a random string while it imports.
pub fn file_name(summary: &str) -> String {
    let stem: String = summary
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let stem = stem.trim_matches('-').to_lowercase();
    let stem: String = stem.chars().take(48).collect();
    if stem.is_empty() {
        "invitation.ics".into()
    } else {
        format!("{stem}.ics")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One event, written as a mail client writes it. `extra` holds the
    /// attendees and whatever else a test needs in the `VEVENT`.
    fn ics(method: &str, extra: &[&str]) -> String {
        let mut lines = vec![
            "BEGIN:VCALENDAR".to_string(),
            format!("METHOD:{method}"),
            "BEGIN:VEVENT".to_string(),
            "UID:design@example.com".to_string(),
            "SUMMARY:Design review".to_string(),
            "DTSTART:20240610T090000Z".to_string(),
            "ORGANIZER;CN=Priya Raman:mailto:priya@example.com".to_string(),
        ];
        lines.extend(extra.iter().map(|line| line.to_string()));
        lines.push("END:VEVENT".to_string());
        lines.push("END:VCALENDAR".to_string());
        lines.push(String::new());
        lines.join("\r\n")
    }

    fn showing(method: &str, extra: &[&str]) -> Showing {
        Showing {
            message_id: "m1".to_string(),
            invitation: mailrs_domain::invitation::read(&ics(method, extra))
                .expect("the part holds an event"),
            also: Vec::new(),
            change: None,
            answer: None,
            me: vec!["me@example.com".to_string()],
            on_calendar: None,
        }
    }

    #[test]
    fn this_meeting_is_dashed_until_answered() {
        assert!(dashed(true, None));
        for answer in Answer::ALL {
            assert!(!dashed(true, Some(answer)), "{answer:?}");
        }
    }

    #[test]
    fn other_events_are_never_dashed() {
        assert!(!dashed(false, None));
        assert!(!dashed(false, Some(Answer::Yes)));
    }

    #[test]
    fn an_event_on_the_calendar_offers_show_in_place_of_add() {
        let mut showing = showing("REQUEST", &[]);
        assert_eq!(calendar_button(&showing), CalendarButton::Add);
        showing.on_calendar = Some(Spot {
            account_id: 1,
            calendar: "primary".to_string(),
            id: "design".to_string(),
            start: 1_718_010_000_000,
        });
        assert_eq!(calendar_button(&showing), CalendarButton::Show);
    }

    #[test]
    fn an_answer_lands_only_on_the_invitation_it_answers() {
        assert!(still_showing(
            Some("design@example.com"),
            "design@example.com"
        ));
        assert!(!still_showing(
            Some("budget@example.com"),
            "design@example.com"
        ));
        assert!(!still_showing(None, "design@example.com"));
    }

    #[test]
    fn an_invitation_with_no_uid_answers_for_nothing() {
        assert!(!still_showing(Some(""), ""));
        assert!(!still_showing(Some("   "), "   "));
        assert!(!still_showing(Some("design@example.com"), ""));
    }

    #[test]
    fn a_taken_hour_names_what_takes_it() {
        assert_eq!(clash(&[]), None);
        assert_eq!(
            clash(&["Design crit".to_string()]).as_deref(),
            Some("You have Design crit then")
        );
        assert_eq!(
            clash(&["Design crit".to_string(), "Standup".to_string()]).as_deref(),
            Some("You have Design crit and Standup then")
        );
        assert_eq!(
            clash(&[
                "Design crit".to_string(),
                "Standup".to_string(),
                "One to one".to_string(),
            ])
            .as_deref(),
            Some("You have Design crit and 2 more then")
        );
    }

    #[test]
    fn the_organizer_line_follows_who_is_speaking() {
        assert_eq!(
            organizer_line(&showing("REQUEST", &[]).invitation).as_deref(),
            Some("Priya Raman, organizer")
        );
        assert_eq!(
            organizer_line(&showing("REPLY", &[]).invitation).as_deref(),
            Some("Reply to the invitation from Priya Raman")
        );
        let mut anonymous = showing("REQUEST", &[]).invitation;
        anonymous.organizer = None;
        assert_eq!(organizer_line(&anonymous), None);
    }

    #[test]
    fn the_guest_list_calls_the_user_you_and_carries_their_answer() {
        let mut showing = showing(
            "REQUEST",
            &[
                "ATTENDEE;PARTSTAT=ACCEPTED;CN=Ann Lee:mailto:ann@example.com",
                "ATTENDEE;PARTSTAT=NEEDS-ACTION;CN=Me:mailto:me@example.com",
            ],
        );
        showing.answer = Some(Answer::Yes);
        let guests = attending(&showing);
        assert_eq!(guests[0].name, "Ann Lee");
        assert_eq!(guests[0].answer, Some(Answer::Yes));
        assert_eq!(guests[1].name, "You");
        assert_eq!(guests[1].answer, Some(Answer::Yes));
    }

    #[test]
    fn the_guest_summary_counts_each_answer() {
        let showing = showing(
            "REQUEST",
            &[
                "ATTENDEE;PARTSTAT=ACCEPTED;CN=Ann Lee:mailto:ann@example.com",
                "ATTENDEE;PARTSTAT=ACCEPTED;CN=Bo Chen:mailto:bo@example.com",
                "ATTENDEE;PARTSTAT=TENTATIVE;CN=Cal Diaz:mailto:cal@example.com",
                "ATTENDEE;PARTSTAT=NEEDS-ACTION;CN=Me:mailto:me@example.com",
            ],
        );
        assert_eq!(
            guest_summary(&attending(&showing)),
            "4 guests · 2 yes, 1 maybe, 1 awaiting"
        );
    }

    #[test]
    fn the_guest_line_counts_who_said_yes() {
        let showing = showing(
            "REQUEST",
            &[
                "ATTENDEE;PARTSTAT=ACCEPTED;CN=Ann Lee:mailto:ann@example.com",
                "ATTENDEE;PARTSTAT=ACCEPTED;CN=Bo Chen:mailto:bo@example.com",
                "ATTENDEE;PARTSTAT=TENTATIVE;CN=Cal Diaz:mailto:cal@example.com",
                "ATTENDEE;PARTSTAT=NEEDS-ACTION;CN=Me:mailto:me@example.com",
            ],
        );
        assert_eq!(said_yes(&attending(&showing)), "2 of 4 said yes");
    }

    #[test]
    fn the_second_line_gives_the_time_then_the_place() {
        let mut event = showing("REQUEST", &[]).invitation;
        event.location = Some("Room 2.04".to_string());
        assert_eq!(joined(&[Some("Wednesday".to_string()), event.location.clone()]), "Wednesday · Room 2.04");
        assert_eq!(joined(&[Some("Wednesday".to_string()), None]), "Wednesday");
        assert_eq!(joined(&[None, Some(" ".to_string())]), "");
    }

    #[test]
    fn the_answers_run_yes_maybe_no_and_only_the_one_given_is_filled() {
        let order: Vec<Answer> = ANSWERS.to_vec();
        assert_eq!(order, [Answer::Yes, Answer::Maybe, Answer::No]);
        for answer in ANSWERS {
            assert!(!calendar_words::answer_filled(answer, None), "{answer:?} before any answer");
        }
        assert!(calendar_words::answer_filled(Answer::Maybe, Some(Answer::Maybe)));
        assert!(!calendar_words::answer_filled(Answer::Yes, Some(Answer::Maybe)));
    }

    #[test]
    fn the_news_line_says_what_the_message_did_to_the_event() {
        let now = Local::now();
        let mut showing = showing("REQUEST", &[]);
        assert_eq!(news(&showing, now), None);
        showing.change = Some(Change::Updated);
        assert_eq!(
            news(&showing, now).as_deref(),
            Some("The organizer changed this meeting")
        );
        assert_eq!(news_tone(&showing), "changed");
        showing.change = Some(Change::Moved {
            was: 1_717_924_800_000,
            all_day: false,
        });
        assert!(
            news(&showing, now).is_some_and(|line| line.starts_with("This meeting moved from "))
        );
        showing.change = Some(Change::Cancelled);
        assert_eq!(
            news(&showing, now).as_deref(),
            Some("The organizer canceled this meeting")
        );
        assert_eq!(news_tone(&showing), "cancelled");
    }

    #[test]
    fn a_cancellation_reads_as_one_without_a_change_behind_it() {
        let showing = showing("CANCEL", &[]);
        assert_eq!(
            news(&showing, Local::now()).as_deref(),
            Some("This meeting is canceled")
        );
        assert_eq!(news_tone(&showing), "cancelled");
    }

    #[test]
    fn the_times_offered_move_the_meeting_whole() {
        let starts_at = 1_717_924_800_000;
        let offered = nearby(starts_at);
        assert_eq!(offered.len(), 4);
        assert_eq!(offered[0].1, starts_at + 30 * 60 * 1_000);
        assert_eq!(offered[1].1, starts_at + 60 * 60 * 1_000);
        assert!(offered[2].1 > offered[1].1);
        assert!(offered[3].1 > offered[2].1);
        assert!(nearby(EpochMillis::MAX).is_empty());
    }

    #[test]
    fn an_event_becomes_a_readable_file_name() {
        assert_eq!(file_name("Q4 roadmap review"), "q4-roadmap-review.ics");
        assert_eq!(file_name("  "), "invitation.ics");
        assert_eq!(file_name("Budget review, Q1"), "budget-review--q1.ics");
        assert!(file_name(&"x".repeat(200)).len() <= 52);
    }

    #[test]
    fn a_proposal_keeps_the_meeting_length() {
        let mut event = showing("REQUEST", &[]).invitation;
        event.when = Some(When::At { starts_at: 0, ends_at: Some(30 * 60 * 1_000) });
        assert_eq!(
            moved(&event, 1_000),
            Some(When::At { starts_at: 1_000, ends_at: Some(1_000 + 30 * 60 * 1_000) })
        );
        event.when = Some(When::At { starts_at: 0, ends_at: None });
        assert_eq!(
            moved(&event, 0),
            Some(When::At { starts_at: 0, ends_at: Some(60 * 60 * 1_000) })
        );
    }
}
