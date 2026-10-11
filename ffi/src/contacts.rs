//! The people behind the addresses, for the Swift front end: the card a
//! click on a sender opens, from the address book the accounts keep, and
//! the VIPs Preferences holds, as the GTK window has them.

use std::path::PathBuf;
use std::sync::Arc;

use mailrs_appcore::contacts::{failed_title, offer_title, saved_title, vip_message};
use mailrs_domain::Address;
use mailrs_appcore::offered::{ContactsRow, contacts_row};
use mailrs_appcore::settings::Change;
use mailrs_domain::translate::gettext;
use mailrs_store::accounts;
use mailrs_sync::{Accounts, ContactBook, Offers, Permitted, SyncEngine, Withheld};

use crate::CoreError;
use crate::mail::{Mail, MailListener, Running};
use crate::prefs;

/// The question after sending: save the new recipients to the account's
/// contacts?
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SaveOffer {
    pub account_id: i64,
    /// "Save Ann Lee to contacts in me@example.com?"
    pub title: String,
    pub people: Vec<OfferedPerson>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OfferedPerson {
    pub name: Option<String>,
    pub email: String,
}

/// One account's line in Settings, Contacts: a switch, a Grant Access
/// row, or a line saying why the server keeps no contacts.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ContactAccount {
    pub account_id: i64,
    pub email: String,
    /// `switch`, `withheld` or `none`.
    pub kind: String,
    /// Whether Iris reads the account's contacts, for a switch.
    pub on: bool,
    /// Why there is no switch: the server's lack, or "Not allowed when
    /// you signed in".
    pub reason: String,
}

/// One person as the contact card shows them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ContactCard {
    pub name: String,
    /// The address the card was opened on, or the contact's first.
    pub email: String,
    /// Every address the contact has, the first being their own.
    pub addresses: Vec<String>,
    pub organization: String,
    pub phone: String,
    /// The contact's photo on disk, or empty for initials.
    pub photo: String,
    pub vip: bool,
}

#[uniffi::export]
impl Mail {
    /// The card for `address`: what the address book knows, or `name`
    /// from the message header alone when it has never heard of them.
    pub fn contact_card(&self, address: String, name: String) -> Result<ContactCard, CoreError> {
        let address = address.trim().to_string();
        let book = self.contact_book();
        let asked = address.clone();
        let found = self.run(async move { book.card(&asked).await }).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "could not read the contact");
            None
        });
        let vip = prefs::load().is_vip(&address);
        Ok(match found {
            Some(card) => ContactCard {
                name: card.contact.display().to_string(),
                email: card.contact.email().unwrap_or(address.as_str()).to_string(),
                addresses: card.contact.emails.clone(),
                organization: card.contact.organization.clone().unwrap_or_default(),
                phone: card.contact.phone.clone().unwrap_or_default(),
                photo: card.photo.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
                vip,
            },
            None => ContactCard {
                name: Some(name).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| address.clone()),
                email: address.clone(),
                addresses: vec![address],
                organization: String::new(),
                phone: String::new(),
                photo: String::new(),
                vip,
            },
        })
    }

    /// Adds `email` to the VIPs under `name`, or takes it off, and says
    /// which, for a toast.
    pub fn toggle_vip(&self, email: String, name: String) -> Result<String, CoreError> {
        prefs::save(Change::ToggleVip {
            email: email.clone(),
            name: name.clone(),
        })?;
        let added = prefs::load().is_vip(&email);
        self.changed("sidebar");
        Ok(vip_message(added, &name))
    }
}

#[uniffi::export]
impl Mail {
    /// Each account's contacts line for Settings, as the GTK Preferences
    /// draws it. An account not connected yet shows its switch.
    pub fn contact_accounts(&self) -> Result<Vec<ContactAccount>, CoreError> {
        let db = self.db.clone();
        let listed = self.run(async move { db.read(accounts::list_accounts).await })?;
        let settings = prefs::load();
        Ok(listed
            .into_iter()
            .map(|account| {
                let services = self.running.services(account.id);
                let offers = services.as_ref().map_or(Offers::EVERYTHING, |s| s.offers());
                let withheld = services.as_ref().map_or(Withheld::NONE, |s| s.withheld());
                let (kind, reason) = match contacts_row(&account, offers, withheld, None) {
                    ContactsRow::Switch => ("switch", String::new()),
                    ContactsRow::Withheld => ("withheld", gettext("Not allowed when you signed in")),
                    ContactsRow::NotOffered(reason) => ("none", reason),
                };
                ContactAccount {
                    account_id: account.id,
                    on: settings.reads_contacts(&account.email),
                    email: account.email,
                    kind: kind.into(),
                    reason,
                }
            })
            .collect())
    }

    /// Turns reading account `account_id`'s contacts on or off. On reads
    /// the address book now; off deletes what was kept of it, photos and
    /// all, as the GTK switch does.
    pub fn set_account_contacts(&self, account_id: i64, email: String, on: bool) -> Result<(), CoreError> {
        prefs::save(Change::AccountContacts { email, on })?;
        let book = self.contact_book();
        let done = match on {
            true => self.run(async move { book.refresh(account_id).await.map(|_| ()) }),
            false => self.run(async move { book.forget(account_id).await }),
        };
        if let Err(err) = done {
            tracing::warn!(error = %err, "could not read or forget the address book");
        }
        self.contacts_changed();
        Ok(())
    }
}

#[uniffi::export]
impl Mail {
    /// What to offer after a message from account `account_id` went to
    /// `to`, `cc` and `bcc` (the recipient fields' text): the people the
    /// address book lacks, who are not automated senders and were not
    /// offered before. `None` when the account does not read its
    /// contacts, or nobody is new.
    pub fn save_offer(&self, account_id: i64, to: String, cc: String, bcc: String) -> Option<SaveOffer> {
        let account = self.account_row(account_id).ok()?;
        if !prefs::load().reads_contacts(&account.email) {
            return None;
        }
        let sent: Vec<Address> = [to, cc, bcc]
            .iter()
            .flat_map(|field| mailrs_appcore::compose::parse_recipients(field))
            .filter(|a| !a.email.trim().is_empty())
            .collect();
        let book = self.contact_book();
        let new = self
            .run(async move { book.new_recipients(account_id, &sent).await })
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "could not read who is new");
                Vec::new()
            });
        if new.is_empty() {
            return None;
        }
        Some(SaveOffer {
            account_id,
            title: offer_title(&new, &account.email),
            people: new
                .into_iter()
                .map(|a| OfferedPerson {
                    name: a.name,
                    email: a.email,
                })
                .collect(),
        })
    }

    /// Saves the people `offer` named to its account's contacts, and says
    /// how it went.
    pub fn save_offered(&self, offer: SaveOffer) -> Result<String, CoreError> {
        let account = self.account_row(offer.account_id)?;
        let people: Vec<Address> = offer
            .people
            .into_iter()
            .map(|p| Address {
                name: p.name,
                email: p.email,
            })
            .collect();
        let (book, asked, account_id) = (self.contact_book(), people.clone(), offer.account_id);
        let done = self
            .run(async move { book.save_recipients(account_id, &asked).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let saved = match done {
            Permitted::Done(saved) => saved,
            Permitted::NeedsPermission => {
                // The account's sign-in left contacts read-only; Settings,
                // Contacts offers Grant Access for it.
                return Err(CoreError::Store(gettext("Allow Iris to Change Your Contacts")));
            }
        };
        self.contacts_changed();
        let saved_people: Vec<Address> = people
            .iter()
            .filter(|p| saved.saved.iter().any(|c| c.emails.iter().any(|e| e.eq_ignore_ascii_case(&p.email))))
            .cloned()
            .collect();
        Ok(match saved.failed.is_empty() {
            true => saved_title(&saved_people, &account.email),
            false => failed_title(&saved.failed, &account.email),
        })
    }

    /// The person closed the offer: these people are not offered again.
    pub fn decline_offered(&self, offer: SaveOffer) {
        let emails: Vec<String> = offer.people.into_iter().map(|p| p.email).collect();
        let (book, account_id) = (self.contact_book(), offer.account_id);
        if let Err(err) = self.run(async move { book.decline_recipients(account_id, &emails).await }) {
            tracing::warn!(error = %err, "could not keep the declined offer");
        }
    }
}

impl Mail {
    /// The address book changed: suggestions read it again, and the
    /// window draws what shows it.
    pub(crate) fn contacts_changed(&self) {
        *self.people.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.changed("contacts");
    }

    /// The address book over every account, with its photos in the cache
    /// the GTK app keeps them in.
    pub(crate) fn contact_book(&self) -> Arc<ContactBook<Running>> {
        Arc::new(ContactBook::new(Arc::clone(&self.running), self.db.clone(), photo_dir()))
    }
}

/// How often the address books are looked at; `ContactBook` reads only
/// the ones older than six hours, as the GTK app's hourly timer does.
const REFRESH_EVERY: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Keeps the address books of the accounts that read their contacts
/// fresh while `engine` runs. The first look waits a minute, for the
/// accounts to connect.
pub(crate) async fn keep_fresh(
    book: Arc<ContactBook<Running>>,
    running: Arc<Running>,
    engine: Arc<SyncEngine>,
    db: mailrs_store::Db,
    listener: Arc<dyn MailListener>,
    people: Arc<std::sync::Mutex<Option<Arc<Vec<mailrs_store::contacts::Suggestion>>>>>,
) {
    let start = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut every = tokio::time::interval_at(start, REFRESH_EVERY);
    loop {
        every.tick().await;
        if !running.current().is_some_and(|now| Arc::ptr_eq(&now, &engine)) {
            return;
        }
        let settings = prefs::load();
        let Ok(listed) = db.read(accounts::list_accounts).await else { continue };
        let ids: Vec<i64> = listed
            .into_iter()
            .filter(|a| settings.reads_contacts(&a.email))
            .map(|a| a.id)
            .collect();
        if ids.is_empty() {
            continue;
        }
        match book.refresh_stale(&ids, mailrs_sync::now_millis()).await {
            Ok(read) if read.contacts > 0 || read.photos > 0 => {
                *people.lock().unwrap_or_else(|p| p.into_inner()) = None;
                listener.changed("contacts".into());
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(%err, "could not read the address books"),
        }
    }
}

/// Where contact photos are kept.
pub(crate) fn photo_dir() -> PathBuf {
    mailrs_appcore::dirs::user_cache_dir()
        .join(mailrs_sync::config::DIR_NAME)
        .join("contact-photos")
}
