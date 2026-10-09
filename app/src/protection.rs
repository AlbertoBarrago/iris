//! The engines' side of protection, which runs through the app's
//! [`Core`]: asking gpg and gpgsm what they hold. The rest, which only
//! reads and builds MIME, lives in `mailrs_appcore::protection`.

pub mod draft;
pub mod run;

pub use mailrs_appcore::protection::*;

use crate::core::Core;

/// What each engine holds for `addresses`. An engine this computer does
/// not have is asked nothing, and one that would not answer is left out
/// the same way, since a question nobody answered is not a missing key.
/// The composer asks this as the recipients change, and the assistant
/// asks it before a message goes out encrypted.
pub async fn held(core: &Core, addresses: &[String]) -> Held {
    let mut held = Held::default();
    if core.has_gpg() {
        let wanted = addresses.to_vec();
        match core.gpg(move |pgp| pgp.keys_for(&wanted)).await {
            Ok(keys) => held.pgp = Some(keys),
            Err(err) => tracing::info!(error = %err, "could not ask gpg about the addresses"),
        }
    }
    if core.has_gpgsm() {
        let wanted = addresses.to_vec();
        match core
            .gpgsm(move |smime| smime.certificates_for(&wanted))
            .await
        {
            Ok(certificates) => held.smime = Some(certificates),
            Err(err) => tracing::info!(error = %err, "could not ask gpgsm about the addresses"),
        }
    }
    held
}

/// Which standard signs a message from `from`, going by what both engines
/// hold of the sender's own.
pub async fn signing_for(core: &Core, from: &str) -> Standard {
    let from = vec![from.trim().to_lowercase()];
    let mut held = held(core, &from).await;
    // Signing needs a secret key. Only gpgsm is asked for one outright:
    // gpg lists a key of the person's own whichever half the question was
    // about.
    if core.has_gpgsm() {
        held.smime = core
            .gpgsm(move |smime| smime.signing_certificates(&from))
            .await
            .ok();
    }
    signing(&held)
}

