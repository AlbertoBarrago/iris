//! A guest's Yes, Maybe or No from the calendar, for the Swift front end:
//! the same question the GTK calendar asks of a repeating event first,
//! then the answer through sync's `Invitations`, which queues it on the
//! calendar as every calendar write is queued.

use std::sync::Arc;

use mailrs_domain::calendar::series::{self, RepeatScope};
use mailrs_domain::invitation::Answer;
use mailrs_domain::translate::{fill, gettext};
use mailrs_sync::Permitted;
use mailrs_sync::calendar_copy::event_change::{self, Action, Facts};
use mailrs_sync::invitations::Invitations;

use crate::CoreError;
use crate::calendar::shown_title;
use crate::event_editor::{EventQuestion, question_record, scope_of};
use crate::mail::Mail;

#[uniffi::export]
impl Mail {
    /// What to ask before answering the occurrence `id` starting at
    /// `starts`: whether the answer covers that event or the whole series.
    /// `None` for a one-off event, which needs no question.
    pub fn answer_question(&self, id: String, starts: i64) -> Result<Option<EventQuestion>, CoreError> {
        let Some(occurrence) = self.occurrence(&id, starts)? else { return Ok(None) };
        let offered = series::answer_scopes(&occurrence.event);
        Ok(event_change::question(Action::Answer, &offered, &[], Facts::default())
            .map(|question| question_record(&question, &shown_title(&occurrence), None)))
    }

    /// Sends the person's `answer` (`yes`, `maybe` or `no`) to the
    /// occurrence `id`, for the occurrences `scope` names (the whole
    /// series when none), with `note` for the organizer. The answer is
    /// queued, so it shows at once and goes out now or once the network
    /// is back.
    pub fn answer_event(
        &self,
        id: String,
        starts: i64,
        answer: String,
        scope: Option<String>,
        note: String,
    ) -> Result<(), CoreError> {
        let Ok(answer) = answer.parse::<Answer>() else {
            return Err(CoreError::Store(format!("not an answer: {answer}")));
        };
        let Some(occurrence) = self.occurrence(&id, starts)? else {
            return Err(CoreError::Store(gettext("The event is no longer on the calendar")));
        };
        let offered = series::answer_scopes(&occurrence.event);
        let scope = event_change::question(Action::Answer, &offered, &[], Facts::default())
            .zip(scope)
            .and_then(|(question, id)| scope_of(&question, &id))
            .unwrap_or(RepeatScope::All);
        let note = Some(note.trim().to_string()).filter(|n| !n.is_empty());
        let account = occurrence.account_id;
        let invitations = Invitations::new(Arc::clone(&self.running), self.db.clone(), Arc::clone(&self.calendar));
        let sent = self.run(async move { invitations.answer_event(account, &occurrence, answer, scope, note).await });
        match sent {
            Ok(Permitted::Done(())) => {
                self.changed("calendar");
                self.send_calendar(account);
                Ok(())
            }
            Ok(Permitted::NeedsPermission) => {
                Err(CoreError::Store(gettext("Iris has no permission to use this calendar. Allow it in Preferences.")))
            }
            Err(err) => Err(CoreError::Store(fill(&gettext("Could not send your answer: {reason}"), &[("reason", &err.to_string())]))),
        }
    }
}

