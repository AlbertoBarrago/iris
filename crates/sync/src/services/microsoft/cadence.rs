//! How often a Microsoft account looks at its mail: the Inbox every 30
//! seconds while the window is open, every other synced folder every 5
//! minutes, and everything every 15 minutes while only the tray runs.
//! Graph cannot push to the app, so the looks are all there is.
//!
//! Graph takes a message from `sendMail` at once but files it in Sent
//! Items a few seconds later, so the look a send triggers usually finds
//! Sent still empty. After a send every folder is read at each look, and
//! the looks come every five seconds, until Sent shows something new or
//! two minutes pass.

use std::time::Duration;

use tokio::time::Instant;

use super::{GraphApi, Microsoft};
use mailrs_domain::Role;

const INBOX_EVERY: Duration = Duration::from_secs(30);
const OTHERS_EVERY: Duration = Duration::from_secs(5 * 60);
const TRAY_EVERY: Duration = Duration::from_secs(15 * 60);
/// How often the feed is read while a sent message waits for its copy.
const SENT_EVERY: Duration = Duration::from_secs(5);
/// How long a send keeps the looks quick. A mailbox that never files
/// what it sends goes back to its usual pace after this.
const SENT_WAIT: Duration = Duration::from_secs(2 * 60);

impl<G: GraphApi> Microsoft<G> {
    pub(super) fn poll_every(&self) -> Duration {
        let known = self.known();
        if awaiting(&known.awaiting_sent) {
            return SENT_EVERY;
        }
        match known.window_open {
            true => INBOX_EVERY,
            false => TRAY_EVERY,
        }
    }

    /// Notes that a message went out, whose copy Graph has yet to file.
    pub(super) fn await_sent(&self) {
        self.known().awaiting_sent = Some(Instant::now() + SENT_WAIT);
    }

    /// Ends the quick looks once a look has found something new in Sent.
    pub(super) fn sent_filed(&self) {
        self.known().awaiting_sent = None;
    }

    /// The folders this look covers, and whether the slow poll is due: the
    /// Inbox each time, the rest when the slow poll comes round, and all of
    /// them at every look while only the tray runs or a sent message
    /// waits for its copy.
    pub(super) fn due(&self, synced: Vec<String>) -> (Vec<String>, bool) {
        let known = self.known();
        let slow = !known.window_open
            || awaiting(&known.awaiting_sent)
            || known.last_slow.is_none_or(|at| at.elapsed() >= OTHERS_EVERY);
        let inbox = known.roles.get(&Role::Inbox).cloned();
        let due = match slow {
            true => synced,
            false => synced.into_iter().filter(|f| Some(f) == inbox.as_ref()).collect(),
        };
        (due, slow)
    }

    pub(super) fn slow_poll_done(&self) {
        self.known().last_slow = Some(Instant::now());
    }
}

/// Whether a send still keeps the looks quick.
fn awaiting(until: &Option<Instant>) -> bool {
    until.is_some_and(|until| Instant::now() < until)
}
