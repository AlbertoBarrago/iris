<div align="center">

# Iris

Mail and calendar for macOS and Linux, written in Rust.

[![CI](https://github.com/AlbertoBarrago/iris/actions/workflows/ci.yml/badge.svg)](https://github.com/AlbertoBarrago/iris/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)
[![Rust 1.98](https://img.shields.io/badge/rust-1.98-orange?logo=rust)](https://www.rust-lang.org)
[![GTK 4 and libadwaita 1.8](https://img.shields.io/badge/GTK_4-libadwaita_1.8-4a86cf?logo=gnome)](https://gnome.pages.gitlab.gnome.org/libadwaita/)

[Status](#status) · [Features](#features) · [Build](#build) · [Usage](#usage) · [Based on Penguin Mail](#based-on-penguin-mail)

</div>

Iris is mail and calendar for your own computer. It reads Gmail accounts,
Microsoft accounts (Outlook.com, Hotmail, Live and Microsoft 365) and any
IMAP and SMTP account, such as Fastmail, iCloud and Yahoo, and finds the
server settings for you. It shows your accounts in one inbox or one at a
time, and keeps your mail on your own computer. Gmail and Google Calendar
accounts talk to Google directly, and Microsoft accounts talk to Microsoft
Graph, so no other server sees your mail.

Iris is a fork of [Penguin Mail](https://github.com/c9dev/penguin-mail), a
GTK and libadwaita mail client for Linux. Iris keeps Linux working and adds
macOS as a first-class platform.

## Status

| Platform | State |
|---|---|
| Linux | Works, as Penguin Mail 1.0 does. Builds from source only for now: Iris publishes no packages yet. |
| macOS | In progress. The sync, store and protocol crates build and pass their tests; the app itself needs the macOS port of the tray, login item, updater and keychain before it runs. |

Iris signs in to Google and Microsoft with its own OAuth clients, which are
not set up yet. Until they are, sign in with an IMAP account, or build with
your own clients as [Build](#build) describes.

## Features

Iris has mail for Gmail, Microsoft (Outlook.com, Hotmail, Live and
Microsoft 365), IMAP and POP3 accounts, a calendar, contacts, OpenPGP and
S/MIME, rules, and an optional assistant. An IMAP or POP3 account gets its
calendar and contacts over CalDAV and CardDAV where the provider offers them,
and an IMAP account its rules over ManageSieve.

### Reading

- **One inbox for every account**, plus each account's Inbox, Flagged,
  Sent, Drafts and labels. A colored dot tells accounts apart.
- **Conversations in one view.** Older messages fold down to a line, and
  quoted text and signatures are dimmed. HTML mail renders in its own
  sandbox with scripts off and remote images blocked until you ask for them.
- **Categories.** A bar above the inbox splits it into Primary, Updates,
  Promotions and Social, using Gmail's own categories. Categorize Sender
  moves a sender to another category for good.
- **Gmail search** with its full query syntax, across one account or all,
  with suggestions for subjects, people and labels as you type.
- **Junk, Trash and All Mail**, for every account or one, read live from
  Gmail. Delete Forever asks you to confirm each time, and needs the
  permission signing in already asked for.

### Writing

- **A composer that shows formatting as you write.** Recipients are chips,
  Cc and Bcc stay hidden until you want them, and attachments list their
  sizes. Replies and forwards thread correctly in Gmail, and drafts save to
  Gmail with their formatting, so they follow you to your phone.
- **Markdown when you want it.** Turn Markdown into Formatting styles the
  Markdown in the body, and Write in Markdown goes back. Paste, drop or
  insert images into the text.
- **Recipient suggestions** from your contacts and the people you have
  written to or heard from.
- **Undo Send and Send Later.** Sent mail waits a few seconds with an Undo
  button. Send Later schedules a message, which goes out on time while
  Iris runs, even in the tray.
- **An Outbox.** A message that cannot go out waits on this computer through
  a quit and a restart, and Iris tries again on a widening interval
  and as soon as the network comes back. Problems another try would not fix,
  such as a refused recipient or a message over Gmail's size limit, come
  back to you instead.
- **Templates** you drop in at the cursor, and a spelling check while you
  write.

### Organizing

- **Select several at once** with Ctrl+click, Shift+click or Ctrl+A, then
  archive, trash, junk, flag, mark or label them together. Ctrl+Z undoes
  each one.
- **One message at a time.** Right-click a message inside a conversation
  to reply to it, archive it, trash it, mark it, flag it, label it or
  export it on its own. The rest of the thread stays where it is.
- **Flags in seven colors**, as in Apple Mail. The flag syncs through
  Gmail's star, and the color stays on this computer.
- **VIPs.** Their mail gathers in a VIPs mailbox, their rows get a star, and
  notifications can be limited to them.
- **Smart Mailboxes**: saved conditions such as sender, subject, label, age,
  size or attachments, for all accounts or one. They search Gmail, so they
  reach past the mail kept on this computer.
- **Remind Me** takes a conversation out of the inbox and brings it back,
  unread, when you choose. **Follow Up** lists mail you sent that has had no
  answer for three days. **Mute** keeps a noisy thread out of the inbox.
- **Labels** from the toolbar or with `l`, nested as a tree under their
  account. Drag mail onto any mailbox or label to move it there.
- **Export** a conversation or a selection as mbox, or one message as `.eml`.

### Gmail settings

- **Rules**: Gmail's filters, listed in plain words, with a form to add one.
- **Automatic replies** with a subject, message and optional dates.
- **Unsubscribe and Block Sender.** List mail shows an Unsubscribe banner
  that uses the list's one-click link when it has one.
- **Hide My Email.** Make a plus address, such as
  `you+kelp.ember795@gmail.com`, for each site you sign up to, and turn it
  off to send its mail to the Trash. Your real address stays visible inside
  it, so this stops lazy spam, not a determined sender.

Gmail runs all four, so they work with your computer off.

### Calendar

- **A calendar beside your mail.** Every calendar on every account, by
  day, week, month or a scrolling list, read from this computer.
  Answer Yes, Maybe or No on an invitation from its event, and search
  across every account's events.

### Security and privacy

- **OpenPGP and S/MIME through your own GnuPG.** A signed message names its
  signer and says how far your trust database or the certificate chain
  vouches for it. An encrypted message opens and says it arrived that way.
  The composer has one Sign and one Encrypt for both standards and offers
  Encrypt once every recipient has a key. A Bcc stays blind under OpenPGP,
  which leaves that reader's key out of the message; S/MIME cannot, so a
  message with a Bcc is not encrypted under it. A draft of an encrypted
  message waits in Gmail encrypted to your own key, and opens again in the
  composer with Encrypt on. Iris holds no key and asks for no
  passphrase: gpg, gpgsm and their pinentry do.
- **Remote content blocked twice**, by a WebKit content filter and by the
  page's own Content-Security-Policy, with JavaScript off. Loading images is
  a choice per conversation, or per sender.
- **Invitations** show as a card above the message, and you can accept,
  decline or propose another time.

### The assistant

An assistant pane (Ctrl+J) summarizes, sorts, cleans up, drafts replies and
changes settings such as an automatic reply, using the app's own tools. It
also reads and changes your Google Calendar, finds free time, looks people
up in your contacts, and reads attachments.

- **Any model, per job.** It runs on a local model (LM Studio, Ollama, or
  any OpenAI-compatible server), an Anthropic API key, or your Claude
  subscription through Claude Code, and translation can use a different
  model from the assistant.
- **You see the work.** Each answer shows the model's thinking and every
  tool it ran, folded up until you open them, and a line saying what it is
  doing right now.
- **Web search**, through Claude's own search or, for a local model, Brave
  Search or your own SearXNG.
- **MCP servers** you add give it more tools, and **skills** teach it your
  routines. A skill's scripts run in a sandbox with no access to your mail,
  keys or home folder.

It asks before it sends mail, changes Gmail settings or your calendar, or
uses a tool from outside the app, and it is off until you pick a model.
[docs/assistant.md](docs/assistant.md) covers setup.

### On the desktop

- **Tray and notifications.** An unread count in the tray, and new-mail
  notifications with Archive, Mark Read, Delete and Reply buttons.
- **Light on memory.** In the tray Iris uses about 55 MB. A minute
  after you close the window, it restarts itself in the background to give
  back the memory the window used.
- **Apple Mail's shortcuts** with Ctrl in place of Command, plus Gmail's
  single keys.
- **English and European Portuguese**, chosen in Preferences, with the
  window's controls named for screen readers.


## Build

You need Rust 1.98.

On Linux, install the development packages, then build and install into
`~/.local`:

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev libwebkitgtk-6.0-dev libglib2.0-dev-bin gettext
scripts/install.sh
```

On macOS the app does not run yet (see [Status](#status)). The crates
without a window build and test with:

```sh
cargo test --workspace --exclude mailrs
```

To sign in to Google and Microsoft, compile your own OAuth clients in:

```sh
export IRIS_GOOGLE_CLIENT_ID=...
export IRIS_GOOGLE_CLIENT_SECRET=...
export IRIS_MICROSOFT_CLIENT_ID=...
```

[docs/setup.md](docs/setup.md#building-your-own-copy) says how to create
them. [CONTRIBUTING.md](CONTRIBUTING.md) has the rest of the build and the
layout of the crates.

## Usage

### Try it without an account

```sh
iris --demo
```

The demo opens three sample accounts in a throwaway store. Search, triage,
the composer and attachments all work against sample data, and nothing
talks to Google.

### Keyboard

Apple Mail's shortcuts work with Ctrl in place of Command. Gmail's single
keys work whenever you are not typing. `Ctrl+?` lists every shortcut.

| Key | Action | Key | Action |
|---|---|---|---|
| `j` / `k` | Next / previous conversation | `Ctrl+R` or `r` | Reply |
| `Ctrl+Alt+A` or `e` | Archive | `Ctrl+Shift+R` or `a` | Reply all |
| `Delete` or `#` | Move to trash | `Ctrl+Shift+F` or `f` | Forward |
| `Ctrl+Shift+J` | Junk | `Ctrl+N` or `c` | New message |
| `Ctrl+Shift+L` or `s` | Flag or unflag | `Ctrl+Shift+D` | Send |
| `Ctrl+Alt+1` to `Ctrl+Alt+7` | Flag color | `Ctrl+Shift+A` | Attach files |
| `Ctrl+Shift+U` or `u` | Mark read or unread | `Ctrl+B` / `Ctrl+I` / `Ctrl+K` | Bold, italic, link |
| `Ctrl+Alt+M` or `l` | Labels | `Ctrl+F` or `/` | Search |
| `Ctrl+Z` | Undo | `Ctrl+1` to `Ctrl+9` | Open a mailbox |
| `Ctrl+A` | Select all | `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Text size |
| `Ctrl+Shift+N` or `F5` | Check for mail | `Ctrl+P` | Print |
| `Ctrl+O` or double-click | Open in a new window | `Ctrl+Alt+U` | View source |

### Command line

```sh
iris --background             # start in the tray, no window
iris --compose                # new message
iris mailto:ann@example.com   # new message to Ann
iris --version
```

To make Iris open `mailto:` links:

```sh
xdg-mime default io.github.AlbertoBarrago.Iris.desktop x-scheme-handler/mailto
```

The running app answers D-Bus actions, for custom shortcuts:

```sh
gdbus call --session --dest io.github.AlbertoBarrago.Iris --object-path /io/github/AlbertoBarrago/Iris \
    --method org.gtk.Actions.Activate show-window [] {}
```

The actions are `show-window`, `hide-window`, `compose`, `check` and `quit`.

`iris-cli` drives the same sync core without a window: `account add`,
`sync`, `threads`, `show`, `triage` and `export`.

## Privacy

- Iris talks to Google's APIs straight from your computer. No
  Iris server sits in between.
- Refresh tokens live in the system keyring. The config file holds sync
  settings and, for accounts added through the old setup page, their Google
  client ID and secret, readable by you alone.
- Mail is cached in `~/.local/share/iris`: the last 30 days plus
  everything in your inbox. Opening an older thread fetches it on demand.
- The assistant is off until you pick a model. A local model keeps mail on
  your computer; the Anthropic API and Claude Code send what the assistant
  reads to Anthropic. API keys live in the system keyring.

The full policy is in [docs/privacy-policy.md](docs/privacy-policy.md).


## Roadmap

1. **macOS build.** Platform gates for the tray, login item, updater and
   packaging; secrets in the macOS Keychain.
2. **macOS bundle.** An `Iris.app` with its GTK libraries inside, and a
   macOS job in CI.
3. **Native touches.** A menu bar item, a login item through
   `SMAppService`, Command shortcuts and the application menu.
4. **Own identity.** An Iris icon and artwork, Google and Microsoft OAuth
   clients, and a privacy policy of its own.
5. **Device sync without a cloud.** Rules, settings and the local store
   kept in step between your own computers, peer to peer.

## Help and feedback

Questions, bug reports and ideas are welcome in
[Issues](https://github.com/AlbertoBarrago/iris/issues).
[CONTRIBUTING.md](CONTRIBUTING.md) says what a useful report holds.
[SECURITY.md](SECURITY.md) says where to send a vulnerability.

## Based on Penguin Mail

Iris is a modified version of
[Penguin Mail](https://github.com/c9dev/penguin-mail) by c9dev, forked from
its 1.0.0 release. The original work and its copyright belong to its
authors; the changes since the fork are Iris's. Fixes from Penguin Mail are
merged back where they apply, and the release history before the fork is in
[CHANGELOG.md](CHANGELOG.md).

The app icon and artwork still come from Penguin Mail and will be replaced.

## License

Iris is free software under the
[GNU General Public License, version 3 or later](LICENSE), the same license
as Penguin Mail.
