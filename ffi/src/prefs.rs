//! Iris Next's Preferences: the same settings file the GTK app keeps,
//! read whole and changed one named change at a time, through
//! `mailrs_appcore::settings::Change`, so each choice lands as it does
//! from the GTK Preferences. Choices come with the GTK app's own words.

use mailrs_appcore::notify::Button;
use mailrs_appcore::settings::{
    Change, Choice, ColorScheme, ComposeFormat, MarkRead, RemoteImages, Settings, TextSize, UndoSend,
    cache_choices, nearest, poll_choices, window_choices,
};
use mailrs_domain::Category;
use mailrs_domain::calendar::hours::WorkingHours;
use mailrs_domain::calendar::week::WeekStart;
use mailrs_sync::config::{Config, config_path};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::CoreError;
use crate::mail::Mail;

/// One option of a choice: the key the settings file keeps, and its name.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ChoiceItem {
    pub key: String,
    pub label: String,
}

/// The preferences Iris Next shows, as they stand.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Preferences {
    pub threading: bool,
    pub inbox_categories: bool,
    pub suggest_follow_ups: bool,
    pub mark_read: String,
    pub remote_images: String,
    pub text_size: String,
    /// How much the conversation page is magnified for `text_size`.
    pub text_zoom: f64,
    pub color_scheme: String,
    pub notifications: bool,
    pub notification_previews: bool,
    pub notify_vips_only: bool,
    pub compose_format: String,
    pub undo_send: String,
    /// How long Undo Send waits, in seconds.
    pub undo_seconds: u32,
    pub check_attachments: bool,
    pub default_account: Option<String>,
    pub language: String,
    /// The inbox category the window opens on.
    pub default_category: String,
    /// The buttons a new-mail notification shows, by key.
    pub notification_buttons: Vec<String>,
    pub event_reminders: bool,
    pub week_start: String,
    pub working_hours: Hours,
}

/// The hours and days meetings usually run in. Hours are whole, the end
/// may be 24 for midnight at the day's end; the days run Monday first.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Hours {
    pub start: u32,
    pub end: u32,
    pub days: Vec<bool>,
}

/// How the sync engine runs, from `config.toml`, each value one of its
/// choices.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SyncSettings {
    pub poll_seconds: i64,
    pub window_days: i64,
    pub cache_mb: i64,
}

/// One option of a numeric choice and its name.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NumberChoice {
    pub value: i64,
    pub label: String,
}

/// A sender whose remote images load without asking.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ImageSender {
    /// An address, or a domain when `whole_domain`.
    pub sender: String,
    pub whole_domain: bool,
}

/// The options each choice offers, in the GTK app's order and words.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PreferenceChoices {
    pub mark_read: Vec<ChoiceItem>,
    pub remote_images: Vec<ChoiceItem>,
    pub text_size: Vec<ChoiceItem>,
    pub color_scheme: Vec<ChoiceItem>,
    pub compose_format: Vec<ChoiceItem>,
    pub undo_send: Vec<ChoiceItem>,
    pub default_category: Vec<ChoiceItem>,
    pub week_start: Vec<ChoiceItem>,
    pub notification_buttons: Vec<ChoiceItem>,
    pub poll: Vec<NumberChoice>,
    pub window: Vec<NumberChoice>,
    pub cache: Vec<NumberChoice>,
}

/// One address's signature: Markdown lines, or a formatted one in HTML.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SignatureSetting {
    pub account_id: i64,
    pub email: String,
    pub markdown: String,
    pub formatted: Option<String>,
}

#[uniffi::export]
impl Mail {
    pub fn preferences(&self) -> Preferences {
        let s = load();
        Preferences {
            threading: s.threading,
            inbox_categories: s.inbox_categories,
            suggest_follow_ups: s.suggest_follow_ups,
            mark_read: key(&s.mark_read),
            remote_images: key(&s.remote_images),
            text_size: key(&s.text_size),
            text_zoom: s.text_size.zoom(),
            color_scheme: key(&s.color_scheme),
            notifications: s.notifications,
            notification_previews: s.notification_previews,
            notify_vips_only: s.notify_vips_only,
            compose_format: key(&s.compose_format),
            undo_send: key(&s.undo_send),
            undo_seconds: s.undo_send.seconds(),
            check_attachments: s.check_attachments,
            default_account: s.default_account.clone(),
            language: s.language.clone(),
            default_category: key(&s.default_category),
            notification_buttons: s.notification_buttons.iter().map(key).collect(),
            event_reminders: s.event_reminders,
            week_start: key(&s.week_start),
            working_hours: Hours {
                start: u32::from(s.working_hours.start_minutes / 60),
                end: u32::from(s.working_hours.end_minutes / 60),
                days: s.working_hours.days.to_vec(),
            },
        }
    }

    pub fn preference_choices(&self) -> PreferenceChoices {
        PreferenceChoices {
            mark_read: choices::<MarkRead>(),
            remote_images: choices::<RemoteImages>(),
            text_size: choices::<TextSize>(),
            color_scheme: choices::<ColorScheme>(),
            compose_format: choices::<ComposeFormat>(),
            undo_send: choices::<UndoSend>(),
            default_category: choices::<Category>(),
            week_start: choices::<WeekStart>(),
            notification_buttons: Button::ALL
                .iter()
                .map(|button| ChoiceItem {
                    key: key(button),
                    label: button.label(),
                })
                .collect(),
            poll: numbers(poll_choices()),
            window: numbers(window_choices()),
            cache: numbers(cache_choices()),
        }
    }

    /// Changes one preference by its name in `Preferences`, to `value`:
    /// `true` or `false`, a choice's key, or an address.
    pub fn set_preference(&self, name: String, value: String) -> Result<(), CoreError> {
        let on = || value == "true";
        let change = match name.as_str() {
            "threading" => Change::Threading(on()),
            "inbox_categories" => Change::InboxCategories(on()),
            "suggest_follow_ups" => Change::SuggestFollowUps(on()),
            "mark_read" => Change::MarkRead(parse(&value)?),
            "remote_images" => Change::RemoteImages(parse(&value)?),
            "text_size" => Change::TextSize(parse(&value)?),
            "color_scheme" => Change::ColorScheme(parse(&value)?),
            "notifications" => Change::Notifications(on()),
            "notification_previews" => Change::NotificationPreviews(on()),
            "notify_vips_only" => Change::NotifyVipsOnly(on()),
            "compose_format" => Change::ComposeFormat(parse(&value)?),
            "undo_send" => Change::UndoSend(parse(&value)?),
            "check_attachments" => Change::CheckAttachments(on()),
            "default_account" => Change::DefaultAccount(Some(value).filter(|v| !v.is_empty())),
            "language" => Change::Language(value),
            "default_category" => Change::DefaultCategory(parse(&value)?),
            "event_reminders" => Change::EventReminders(on()),
            "week_start" => Change::WeekStart(parse(&value)?),
            button if button.starts_with("notification_button:") => Change::NotificationButton {
                button: parse(&button["notification_button:".len()..])?,
                show: on(),
            },
            other => return Err(CoreError::Store(format!("{other} is not a preference"))),
        };
        save(change)
    }

    /// Keeps the working hours. An end at or before the start moves to an
    /// hour after it, so the working day never runs backwards.
    pub fn set_working_hours(&self, hours: Hours) -> Result<(), CoreError> {
        let start = hours.start.min(23);
        let end = hours.end.clamp(1, 24).max(start + 1);
        let mut days = [false; 7];
        for (day, on) in days.iter_mut().zip(hours.days) {
            *day = on;
        }
        save(Change::WorkingHours(WorkingHours {
            start_minutes: u16::try_from(start * 60).unwrap_or(0),
            end_minutes: u16::try_from(end * 60).unwrap_or(24 * 60),
            days,
        }))
    }

    /// The sync section of `config.toml`, each value at its nearest choice.
    pub fn sync_settings(&self) -> SyncSettings {
        let sync = read_config().map(|config| config.sync).unwrap_or_default();
        let pick = |choices: Vec<(i64, String)>, value: i64| {
            choices
                .get(nearest(&choices, value) as usize)
                .map_or(value, |(v, _)| *v)
        };
        SyncSettings {
            poll_seconds: pick(poll_choices(), sync.poll_seconds.map_or(30, |s| s as i64)),
            window_days: pick(window_choices(), sync.window_days.unwrap_or(30)),
            cache_mb: pick(cache_choices(), sync.body_cache_mb.unwrap_or(1024)),
        }
    }

    /// Saves new sync settings and starts syncing again with them, as
    /// the GTK app does when its Preferences close.
    pub fn set_sync_settings(&self, settings: SyncSettings) -> Result<(), CoreError> {
        let path = config_path().map_err(|err| CoreError::Store(err.to_string()))?;
        let mut config = read_config()?;
        let sync = mailrs_sync::config::SyncConfig {
            poll_seconds: u64::try_from(settings.poll_seconds).ok(),
            window_days: Some(settings.window_days),
            body_cache_mb: Some(settings.cache_mb),
        };
        if config.sync == sync {
            return Ok(());
        }
        config.sync = sync;
        config.save(&path).map_err(|err| CoreError::Store(err.to_string()))?;
        self.restart_sync()
    }

    /// The senders whose remote images load without asking, newest first.
    pub fn image_senders(&self) -> Result<Vec<ImageSender>, CoreError> {
        let db = self.db.clone();
        let list = self.run(async move { db.read(mailrs_store::image_senders::list).await })?;
        Ok(list
            .into_iter()
            .map(|entry| ImageSender {
                sender: entry.sender,
                whole_domain: entry.whole_domain,
            })
            .collect())
    }

    /// Takes `sender` off the list, so its images wait to be asked for.
    pub fn forget_image_sender(&self, sender: String) -> Result<(), CoreError> {
        let db = self.db.clone();
        self.run(async move { db.write(move |c| mailrs_store::image_senders::forget(c, &sender)).await })?;
        Ok(())
    }

    /// The signature Gmail keeps for the account's own address, as plain
    /// text, or `None` when it has none. Needs the account to be syncing.
    pub fn gmail_signature(&self, account_id: i64) -> Result<Option<String>, CoreError> {
        let settings = mailrs_sync::AccountSettings::new(std::sync::Arc::clone(&self.running), self.db.clone());
        self.run(async move { settings.signature(account_id).await })
            .map_err(|err| CoreError::Store(err.to_string()))
    }

    /// Every account address's signature.
    pub fn signatures(&self) -> Result<Vec<SignatureSetting>, CoreError> {
        let s = load();
        Ok(self
            .senders()?
            .into_iter()
            .map(|sender| SignatureSetting {
                markdown: s.signature(&sender.email).to_string(),
                formatted: Some(s.formatted_signature(&sender.email).to_string()).filter(|h| !h.is_empty()),
                account_id: sender.account_id,
                email: sender.email,
            })
            .collect())
    }

    /// Keeps a Markdown signature for `email`; empty text removes it.
    pub fn set_signature(&self, email: String, text: String) -> Result<(), CoreError> {
        save(Change::Signature { email, text })
    }

    /// Keeps a formatted signature for `email`, cleaned; HTML that shows
    /// nothing removes it.
    pub fn set_formatted_signature(&self, email: String, html: String) -> Result<(), CoreError> {
        save(Change::FormattedSignature { email, html })
    }
}

/// A formatted signature from an `.htm` file, its pictures beside it read
/// in, cleaned, as the GTK app imports one.
#[uniffi::export]
pub fn import_signature(path: String) -> Result<String, CoreError> {
    let path = std::path::PathBuf::from(path);
    let bytes = std::fs::read(&path).map_err(|err| CoreError::Store(err.to_string()))?;
    let html = mailrs_appcore::signature::decode(&bytes);
    let dir = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    Ok(mailrs_appcore::signature::import(&html, &dir))
}

/// The language the person picked in Preferences, or empty to follow
/// the system. Read before the mail store opens, to bind the catalogs.
#[uniffi::export]
pub fn chosen_language() -> String {
    load().language
}

/// Whether Preferences say to load remote images in every conversation.
pub(crate) fn loads_remote_images() -> bool {
    load().remote_images == RemoteImages::Always
}

fn read_config() -> Result<Config, CoreError> {
    let path = config_path().map_err(|err| CoreError::Store(err.to_string()))?;
    match Config::load(&path) {
        Ok(config) => Ok(config),
        Err(err) if err.is_missing() => Ok(Config::default()),
        Err(err) => Err(CoreError::Store(err.to_string())),
    }
}

fn numbers(choices: Vec<(i64, String)>) -> Vec<NumberChoice> {
    choices
        .into_iter()
        .map(|(value, label)| NumberChoice { value, label })
        .collect()
}

fn load() -> Settings {
    Settings::load(&Settings::default_path())
}

fn save(change: Change) -> Result<(), CoreError> {
    let path = Settings::default_path();
    let mut settings = Settings::load(&path);
    change.apply_to(&mut settings);
    settings.save(&path).map_err(|err| CoreError::Store(err.to_string()))
}

/// The key the settings file keeps a choice under.
fn key<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn parse<T: DeserializeOwned>(key: &str) -> Result<T, CoreError> {
    serde_json::from_value(serde_json::Value::String(key.to_string()))
        .map_err(|_| CoreError::Store(format!("{key} is not one of the choices")))
}

fn choices<T: Choice + Serialize>() -> Vec<ChoiceItem> {
    T::ALL
        .iter()
        .map(|choice| ChoiceItem {
            key: key(choice),
            label: choice.label(),
        })
        .collect()
}
