//! Adding and removing accounts, as both front ends do it: finding an
//! address's servers, signing in with a password or in the browser, and
//! forgetting an account. Starting or stopping the account on the sync
//! engine stays with the caller, which holds the engine.

use std::future::Future;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use mailrs_discover::{Found, Net, RealNet, Security, SrvRecord};
use mailrs_domain::Account;
use mailrs_domain::translate::gettext;
use mailrs_store::{Db, accounts};
use mailrs_sync::passwords::{Passwords, Secrets};
use mailrs_sync::sign_in::{
    Browser, NewImap, NewPop3, SignedIn, Wanted, check_pop3, imap_signed_in, in_browser, pop3_signed_in,
};
use mailrs_sync::{AccountServices, Connector, connect_imap, connect_pop3, now_millis, pop3_servers_for, servers_for};

use crate::add_account::Attempt;
use crate::add_account::lookup::{Check, Heard, check_of_url};

/// How long a browser sign-in waits for the person to finish on the
/// provider's page. The waiting page counts it down.
pub const BROWSER_WAIT: std::time::Duration = std::time::Duration::from_secs(300);

/// The servers for `address`, found the way discovery goes: the provider
/// table, then DNS, the domain's own files and a probe. `offline` reads
/// the table alone, as the demo does, so nothing goes anywhere. Each
/// question discovery sends out, and its answer, goes to `heard` for the
/// lookup page.
pub async fn discover(address: &str, heard: async_channel::Sender<Heard>, offline: bool) -> Result<Found> {
    Ok(match offline {
        true => mailrs_discover::find(&Watched { net: Offline, heard }, address).await,
        false => mailrs_discover::find(&Watched { net: RealNet::new()?, heard }, address).await,
    })
}

/// Runs `browser`'s sign-in and keeps the account and its refresh token.
/// `open` receives the page to open. Each sign-in asks for every
/// permission Iris uses; the provider keeps what the account already
/// granted, so signing in again asks nothing new. A message on `cancel`,
/// or its sender dropped, stops the wait, as Cancel on the waiting page
/// does; so does [`BROWSER_WAIT`] passing.
pub async fn sign_in_in_browser(
    connector: &Connector,
    browser: Browser,
    wanted: Wanted,
    open: impl FnOnce(&str),
    cancel: async_channel::Receiver<()>,
) -> Result<SignedIn> {
    let run = in_browser(connector, browser, wanted, open);
    let waited = tokio::select! {
        waited = tokio::time::timeout(BROWSER_WAIT, run) => waited,
        _ = cancel.recv() => bail!(gettext("Canceled.")),
    };
    Ok(waited.map_err(|_| anyhow!(gettext("Gave up waiting for the browser after five minutes.")))??)
}

/// Signs in to an IMAP account: tries the login on both servers, then
/// keeps the account, its servers and its password, and builds its
/// services. Nothing is kept when the login fails, which comes back as a
/// `CheckError` naming the server. An account signing in again keeps its
/// mail and starts over with the new password.
pub async fn sign_in_imap(
    db: &Db,
    passwords: Arc<Passwords>,
    window_days: i64,
    attempt: Attempt,
) -> Result<(Account, AccountServices)> {
    let Attempt {
        address,
        provider_name,
        incoming,
        smtp,
        incoming_login,
        smtp_login,
        password,
        ..
    } = attempt;
    let checked = mailrs_imap::check(&incoming, &smtp, &incoming_login, &smtp_login, &password).await?;
    let new = NewImap {
        address,
        provider_name,
        servers: servers_for(&incoming, &checked.imap_user, &smtp, &checked.smtp_user),
        password,
    };
    let account = imap_signed_in(db, Arc::clone(&passwords), new, now_millis()).await?;
    let services = connect_imap(db, passwords, &account, window_days).await?;
    Ok((account, services))
}

/// Signs in to a POP3 account: signs in to the POP3 server, which must
/// answer UIDL, then to the SMTP server, then keeps the account, its
/// servers, its removal setting and its password, and builds its
/// services. Nothing is kept when a sign-in fails; a failure comes back as
/// a `CheckError` naming the server, as for IMAP.
pub async fn sign_in_pop3(db: &Db, passwords: Arc<Passwords>, attempt: Attempt) -> Result<(Account, AccountServices)> {
    let Attempt {
        address,
        provider_name,
        incoming,
        smtp,
        incoming_login,
        smtp_login,
        password,
        pop3_remove,
        ..
    } = attempt;
    let pop3_user = check_pop3(&incoming, &incoming_login, &password).await?;
    let smtp_user = mailrs_imap::check_smtp(&smtp, &pop3_user, &smtp_login, &password).await?;
    let new = NewPop3 {
        address,
        provider_name,
        servers: pop3_servers_for(&incoming, &pop3_user, &smtp, &smtp_user),
        remove: pop3_remove,
        password,
    };
    let account = pop3_signed_in(db, Arc::clone(&passwords), new, now_millis()).await?;
    let services = connect_pop3(db, passwords, &account).await?;
    Ok((account, services))
}

/// Deletes an account's local mail and what signs it in: a Google or
/// Microsoft account's refresh token, an IMAP or POP3 account's
/// password. The caller stops syncing it first.
pub async fn forget_account(db: &Db, secrets: &Secrets, account: Account) -> Result<()> {
    let id = account.id;
    db.write(move |c| accounts::delete_account(c, id)).await?;
    secrets.forget(&account).await?;
    Ok(())
}

/// A network that answers nothing, for the demo: discovery then finds
/// what the provider table knows and sends nothing anywhere.
struct Offline;

impl Net for Offline {
    async fn mx(&self, _domain: &str) -> Vec<String> {
        Vec::new()
    }

    async fn srv(&self, _name: &str) -> Vec<SrvRecord> {
        Vec::new()
    }

    async fn get(&self, _url: &str) -> Option<String> {
        None
    }

    async fn reaches(&self, _host: &str, _port: u16, _security: Security) -> bool {
        false
    }
}

/// A network that tells the lookup page each time discovery asks it
/// something and each time the answer comes back. The page reads nothing
/// else from it, so the answers themselves never leave discovery.
struct Watched<N> {
    net: N,
    heard: async_channel::Sender<Heard>,
}

impl<N: Net> Watched<N> {
    async fn asking<T>(&self, check: Check, ask: impl Future<Output = T>, found: impl Fn(&T) -> bool) -> T {
        let _ = self.heard.try_send(Heard::Asked(check));
        let answer = ask.await;
        let _ = self.heard.try_send(Heard::Answered(check, found(&answer)));
        answer
    }
}

impl<N: Net> Net for Watched<N> {
    async fn mx(&self, domain: &str) -> Vec<String> {
        self.asking(Check::Mx, self.net.mx(domain), |hosts| !hosts.is_empty())
            .await
    }

    async fn srv(&self, name: &str) -> Vec<SrvRecord> {
        self.asking(Check::Common, self.net.srv(name), |records| !records.is_empty())
            .await
    }

    async fn get(&self, url: &str) -> Option<String> {
        self.asking(check_of_url(url), self.net.get(url), Option::is_some)
            .await
    }

    async fn reaches(&self, host: &str, port: u16, security: Security) -> bool {
        self.asking(Check::Common, self.net.reaches(host, port, security), |ok| *ok)
            .await
    }
}
