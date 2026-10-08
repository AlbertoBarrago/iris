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
| macOS | Works. An `Iris.app` with a Dock badge for unread mail, a menu bar, Command shortcuts and secrets in the Keychain. Test builds come as a DMG; see [Install on macOS](#install-on-macos). |
| Linux | Works, as Penguin Mail 1.0 does. Builds from source only for now: Iris publishes no packages yet. |

A build signs in to Google and Microsoft only with OAuth clients compiled
in. The test DMGs carry Iris's own; a build from source needs yours, as
[Build](#build) describes. Without one, sign in with an IMAP account.

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

- **Select several at once** with Command-click (Ctrl+click on Linux),
  Shift-click or ⌘A, then archive, trash, junk, flag, mark or label them
  together. ⌘Z undoes each one.
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
- **A sidebar you arrange.** Fold Favorites, Mailboxes, Smart Mailboxes or
  Accounts by clicking the title, and each account under its heading.
  Iris remembers what you left open.
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

An assistant pane (⌘J, or Ctrl+J on Linux) summarizes, sorts, cleans up, drafts replies and
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

- **On macOS**, the unread count on the Dock icon, the app's menu in the
  menu bar, and Preferences in a window of its own. Closing the window
  keeps Iris running, and a click on the Dock icon brings it back.
- **On Linux**, an unread count in the tray, and new-mail notifications
  with Archive, Mark Read, Delete and Reply buttons. In the tray Iris
  uses about 55 MB: a minute after you close the window, it restarts
  itself in the background to give back the memory the window used.
- **Panes you size.** Drag the edge of the mailboxes, the message list or
  the assistant; a double click puts it back. Iris remembers the widths and
  the window's size.
- **Apple Mail's shortcuts**, with Command on macOS and Ctrl on Linux, plus
  Gmail's single keys.
- **English, European Portuguese and Italian**, chosen in Preferences, with
  the window's controls named for screen readers.


## Install on macOS

Test builds come as `Iris-<version>.dmg`. Open it and drag Iris onto
Applications.

The DMG is signed with the author's own certificate, not yet with an Apple
Developer ID, so macOS stops Iris the first time. Open it once, then go to
**System Settings > Privacy & Security** and choose **Open Anyway** beside
Iris. From then on it opens like any other app.

## Build

You need Rust 1.98.

On macOS, install GTK and libadwaita from Homebrew, then build and run, or
install into `/Applications`:

```sh
brew install gtk4 libadwaita adwaita-icon-theme librsvg gettext pkgconf
scripts/dev-macos.sh              # build the debug app and run it
scripts/dev-macos.sh --demo       # the same, on sample accounts
scripts/macos-install.sh          # build a release and install Iris.app
```

`macos-install.sh` builds an app that loads GTK from Homebrew, so it runs on
the Mac that built it. Short on disk, build with
`CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`.

On Linux, install the development packages, then build and install into
`~/.local`:

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev libwebkitgtk-6.0-dev libglib2.0-dev-bin gettext
scripts/install.sh
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

Apple Mail's shortcuts work as they do in Mail. The table writes them as a
Mac does; on Linux, Ctrl takes ⌘'s place, Alt ⌥'s and Shift ⇧'s. Gmail's
single keys work whenever you are not typing. ⌘? lists every shortcut.

| Key | Action | Key | Action |
|---|---|---|---|
| `j` / `k` | Next / previous conversation | `⌘R` or `r` | Reply |
| `⌥⌘A` or `e` | Archive | `⇧⌘R` or `a` | Reply all |
| `Delete` or `#` | Move to trash | `⇧⌘F` or `f` | Forward |
| `⇧⌘J` | Junk | `⌘N` or `c` | New message |
| `⇧⌘L` or `s` | Flag or unflag | `⇧⌘D` | Send |
| `⌥⌘1` to `⌥⌘7` | Flag color | `⇧⌘A` | Attach files |
| `⇧⌘U` or `u` | Mark read or unread | `⌘B` / `⌘I` / `⌘K` | Bold, italic, link |
| `⌥⌘M` or `l` | Labels | `⌘F` or `/` | Search |
| `⌘Z` | Undo | `⌘1` to `⌘9` | Open a mailbox |
| `⌘A` | Select all | `⌘=` / `⌘-` / `⌘0` | Text size |
| `⇧⌘N` or `F5` | Check for mail | `⌘P` | Print |
| `⌘O` or double-click | Open in a new window | `⌥⌘U` | View source |

### Command line

```sh
iris --background             # start in the tray, no window
iris --compose                # new message
iris mailto:ann@example.com   # new message to Ann
iris --version
```

To make Iris open `mailto:` links on Linux:

```sh
xdg-mime default io.github.AlbertoBarrago.Iris.desktop x-scheme-handler/mailto
```

On Linux, the running app answers D-Bus actions, for custom shortcuts:

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
- Refresh tokens live in the macOS Keychain or the Linux keyring. The config file holds sync
  settings and, for accounts added through the old setup page, their Google
  client ID and secret, readable by you alone.
- Mail is cached in `~/Library/Application Support/iris` on macOS and
  `~/.local/share/iris` on Linux: the last 30 days plus everything in your
  inbox. Opening an older thread fetches it on demand.
- The assistant is off until you pick a model. A local model keeps mail on
  your computer; the Anthropic API and Claude Code send what the assistant
  reads to Anthropic. API keys live in the system keyring.

The full policy is in [docs/privacy-policy.md](docs/privacy-policy.md).


## Roadmap

Done: the macOS app with its menu bar, Dock badge, Command shortcuts and
Keychain; the winged-envelope identity; Google and Microsoft clients; the
Italian translation.

1. **A DMG that runs on any Mac**, with GTK and its libraries inside the
   bundle.
2. **Updates through Sparkle**, from an appcast on albz.it.
3. **Start at login on macOS** through `SMAppService`; the switch in
   Preferences only works on Linux today.
4. **A Developer ID and notarization**, so the DMG opens without the
   Privacy & Security step.
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

Iris has its own icon and artwork, a winged envelope; the penguin is
Penguin Mail's.

## License

Iris is free software under the
[GNU General Public License, version 3 or later](LICENSE), the same license
as Penguin Mail.
