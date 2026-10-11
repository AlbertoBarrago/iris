//! The words and rules of the automatic reply form, shared by the GTK
//! dialog and Iris Next's sheet. `mailrs_sync::AccountSettings` reads and
//! stores the reply itself.

use mailrs_domain::Provider;
use mailrs_domain::translate::{fill, gettext};

/// The switch's subtitle: who answers while the person is away. An IMAP
/// account with no provider name has only "IMAP" to call its server, which
/// reads badly as a subject, so it says "Your mail server".
pub fn away_subtitle(provider_name: &str) -> String {
    let who = if provider_name == Provider::Imap.name() {
        gettext("Your mail server")
    } else {
        provider_name.to_string()
    };
    fill(
        &gettext("{provider} answers new mail while you are away, even when this computer is off"),
        &[("provider", &who)],
    )
}

/// The subject a save sends: what was typed when the row shows, and what
/// the reply already held when it does not, so a hidden row never blanks it.
pub fn subject_to_save(shown: bool, typed: &str, kept: &str) -> String {
    if shown { typed.to_string() } else { kept.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subtitle_names_the_provider_that_answers() {
        let off = "while you are away, even when this computer is off";
        assert_eq!(away_subtitle("Gmail"), format!("Gmail answers new mail {off}"));
        assert_eq!(away_subtitle("Outlook"), format!("Outlook answers new mail {off}"));
        assert_eq!(away_subtitle("Fastmail"), format!("Fastmail answers new mail {off}"));
    }

    #[test]
    fn an_imap_account_with_no_provider_name_says_its_mail_server_answers() {
        assert_eq!(
            away_subtitle(Provider::Imap.name()),
            "Your mail server answers new mail while you are away, even when this computer is off"
        );
    }

    #[test]
    fn a_hidden_subject_row_leaves_the_subject_as_it_was() {
        assert_eq!(subject_to_save(false, "", "Away"), "Away");
        assert_eq!(subject_to_save(true, "Back soon", "Away"), "Back soon");
    }
}
