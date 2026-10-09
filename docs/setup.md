# Setting up Iris

Iris works with Google accounts and with any mail provider that
offers IMAP and SMTP, such as Fastmail, iCloud or Yahoo. Add each account in
the app, or from the command line.

## Adding an account

The first window shows the providers you can add. For each account after the
first, open the main menu and choose **Add Account**. From a terminal,
`iris-cli account add` does the same for a Google account.

### Google

Choose **Google**. Your browser opens Google's sign-in page; sign in and allow
the permissions Iris asks for, then come back to the app. Until Google
finishes verifying the app, the page warns that Google has not verified it.
Choose **Advanced**, then continue.

### Other providers

Choose your provider, or **Other**, and type your address. Iris finds
the server settings for you and says what the provider needs, such as an app
password for iCloud, Fastmail or Yahoo, with a link to the page where you make
one. If it can't find them, **Enter Server Settings** lets you type the
incoming (IMAP) and outgoing (SMTP) servers yourself. The password goes to
the macOS Keychain, never into a file.

The account appears in the sidebar and starts downloading.

### Calendars, contacts and rules on other providers

Iris looks for an IMAP account's calendar (CalDAV), contacts
(CardDAV) and rules server (ManageSieve) when you add the account, with
the same password. Fastmail, iCloud, Yahoo, Zoho, GMX, WEB.DE, mail.com,
Yandex, mailbox.org and Posteo are known; for another server it asks the
domain's DNS and its well-known addresses. **Preferences > Contacts &
Calendar** lists what it found under **Calendar, Contacts and Rules
Servers**, with **Find Again**, and **Edit** to type a CalDAV or CardDAV
address yourself. A server outside your address's domain waits there for
you to press **Use It** before your password goes to it. Where the server
runs no rules, Iris runs them on this computer while it is open.

Adding a Google account asks for every permission Iris uses in that one
visit, so you never see a second consent screen for automatic replies and
Rules, contacts, the calendar and its list of calendars, Google Drive
files, or Delete Forever. Leave a box unticked and
the feature it serves turns off with a reason rather than an error, and
offers **Grant Access** where you would use it: in the calendar sidebar,
on an invitation, in Preferences, or when you first try the feature. An account added
before this way of signing in gets a bar at the top of the mail list that
names what it lacks, until you go through Google's screen once more. Mail
keeps syncing throughout.

Accounts you added through the old setup page signed in with a Google Cloud
client of your own, kept in `config.toml`. They keep
working. The next time one of them signs in again, it moves to the app's own
client, and the `[oauth]` section can go once none of them uses it.

## The config file

Iris needs no config file. To change how often it checks for mail,
how many days of mail it keeps, or how much message text it caches, write
`config.toml` in `~/Library/Application Support/iris/`:

```toml
# These are the defaults.
[sync]
poll_seconds = 30
window_days = 30
body_cache_mb = 1024
```

Iris keeps your refresh tokens in the macOS Keychain, not in this file.

## Building your own copy

A build signs in to Google or Microsoft only when it was compiled with the
project's client, from these variables:

- `IRIS_GOOGLE_CLIENT_ID`
- `IRIS_GOOGLE_CLIENT_SECRET`
- `IRIS_MICROSOFT_CLIENT_ID`

`scripts/dev-macos.sh`, `scripts/macos-install.sh` and
`scripts/macos-dmg.sh` read them from `packaging/secrets.env` when that
file exists. A copy built without them
works in every other way and says so when you try to add a Google account.
A copy built without the Microsoft one hides Microsoft in Add Account.

To build with a client of your own instead, make one in a Google Cloud
project:

1. Enable the Gmail, People, Calendar and Drive APIs.
2. Under Branding, set the app name to `Iris` and your own address as
   the support and developer contact. Leave the logo out for personal use:
   uploading one sends the app toward Google's verification.
3. Under Audience, choose **External** and publish the app. Don't submit it
   for verification for personal use; you click through Google's
   "unverified app" notice once per account instead.
4. Under Data Access, add the seven scopes the app asks for at sign-in, so
   Google's list matches the app's:
   - `https://mail.google.com/`
   - `https://www.googleapis.com/auth/gmail.settings.basic`
   - `https://www.googleapis.com/auth/contacts`
   - `https://www.googleapis.com/auth/calendar.events`
   - `https://www.googleapis.com/auth/calendar.calendarlist`
   - `https://www.googleapis.com/auth/calendar.calendars`
   - `https://www.googleapis.com/auth/drive.file`

   The [privacy policy](privacy-policy.md) says what each one is for.
5. Under Clients, create an OAuth client of type **Desktop app**.

Put its ID and secret in `packaging/secrets.env`:

```sh
IRIS_GOOGLE_CLIENT_ID=1234567890-abc.apps.googleusercontent.com
IRIS_GOOGLE_CLIENT_SECRET=GOCSPX-...
```

Google issues a desktop client secret to identify the app, and anyone who
downloads a desktop app can read it. Your refresh tokens are what grant access
to mail.

To build with a Microsoft client of your own, register a public client in
Microsoft Entra for "Accounts in any organizational directory and personal
Microsoft accounts", with the platform "Mobile and desktop applications" and
the redirect `http://localhost`. Put its id in `packaging/secrets.env`:

```sh
IRIS_MICROSOFT_CLIENT_ID=...
```

## The command line

```sh
cargo run --release -p mailrs-cli -- sync                 # Ctrl-C to stop
cargo run --release -p mailrs-cli -- threads              # unified inbox
cargo run --release -p mailrs-cli -- threads --account you@gmail.com --label SENT
cargo run --release -p mailrs-cli -- show you@gmail.com <thread-id>
cargo run --release -p mailrs-cli -- triage you@gmail.com <thread-id> archive
```

## On macOS

Iris is `Iris.app`, for a Mac with Apple silicon and macOS 26 or later. A
release comes as a DMG; a build from source comes from
`scripts/macos-install.sh`, which loads GTK from Homebrew and so runs on
the Mac that built it.

- **Updates.** A copy installed from a DMG updates itself through Sparkle:
  it checks once a day, asks before it installs, and **Check for
  Updates…** in the Iris menu checks at once. A copy built with
  `scripts/macos-install.sh` has no updater.
- **Secrets.** Refresh tokens, IMAP passwords and the assistant's keys sit
  in the login Keychain. macOS asks once whether Iris may read them; choose
  Always Allow.
- **Unread mail** shows on the Dock icon. Closing the window keeps Iris
  running, and a click on the Dock icon brings the window back.
- **Start at login** is not on macOS yet. Add Iris under System Settings >
  General > Login Items meanwhile.
- **Notifications** for new mail open the conversation when clicked; their
  Archive, Mark as Read, Delete and Reply buttons are not on macOS yet.
- **Skills.** A skill's instructions work, but its scripts do not run on
  macOS yet: they need bubblewrap's sandbox, which is Linux only.

## Where things live

| What | Where | Override |
|---|---|---|
| Config | `~/Library/Application Support/iris/config.toml` | `MAILRS_CONFIG` |
| Preferences | `~/Library/Application Support/iris/settings.toml` | `MAILRS_SETTINGS` |
| Mail cache | `~/Library/Application Support/iris/mailrs.db` | `MAILRS_DATA_DIR` |
| Refresh tokens | login Keychain, service `mailrs`, one entry per address | |

The Keychain service keeps the app's old name, mailrs.

`iris-cli account remove you@gmail.com` deletes an account's local mail and
its Keychain entry. To revoke access on Google's side as well, use
<https://myaccount.google.com/permissions>.
