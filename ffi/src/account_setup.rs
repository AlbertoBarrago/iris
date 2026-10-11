//! Adding and removing accounts in Iris. `AccountSetup` walks the
//! Add Account steps the GTK dialog walks, with its words and decisions
//! from `mailrs_appcore::add_account`, and signs in through
//! `mailrs_appcore::accounts`. A new account starts on the running engine
//! at once, as in the GTK app.

use std::sync::{Arc, Mutex};

use mailrs_appcore::accounts as shared;
use mailrs_appcore::add_account::lookup::{Check, Checks, State as CheckState};
use mailrs_appcore::add_account::post::{self, Browser, BrowserWords, Stamp, Step, Tile};
use mailrs_appcore::add_account::{
    self, Address, Continue, FailureKind, Next, Outcome, Proposal, Protocol, Role, Typed,
};
use mailrs_appcore::settings::{Change, Settings};
use mailrs_discover::{Security, Server};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{Account, Provider, RemoveSetting};
use mailrs_store::{accounts, servers};
use mailrs_sync::passwords::Secrets;
use mailrs_sync::sign_in::Wanted;
use mailrs_sync::{Clients, Connector};

use crate::CoreError;
use crate::mail::Mail;
use crate::prefs::read_config;

/// One provider on the first page.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetupTile {
    /// `google`, `microsoft`, `icloud`, `fastmail`, `yahoo` or `other`.
    pub key: String,
    pub title: String,
    pub subtitle: String,
    /// `google` or `microsoft` when the sign-in runs in the browser.
    pub browser: Option<String>,
    /// What a screen reader says for the tile.
    pub described: String,
    pub stamp: SetupStamp,
}

/// A provider's mark: its initial on a tile of its colour, or its logo.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetupStamp {
    pub letter: String,
    /// `#rrggbb`.
    pub colour: String,
    /// `google`, `microsoft` or `icloud`, the logo's file name, when the
    /// tile shows artwork in place of the letter.
    pub logo: Option<String>,
}

/// Where the flow stands, as far as the band at the top cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BandStep {
    Pick,
    Address,
    Lookup,
    Browser,
    Found,
    Refused,
    Unreachable,
    Closed,
    Added,
}

/// What the band shows: the envelope's pose, the stamp on its corner and
/// what a screen reader hears for it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BandView {
    /// The pose's name, as its SVG is called: `idle`, `stamped`,
    /// `browser`, `lookup`, `error`, `unreachable` or `success`.
    pub pose: String,
    pub stamp: Option<SetupStamp>,
    pub described: String,
}

/// How far one lookup question has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CheckStatus {
    NotListed,
    Waiting,
    Asking,
    Answered,
    Nothing,
}

/// What Continue on the address page does.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum AddressAnswer {
    /// Look this address up.
    Look { address: String },
    /// Offer this address in place of the one typed.
    Suggest { address: String, words: String },
    /// Stay and say this.
    Say { words: String },
}

/// One row of the lookup page: what discovery asks, and how it stands.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LookupCheck {
    pub title: String,
    pub state: String,
    pub status: CheckStatus,
    /// The MX row, which every later question waits for.
    pub mx: bool,
}

/// A page that helps, with its name.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetupLink {
    pub label: String,
    pub url: String,
}

/// The password page.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PasswordPage {
    pub address: String,
    /// What the field is called: Password, or App Password.
    pub field: String,
    /// Where the servers were found.
    pub found: String,
    /// The servers, in one line.
    pub servers: String,
    /// The line under the password when the provider wants an app
    /// password, and the page that makes one.
    pub hint: Option<String>,
    pub hint_link: Option<SetupLink>,
    /// The person must say yes to the servers before the password goes.
    pub confirm: bool,
}

/// Server Settings, as the form shows them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ServerForm {
    /// `imap` or `pop3`.
    pub protocol: String,
    pub incoming: ServerFields,
    pub outgoing: ServerFields,
    /// What the page says above the form, when it opened for a reason.
    pub line: Option<String>,
}

/// One server's fields. `security` is `tls` or `starttls`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ServerFields {
    pub host: String,
    pub port: u16,
    pub security: String,
    pub user: String,
}

/// What a failed sign-in says.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FailureCard {
    /// The whole failure in one sentence, for a screen reader.
    pub line: String,
    pub title: String,
    pub body: String,
    /// The server's own words, as it sent them.
    pub said: Option<String>,
    pub links: Vec<SetupLink>,
    /// Text a Copy button puts on the clipboard.
    pub copy: Option<String>,
    /// A server did not answer, rather than turning the sign-in down.
    pub unreachable: bool,
}

/// Where discovery takes the dialog.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum SetupStep {
    Password { page: PasswordPage },
    /// Stay on the address page and say this.
    Say { words: String },
    /// The provider lets no other mail app in.
    Closed { words: String },
    /// The provider's own sign-in serves the address, in the browser.
    Browser { browser: String },
    /// Nothing found: Server Settings, filled with a guess.
    Servers { form: ServerForm },
}

/// How a sign-in ended.
// UniFFI hands the enum to Swift by value, so the larger variant cannot
// go behind a Box; one answer per sign-in costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum SignInAnswer {
    Added { account_id: i64, email: String },
    /// Show this on the password page, which may now name other servers.
    Failed { card: FailureCard, page: PasswordPage },
}

/// Hears the lookup page's rows change while discovery runs.
#[uniffi::export(with_foreign)]
pub trait LookupListener: Send + Sync {
    fn changed(&self, checks: Vec<LookupCheck>);
}

/// Opens the provider's sign-in page in the browser.
#[uniffi::export(with_foreign)]
pub trait BrowserOpener: Send + Sync {
    // Not `open`: UniFFI writes that name into the C header with Swift's
    // backticks around it, which C cannot read.
    fn open_page(&self, url: String);
}

/// What Remove Account asks.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RemoveQuestion {
    pub title: String,
    pub body: String,
    pub email: String,
}

/// The Add Account steps, from the address to a running account.
#[derive(uniffi::Object)]
pub struct AccountSetup {
    mail: Arc<Mail>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The address whose suggestion the page already showed.
    declined: Option<Address>,
    address: Option<Address>,
    proposal: Option<Proposal>,
    /// Sending on it, or dropping it, stops a browser sign-in's wait.
    cancel: Option<async_channel::Sender<()>>,
    /// The account signing in again, rather than a new one.
    again: Option<String>,
}

#[uniffi::export]
impl Mail {
    /// A fresh walk through Add Account.
    pub fn account_setup(self: Arc<Self>) -> Arc<AccountSetup> {
        Arc::new(AccountSetup {
            mail: self,
            state: Mutex::new(State::default()),
        })
    }

    /// Whether the store holds no account yet, which shows the welcome
    /// page in place of the mail.
    pub fn has_no_accounts(&self) -> bool {
        let db = self.db.clone();
        self.run(async move { db.read(accounts::list_accounts).await })
            .map(|all| all.is_empty())
            .unwrap_or(false)
    }

    /// What Remove Account asks before it goes ahead. What a POP3
    /// account's removal loses depends on whether its setting already
    /// takes mail off the server.
    pub fn remove_account_question(&self, account_id: i64) -> Result<RemoveQuestion, CoreError> {
        let account = self.account_row(account_id)?;
        let remove = match account.provider {
            Provider::Pop3 => {
                let db = self.db.clone();
                self.run(async move { db.read(move |c| accounts::pop3_remove(c, account_id)).await })
                    .unwrap_or(RemoveSetting::Downloaded)
            }
            _ => RemoveSetting::Never,
        };
        Ok(RemoveQuestion {
            title: fill(&gettext("Remove {account}?"), &[("account", &account.email)]),
            body: mailrs_appcore::offered::remove_account_body(&account, remove),
            email: account.email,
        })
    }

    /// Stops syncing an account and deletes its mail on this Mac and what
    /// signs it in. Its mail on the server stays.
    pub fn remove_account(&self, account_id: i64) -> Result<(), CoreError> {
        let account = self.account_row(account_id)?;
        if let Some(engine) = self.running.current() {
            engine.stop_account(account.id);
        }
        self.actions.forget_account(account.id);
        let db = self.db.clone();
        self.run(async move { shared::forget_account(&db, &Secrets::keyring(), account).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        self.changed("accounts");
        Ok(())
    }
}

impl Mail {
    pub(crate) fn account_row(&self, account_id: i64) -> Result<Account, CoreError> {
        let db = self.db.clone();
        self.run(async move { db.read(move |c| accounts::account(c, account_id)).await })?
            .ok_or_else(|| CoreError::Store("That account is no longer here.".into()))
    }
}

#[uniffi::export]
impl AccountSetup {
    /// The providers the first page offers. Microsoft shows only in a
    /// build with its sign-in.
    pub fn tiles(&self) -> Vec<SetupTile> {
        post::tiles(Clients::built_in().microsoft.is_some())
            .into_iter()
            .map(|tile| SetupTile {
                key: tile_key(tile).into(),
                title: tile.title(),
                subtitle: tile.subtitle(),
                browser: tile.browser().map(|b| browser_key(b).into()),
                described: tile.described(),
                stamp: stamp(tile.stamp()),
            })
            .collect()
    }

    /// Continue on the address page for `typed`.
    pub fn on_continue(&self, typed: String) -> AddressAnswer {
        let mut state = self.lock();
        match add_account::on_continue(&typed, state.declined.as_ref()) {
            Continue::Look(address) => AddressAnswer::Look { address: address.full() },
            Continue::Suggest(better) => {
                let words = add_account::did_you_mean(&better);
                state.declined = Address::parse(&typed);
                AddressAnswer::Suggest {
                    address: better.full(),
                    words,
                }
            }
            Continue::Say(words) => AddressAnswer::Say { words },
        }
    }

    /// Finds `address`'s servers, telling `listener` how each question
    /// stands. Blocks: call it off the main thread.
    pub fn discover(&self, address: String, listener: Arc<dyn LookupListener>) -> Result<SetupStep, CoreError> {
        let parsed = Address::parse(&address).ok_or_else(|| CoreError::Store(add_account::not_an_address()))?;
        let (heard, hearing) = async_channel::unbounded();
        let domain = parsed.domain.clone();
        self.mail.runtime.spawn(async move {
            let mut checks = Checks::default();
            while let Ok(news) = hearing.recv().await {
                checks.heard(news);
                listener.changed(
                    Check::ALL
                        .into_iter()
                        .map(|check| {
                            let state = checks.state(check);
                            LookupCheck {
                                title: check.title(&domain),
                                state: state.text(),
                                status: status(state),
                                mx: check == Check::Mx,
                            }
                        })
                        .collect(),
                );
            }
        });
        let asked = address.clone();
        let found = self
            .mail
            .run(async move { shared::discover(&asked, heard, false).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let microsoft = Clients::built_in().microsoft.is_some();
        let next = add_account::after_discovery(found, &parsed, microsoft);
        let mut state = self.lock();
        state.address = Some(parsed.clone());
        Ok(match next {
            Next::Password(proposal) => {
                let page = password_page(&parsed, &proposal);
                state.proposal = Some(proposal);
                SetupStep::Password { page }
            }
            Next::Say(words) => SetupStep::Say { words },
            Next::Closed(words) => SetupStep::Closed { words },
            Next::Google => SetupStep::Browser { browser: "google".into() },
            Next::Microsoft => SetupStep::Browser { browser: "microsoft".into() },
            Next::Manual { proposal, line } => {
                let form = form(&proposal, Some(line));
                state.proposal = Some(proposal);
                SetupStep::Servers { form }
            }
        })
    }

    /// Server Settings for the address being added: the servers found,
    /// or a guess from the address. `address` is the one typed, for an
    /// address entered by hand that was never looked up.
    pub fn server_form(&self, address: String) -> Result<ServerForm, CoreError> {
        let mut state = self.lock();
        if state.address.is_none() {
            state.address = Some(Address::parse(&address).ok_or_else(|| CoreError::Store(add_account::not_an_address()))?);
        }
        let address = state.address.clone().expect("set above");
        let proposal = state.proposal.get_or_insert_with(|| add_account::guess(&address));
        Ok(form(proposal, None))
    }

    /// The form with its protocol switched to `protocol`: the ports and
    /// the host follow when they still hold what the other protocol had.
    pub fn switch_protocol(&self, form: ServerForm, protocol: String) -> ServerForm {
        let now = protocol_of(&protocol);
        let security = security_of(&form.incoming.security);
        let mut switched = form.clone();
        switched.protocol = protocol;
        switched.incoming.port = add_account::port_after_protocol(form.incoming.port, security, now);
        if let Some(proposal) = self.lock().proposal.as_ref()
            && let Some(server) = add_account::server_after_protocol(proposal, &form.incoming.host, now)
        {
            switched.incoming.host = server.host;
            switched.incoming.port = server.port;
            switched.incoming.security = security_key(server.security).into();
        }
        switched
    }

    /// The port after a server's Security switched, by the same rule.
    pub fn port_after_security(&self, outgoing: bool, protocol: String, port: u16, security: String) -> u16 {
        let role = if outgoing { Role::Outgoing } else { Role::Incoming };
        add_account::port_after_switch(role, protocol_of(&protocol), port, security_of(&security))
    }

    /// Takes the servers typed in Server Settings, for the password page,
    /// or the line that says what to fix.
    pub fn use_servers(&self, form: ServerForm) -> Result<PasswordPage, CoreError> {
        let mut state = self.lock();
        let address = state.address.clone().ok_or_else(|| CoreError::Store(add_account::not_an_address()))?;
        let before = state.proposal.clone().unwrap_or_else(|| add_account::guess(&address));
        let typed = |fields: &ServerFields| Typed {
            host: fields.host.clone(),
            port: fields.port,
            security: security_of(&fields.security),
            user: fields.user.clone(),
        };
        let proposal = add_account::typed_servers(&typed(&form.incoming), &typed(&form.outgoing), &before, &address)
            .map_err(CoreError::Store)?;
        let protocol = protocol_of(&form.protocol);
        let remove = before.pop3_remove.unwrap_or_default();
        let proposal = add_account::with_protocol(proposal, protocol, remove);
        let page = password_page(&address, &proposal);
        state.proposal = Some(proposal);
        Ok(page)
    }

    /// Signs in with `password`, moving on to discovery's next servers
    /// when one does not answer, and starts the account syncing. `name`
    /// is how mail from it is signed, kept when not empty. Blocks: call it
    /// off the main thread.
    pub fn sign_in(&self, password: String, name: String) -> Result<SignInAnswer, CoreError> {
        let (address, mut proposal) = {
            let state = self.lock();
            match (state.address.clone(), state.proposal.clone()) {
                (Some(address), Some(proposal)) => (address, proposal),
                _ => return Err(CoreError::Store(add_account::not_an_address())),
            }
        };
        let engine = self.engine()?;
        let config = read_config()?;
        let window_days = config.engine_config().window_days;
        loop {
            let attempt = add_account::attempt(&address, &proposal, &password);
            let (db, passwords) = (self.mail.db.clone(), Arc::clone(&Secrets::keyring().passwords));
            let tried = match proposal.protocol {
                Protocol::Imap => self.mail.run(async move { shared::sign_in_imap(&db, passwords, window_days, attempt).await }),
                Protocol::Pop3 => self.mail.run(async move { shared::sign_in_pop3(&db, passwords, attempt).await }),
            };
            match tried {
                Ok((account, services)) => {
                    engine.start_account(account.id, services);
                    keep_send_as_name(&account, &name);
                    self.signed_in_again(&account.email);
                    self.mail.changed("accounts");
                    return Ok(SignInAnswer::Added {
                        account_id: account.id,
                        email: account.email,
                    });
                }
                Err(err) => match add_account::after_failure(&err, &proposal, &address) {
                    // A next candidate the person need not say yes to is
                    // tried at once, with the same password.
                    Outcome::TryNext(next) if !next.confirm => proposal = *next,
                    Outcome::TryNext(next) => {
                        let card = card(add_account::failure(&err, &proposal));
                        let page = password_page(&address, &next);
                        self.lock().proposal = Some(*next);
                        return Ok(SignInAnswer::Failed { card, page });
                    }
                    Outcome::Failed(failure) => {
                        self.lock().proposal = Some(proposal.clone());
                        return Ok(SignInAnswer::Failed {
                            card: card(failure),
                            page: password_page(&address, &proposal),
                        });
                    }
                },
            }
        }
    }

    /// Signs in to Google or Microsoft in the browser, which `opener`
    /// opens, and starts the account syncing. With `expected`, Google
    /// must come back with that address, and Microsoft's page starts
    /// from it. Blocks for as long as the person takes, up to five
    /// minutes: call it off the main thread; `cancel_browser` stops it.
    pub fn sign_in_in_browser(
        &self,
        browser: String,
        expected: Option<String>,
        opener: Arc<dyn BrowserOpener>,
    ) -> Result<SignInAnswer, CoreError> {
        let browser = browser_of(&browser);
        let wanted = match (browser, expected.filter(|e| !e.trim().is_empty())) {
            (_, None) => Wanted::Anyone,
            (Browser::Google, Some(address)) => Wanted::Only(address),
            (Browser::Microsoft, Some(address)) => Wanted::Suggested(address),
        };
        let engine = self.engine()?;
        let connector = Connector {
            db: self.mail.db.clone(),
            config: read_config()?,
            clients: Clients::built_in(),
            secrets: Secrets::keyring(),
        };
        let (cancel, canceled) = async_channel::bounded(1);
        self.lock().cancel = Some(cancel);
        let signed = self.mail.run(async move {
            let open = move |url: &str| opener.open_page(url.to_string());
            shared::sign_in_in_browser(&connector, browser, wanted, open, canceled).await
        });
        self.lock().cancel = None;
        let signed = signed.map_err(|err| CoreError::Store(err.to_string()))?;
        engine.start_account(signed.account.id, signed.services);
        if let Some(name) = &signed.name {
            keep_send_as_name(&signed.account, name);
        }
        self.signed_in_again(&signed.account.email);
        self.mail.changed("accounts");
        Ok(SignInAnswer::Added {
            account_id: signed.account.id,
            email: signed.account.email,
        })
    }

    /// Starts signing account `account_id` in again: in the browser for
    /// Google and Microsoft, else on the password page with the servers
    /// it has kept, or Server Settings when it kept none.
    pub fn sign_in_again(&self, account_id: i64) -> Result<SetupStep, CoreError> {
        let account = self.mail.account_row(account_id)?;
        let address = Address::parse(&account.email).ok_or_else(|| CoreError::Store(add_account::not_an_address()))?;
        self.lock().again = Some(account.email.clone());
        if let Some(browser) = Browser::of(account.provider) {
            return Ok(SetupStep::Browser {
                browser: browser_key(browser).into(),
            });
        }
        let db = self.mail.db.clone();
        let kept = self.mail.run(async move {
            db.read(move |c| {
                if let Some(servers) = servers::load(c, account_id)? {
                    return Ok(Some(add_account::Kept::Imap(servers)));
                }
                let Some(servers) = servers::load_pop3(c, account_id)? else {
                    return Ok(None);
                };
                let remove = accounts::pop3_remove(c, account_id)?;
                Ok(Some(add_account::Kept::Pop3(servers, remove)))
            })
            .await
        })?;
        let mut state = self.lock();
        state.address = Some(address.clone());
        match kept {
            Some(kept) => {
                let proposal = add_account::saved_proposal(&account, &kept);
                let page = password_page(&address, &proposal);
                state.proposal = Some(proposal);
                Ok(SetupStep::Password { page })
            }
            None => {
                let proposal = add_account::guess(&address);
                let form = form(&proposal, None);
                state.proposal = Some(proposal);
                Ok(SetupStep::Servers { form })
            }
        }
    }

    /// Stops waiting for the browser.
    pub fn cancel_browser(&self) {
        if let Some(cancel) = self.lock().cancel.take() {
            let _ = cancel.try_send(());
        }
    }

    /// The band for `step`. The stamp follows what the flow knows: the
    /// servers found, the browser's provider, else what the built-in list
    /// says of the address `typed`, else the tile picked, else any server.
    pub fn band(&self, step: BandStep, typed: String, tile: Option<String>, browser: Option<String>) -> BandView {
        let microsoft = Clients::built_in().microsoft.is_some();
        let tile = tile.and_then(|key| post::tiles(true).into_iter().find(|t| tile_key(*t) == key));
        let browser = browser.map(|key| browser_of(&key).name());
        let from_servers = || {
            self.lock().proposal.as_ref().map(|proposal| {
                let provider = proposal.provider_name.clone();
                let stamp = if proposal.info.is_some() { post::stamp_for(&provider) } else { post::ANY_SERVER };
                (stamp, Some(provider))
            })
        };
        let from_address = || {
            let domain = typed.rsplit_once('@').map(|(_, d)| d.trim().trim_end_matches('.').to_lowercase());
            match domain.as_deref().and_then(|d| post::advice(d, microsoft)) {
                Some(advice) => (advice.stamp, Some(advice.provider)),
                None => (tile.filter(|t| *t != Tile::Other).map_or(post::ANY_SERVER, Tile::stamp), None),
            }
        };
        let from_browser = || browser.map(|name| (post::stamp_for(name), Some(name.to_string())));
        let (core, (marked, provider)) = match step {
            BandStep::Pick => (Step::Pick, (post::ANY_SERVER, None)),
            BandStep::Address => (Step::Address, from_address()),
            BandStep::Lookup => (Step::Lookup, (from_address().0, None)),
            BandStep::Closed => (Step::Closed, from_address()),
            BandStep::Browser => (Step::Browser, from_browser().unwrap_or_else(from_address)),
            BandStep::Found => (Step::Found, from_servers().unwrap_or_else(from_address)),
            BandStep::Refused => (
                Step::Refused,
                from_browser().or_else(from_servers).unwrap_or_else(from_address),
            ),
            BandStep::Unreachable => (Step::Unreachable, from_servers().unwrap_or_else(from_address)),
            BandStep::Added => (
                Step::Added,
                from_browser().or_else(from_servers).unwrap_or_else(from_address),
            ),
        };
        // Every step has a band; only Server Settings goes without.
        let pose = post::band_for(core).unwrap_or(post::Band::Idle);
        BandView {
            pose: pose.name().into(),
            stamp: (pose != post::Band::Idle).then(|| stamp(marked)),
            described: pose.described(provider.as_deref()),
        }
    }

    /// How long the browser sign-in waits, in seconds.
    pub fn browser_wait_seconds(&self) -> u32 {
        shared::BROWSER_WAIT.as_secs() as u32
    }

    /// "4:59 left", for the browser page's countdown.
    pub fn time_left(&self, seconds: u32) -> String {
        add_account::time_left(std::time::Duration::from_secs(u64::from(seconds)))
    }
}

impl AccountSetup {
    /// An account that signed in again asks Gmail for its send-as
    /// addresses at once, as the GTK app does.
    fn signed_in_again(&self, email: &str) {
        if self.lock().again.as_deref() != Some(email) {
            return;
        }
        let path = Settings::default_path();
        let mut settings = Settings::load(&path);
        Change::SignedInAgain { account: email.to_string() }.apply_to(&mut settings);
        if let Err(err) = settings.save(&path) {
            tracing::warn!(error = %err, "could not note an account signed in again");
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn engine(&self) -> Result<Arc<mailrs_sync::SyncEngine>, CoreError> {
        // Without the engine there is nothing to start the account on:
        // another copy of Iris holds the store.
        self.mail
            .running
            .current()
            .ok_or_else(|| CoreError::Store("Another copy of Iris is syncing your mail. Quit it, then try again.".into()))
    }
}

/// Keeps the name the person gave as the one mail from `account` is
/// signed with.
fn keep_send_as_name(account: &Account, name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let path = Settings::default_path();
    let mut settings = Settings::load(&path);
    Change::SendAsAddresses {
        account: account.email.clone(),
        addresses: vec![mailrs_appcore::compose::SendAsAddress {
            email: account.email.clone(),
            name: Some(name.to_string()),
            default: true,
            ..Default::default()
        }],
        at: mailrs_sync::now_millis(),
    }
    .apply_to(&mut settings);
    if let Err(err) = settings.save(&path) {
        tracing::warn!(error = %err, "could not keep the name for a new account");
    }
}

fn password_page(address: &Address, proposal: &Proposal) -> PasswordPage {
    let hint = add_account::password_hint(proposal);
    PasswordPage {
        address: address.full(),
        field: add_account::password_title(proposal),
        found: add_account::found_line(proposal),
        servers: add_account::servers_summary(proposal),
        hint_link: hint.as_ref().and_then(|h| h.link.clone()).map(|l| SetupLink {
            label: l.label,
            url: l.url,
        }),
        hint: hint.map(|h| h.line),
        confirm: proposal.confirm,
    }
}

fn form(proposal: &Proposal, line: Option<String>) -> ServerForm {
    let fields = |server: &Server, user: &Option<String>| ServerFields {
        host: server.host.clone(),
        port: server.port,
        security: security_key(server.security).into(),
        user: user.clone().unwrap_or_default(),
    };
    ServerForm {
        protocol: match proposal.protocol {
            Protocol::Imap => "imap",
            Protocol::Pop3 => "pop3",
        }
        .into(),
        incoming: fields(&proposal.incoming, &proposal.incoming_user),
        outgoing: fields(&proposal.smtp, &proposal.smtp_user),
        line,
    }
}

fn card(failure: add_account::Failure) -> FailureCard {
    FailureCard {
        line: failure.line,
        title: failure.title,
        body: failure.body,
        said: failure.said,
        links: failure
            .links
            .into_iter()
            .map(|l| SetupLink { label: l.label, url: l.url })
            .collect(),
        copy: failure.copy,
        unreachable: matches!(failure.kind, FailureKind::Unreachable { .. }),
    }
}

fn stamp(stamp: Stamp) -> SetupStamp {
    SetupStamp {
        letter: stamp.letter.to_string(),
        colour: stamp.colour.into(),
        logo: stamp.logo.map(str::to_string),
    }
}

fn status(state: CheckState) -> CheckStatus {
    match state {
        CheckState::NotListed => CheckStatus::NotListed,
        CheckState::Waiting => CheckStatus::Waiting,
        CheckState::Asking => CheckStatus::Asking,
        CheckState::Answered => CheckStatus::Answered,
        CheckState::Nothing => CheckStatus::Nothing,
    }
}

fn tile_key(tile: Tile) -> &'static str {
    match tile {
        Tile::Google => "google",
        Tile::Microsoft => "microsoft",
        Tile::Icloud => "icloud",
        Tile::Fastmail => "fastmail",
        Tile::Yahoo => "yahoo",
        Tile::Other => "other",
    }
}

fn browser_key(browser: Browser) -> &'static str {
    match browser {
        Browser::Google => "google",
        Browser::Microsoft => "microsoft",
    }
}

fn browser_of(key: &str) -> Browser {
    match key {
        "microsoft" => Browser::Microsoft,
        _ => Browser::Google,
    }
}

fn protocol_of(key: &str) -> Protocol {
    match key {
        "pop3" => Protocol::Pop3,
        _ => Protocol::Imap,
    }
}

fn security_of(key: &str) -> Security {
    match key {
        "starttls" => Security::StartTls,
        _ => Security::Tls,
    }
}

fn security_key(security: Security) -> &'static str {
    match security {
        Security::Tls => "tls",
        Security::StartTls => "starttls",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Arc<Mail>, Arc<AccountSetup>) {
        let dir = tempfile::tempdir().unwrap();
        let mail = Arc::new(Mail::open(dir.path().join("mailrs.db").display().to_string()).unwrap());
        let setup = Arc::clone(&mail).account_setup();
        (dir, mail, setup)
    }

    #[test]
    fn a_new_store_has_no_accounts_and_offers_the_providers() {
        let (_dir, mail, setup) = setup();
        assert!(mail.has_no_accounts());
        let keys: Vec<String> = setup.tiles().into_iter().map(|t| t.key).collect();
        assert!(keys.contains(&"google".to_string()));
        assert_eq!(keys.last().map(String::as_str), Some("other"));
    }

    #[test]
    fn the_band_stamps_what_the_flow_knows() {
        let (_dir, _mail, setup) = setup();
        let pick = setup.band(BandStep::Pick, String::new(), None, None);
        assert_eq!((pick.pose.as_str(), pick.stamp), ("idle", None));

        let icloud = setup.band(BandStep::Address, "dana@icloud.com".into(), None, None);
        assert_eq!(icloud.pose, "stamped");
        assert_eq!(icloud.stamp.map(|s| s.letter), Some("i".to_string()));

        // An address the list does not know keeps the tile's stamp, and the
        // lookup shows it too.
        let lookup = setup.band(BandStep::Lookup, "dana@example.com".into(), Some("fastmail".into()), None);
        assert_eq!(lookup.pose, "lookup");
        assert_eq!(lookup.stamp.map(|s| s.letter), Some("F".to_string()));

        let browser = setup.band(BandStep::Browser, String::new(), None, Some("google".into()));
        assert_eq!(browser.pose, "browser");
        assert_eq!(browser.stamp.and_then(|s| s.logo), Some("google".to_string()));
    }

    #[test]
    fn a_misspelled_domain_is_offered_once_then_taken_as_typed() {
        let (_dir, _mail, setup) = setup();
        let AddressAnswer::Suggest { address, .. } = setup.on_continue("dana@gmial.com".into()) else {
            panic!("a suggestion");
        };
        assert_eq!(address, "dana@gmail.com");
        assert_eq!(
            setup.on_continue("dana@gmial.com".into()),
            AddressAnswer::Look { address: "dana@gmial.com".into() }
        );
        assert!(matches!(setup.on_continue("dana".into()), AddressAnswer::Say { .. }));
    }

    #[test]
    fn servers_typed_by_hand_lead_to_the_password_page() {
        let (_dir, _mail, setup) = setup();
        let mut form = setup.server_form("dana@example.com".into()).unwrap();
        assert_eq!(form.protocol, "imap");
        form.incoming.host = "mail.example.com".into();
        form.outgoing.host = "smtp.example.com".into();
        let page = setup.use_servers(form.clone()).unwrap();
        assert_eq!(page.address, "dana@example.com");
        assert!(page.servers.starts_with("mail and smtp.example.com"), "{}", page.servers);
        assert!(!page.confirm, "servers typed by hand need no second yes");

        form.incoming.host = String::new();
        assert!(setup.use_servers(form).is_err());
    }

    #[test]
    fn switching_to_pop3_moves_a_default_port_along() {
        let (_dir, _mail, setup) = setup();
        let form = setup.server_form("dana@example.com".into()).unwrap();
        let mut imap = form.clone();
        imap.incoming.port = 993;
        imap.incoming.security = "tls".into();
        assert_eq!(setup.switch_protocol(imap, "pop3".into()).incoming.port, 995);
        assert_eq!(setup.port_after_security(true, "imap".into(), 465, "starttls".into()), 587);
    }
}
