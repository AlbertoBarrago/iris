//! Saving and opening encrypted drafts through the app's [`Core`], over
//! the MIME `mailrs_appcore::protection::draft` builds and reads.

use mailrs_domain::MessageBody;
use mailrs_domain::translate::{fill, gettext, with_reason};
use mailrs_pgp::PgpError;
use mailrs_smime::SmimeError;
use mailrs_sync::{SavedDraft, now_millis};

pub use mailrs_appcore::protection::draft::*;

use super::Standard;
use crate::compose::{self, Draft};
use crate::core::Core;

/// What Gmail keeps for `draft` while the writer means it to go out
/// encrypted. `preferred` is the standard to try first; the other one gets
/// a turn when the first holds nothing of the writer's own. The answer is
/// an error to show, in the writer's language, when neither can do it.
pub async fn sealed(
    core: &Core,
    draft: &Draft,
    preferred: Standard,
    date_secs: i64,
    message_id: &str,
) -> Result<Vec<u8>, String> {
    let failed = |reason: &str| fill(&gettext("Draft not saved: {reason}"), &[("reason", reason)]);
    let part = compose::build_saved_body_part(draft).map_err(|err| failed(&err))?;
    let order = match preferred {
        Standard::Pgp => [Standard::Pgp, Standard::Smime],
        Standard::Smime => [Standard::Smime, Standard::Pgp],
    };
    for standard in order {
        let (part, from) = (part.clone(), draft.from.email.clone());
        let entity = match standard {
            Standard::Pgp if core.has_gpg() => {
                core.gpg(move |pgp| for_writer_pgp(pgp, &part, &from)).await
            }
            Standard::Smime if core.has_gpgsm() => {
                core.gpgsm(move |smime| for_writer_smime(smime, &part, &from))
                    .await
            }
            _ => continue,
        };
        match entity {
            Ok(Some(entity)) => {
                return build(draft, date_secs, message_id, entity).map_err(|err| failed(&err));
            }
            Ok(None) => continue,
            Err(err) => return Err(failed(&err.to_string())),
        }
    }
    Err(fill(
        &gettext(
            "Draft not saved. An encrypted message waits in Drafts encrypted to your own key, and \
             this computer holds no key or certificate for {address}.",
        ),
        &[("address", &draft.from.email)],
    ))
}

/// Saves `draft` into Gmail's Drafts and gives back where Gmail keeps it.
/// With `secret`, the body goes in encrypted to the writer, trying
/// `standard` first, as [`sealed`] says; otherwise it goes in readable.
/// The composer's Save Draft and the assistant's edit of a draft both come
/// through here. The error is what to tell the writer.
pub async fn save(
    core: &Core,
    draft: &Draft,
    secret: bool,
    standard: Standard,
) -> Result<SavedDraft, String> {
    let Some(account) = core.account(draft.account_id) else {
        return Err(gettext("That account is not connected."));
    };
    let (date, message_id) = (
        now_millis() / 1000,
        compose::new_message_id(&draft.from.email),
    );
    let raw = match secret {
        true => {
            let mut kept = draft.clone();
            kept.encrypt = true;
            sealed(core, &kept, standard, date, &message_id).await?
        }
        false => compose::build_saved_draft(draft, date, &message_id)
            .map_err(|err| fill(&gettext("Could not save: {reason}"), &[("reason", &err)]))?,
    };
    let (thread, draft_id) = (draft.thread_id.clone(), draft.draft_id.clone());
    let saved = core
        .call(async move { account.save_draft(raw, thread, draft_id).await })
        .await
        .map_err(|err| with_reason(&gettext("Draft not saved: {reason}"), &err, &[]))?;
    // A Send Later message waiting on this draft now names its new message.
    let (outbox, account_id, kept) = (core.outbox(), draft.account_id, saved.clone());
    core.spawn(async move {
        if let Err(err) = outbox.draft_saved(account_id, kept).await {
            tracing::warn!(error = %err, "could not update the store");
        }
    });
    core.poke(draft.account_id);
    Ok(saved)
}

/// Opens the draft `raw`, encrypted under `standard`, and fills `draft`
/// from it. The engine may ask for a passphrase, as it does for any
/// encrypted message. The error is what to tell the writer when it would
/// not open.
async fn opened(
    core: &Core,
    raw: Vec<u8>,
    standard: Standard,
    draft: &mut Draft,
) -> Result<(), String> {
    let unopened = |reason: &str| {
        fill(
            &gettext("Could not open the draft: {reason}"),
            &[("reason", reason)],
        )
    };
    let ciphertext = raw.clone();
    let read = match standard {
        Standard::Pgp => {
            core.gpg(move |pgp| {
                Ok::<_, PgpError>(crate::pgp::read(
                    pgp,
                    crate::pgp::Opening::Decrypt,
                    &ciphertext,
                    &MessageBody::default(),
                ))
            })
            .await
        }
        Standard::Smime => {
            core.gpgsm(move |smime| {
                Ok::<_, SmimeError>(crate::smime::read(
                    smime,
                    crate::smime::Opening::Decrypt,
                    &ciphertext,
                    &MessageBody::default(),
                ))
            })
            .await
        }
    }
    .map_err(|err| unopened(&err.to_string()))?;
    reopen(&raw, standard, read, draft).map_err(|reason| unopened(&reason))
}

/// Opens the draft `raw` whichever way Gmail holds it, and fills `draft`
/// from it. The error is what to tell the writer when it would not open.
pub async fn reopened(core: &Core, raw: Vec<u8>, draft: &mut Draft) -> Result<(), String> {
    match standard_of(&raw) {
        Some(standard) => opened(core, raw, standard, draft).await,
        None => {
            reopen_plain(&raw, draft);
            Ok(())
        }
    }
}

