<div align="center">

# Iris

Mail and calendar for macOS, written in Rust.

[![CI](https://github.com/AlbertoBarrago/iris/actions/workflows/ci.yml/badge.svg)](https://github.com/AlbertoBarrago/iris/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)
[![Rust 1.98](https://img.shields.io/badge/rust-1.98-orange?logo=rust)](https://www.rust-lang.org)
[![GTK 4 and libadwaita 1.8](https://img.shields.io/badge/GTK_4-libadwaita_1.8-4a86cf?logo=gnome)](https://gnome.pages.gitlab.gnome.org/libadwaita/)

[Status](#status) · [Features](#features) · [Install](#install) · [Build](#build) · [Usage](#usage)

</div>

Iris is mail and calendar for your Mac. It reads Gmail accounts,
Microsoft accounts (Outlook.com, Hotmail, Live and Microsoft 365) and any
IMAP and SMTP account, such as Fastmail, iCloud and Yahoo, and finds the
server settings for you. It shows your accounts in one inbox or one at a
time, and keeps your mail on your own computer. Gmail and Google Calendar
accounts talk to Google directly, and Microsoft accounts talk to Microsoft
Graph, so no other server sees your mail.

## Status

Iris runs on a Mac with Apple silicon and macOS 26 or later, as an
`Iris.app` with a Dock badge for unread mail, a menu bar, Command shortcuts
and secrets in the Keychain. Builds come as a DMG and update themselves
through Sparkle; see [Install](#install).

A few features are not on macOS yet: a skill's scripts (the sandbox they
run in is Linux's bubblewrap), the Archive, Mark as Read, Delete and Reply
buttons on new-mail notifications, and starting at login.

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
  Iris runs, even with its window closed.
- **An Outbox.** A message that cannot go out waits on this computer through
  a quit and a restart, and Iris tries again on a widening interval
  and as soon as the network comes back. Problems another try would not fix,
  such as a refused recipient or a message over Gmail's size limit, come
  back to you instead.
- **Templates** you drop in at the cursor, and a spelling check while you
  write.

### Organizing

- **Select several at once** with Command-click, Shift-click or ⌘A, then archive, trash, junk, flag, mark or label them
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

An assistant pane (⌘J) summarizes, sorts, cleans up, drafts replies and
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
  keys or home folder; that sandbox is not on macOS yet, so scripts do not
  run there, while a skill's instructions do.

It asks before it sends mail, changes Gmail settings or your calendar, or
uses a tool from outside the app, and it is off until you pick a model.
[docs/assistant.md](docs/assistant.md) covers setup.

### On the Mac

- **The unread count on the Dock icon**, the app's menu in the menu bar,
  and Preferences in a window of its own. Closing the window keeps Iris
  running, and a click on the Dock icon brings it back.
- **New-mail notifications.** A click opens the conversation; the Archive,
  Mark as Read, Delete and Reply buttons are not on macOS yet.
- **Panes you size.** Drag the edge of the message list or the assistant;
  a double click puts it back. Iris remembers the widths and the window's
  size.
- **Apple Mail's shortcuts**, plus Gmail's single keys.
- **English, European Portuguese and Italian**, chosen in Preferences, with
  the window's controls named for screen readers.

## Install

Builds are for a Mac with Apple silicon and macOS 26 or later. The
quickest way in is the installer, which fetches the newest build, puts it in
Applications and opens it:

```sh
curl -fsSL https://albz.it/iris/install.sh | sh
```

The DMG is also on the
[releases page](https://github.com/AlbertoBarrago/iris/releases).

The builds are signed with the author's own certificate, not yet with an
Apple Developer ID. A copy installed this way opens at once; a DMG
downloaded in a browser carries macOS's quarantine mark, and macOS stops
Iris until you choose **Open Anyway** in **System Settings > Privacy &
Security**, or clear the mark yourself:

```sh
xattr -cr /Applications/Iris.app
```

Iris then updates itself through Sparkle: it checks once a day, and
**Check for Updates…** in the Iris menu checks at once.

## Build

You need Rust 1.98 (`cargo +1.98` when your default toolchain is older).
Install GTK and libadwaita from Homebrew, then build and run, or install
into `/Applications`:

```sh
brew install gtk4 libadwaita adwaita-icon-theme librsvg gettext pkgconf
scripts/dev-macos.sh              # build the debug app and run it
scripts/dev-macos.sh --demo       # the same, on sample accounts
scripts/macos-install.sh          # build a release and install Iris.app
```

`macos-install.sh` builds an app that loads GTK from Homebrew, so it runs on
the Mac that built it. `scripts/macos-dmg.sh` builds the test DMG instead,
in `target/dmg/`: the app carries GTK and its libraries inside, so it runs
on a Mac without Homebrew, on the macOS Homebrew built them for or later.
Short on disk, build with
`CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`.

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
scripts/dev-macos.sh --demo
/Applications/Iris.app/Contents/MacOS/iris --demo   # an installed copy
```

The demo opens three sample accounts in a throwaway store. Search, triage,
the composer and attachments all work against sample data, and nothing
talks to Google.

### Keyboard

Apple Mail's shortcuts work as they do in Mail. Gmail's single keys work
whenever you are not typing. ⌘? lists every shortcut.

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

The app's binary is `Iris.app/Contents/MacOS/iris`:

```sh
iris --background             # start without opening a window
iris --compose                # new message
iris mailto:ann@example.com   # new message to Ann
iris --version
```

`Iris.app` registers for `mailto:` links. To make it the app that opens
them, choose it as the default email reader in Mail's settings.

`iris-cli` drives the same sync core without a window: `account add`,
`sync`, `threads`, `show`, `triage` and `export`.

## Privacy

- Iris talks to Google's APIs straight from your computer. No
  Iris server sits in between.
- Refresh tokens live in the macOS Keychain. The config file holds sync
  settings and, for accounts added through the old setup page, their Google
  client ID and secret, readable by you alone.
- Mail is cached in `~/Library/Application Support/iris`: the last 30 days plus everything in your
  inbox. Opening an older thread fetches it on demand.
- The assistant is off until you pick a model. A local model keeps mail on
  your computer; the Anthropic API and Claude Code send what the assistant
  reads to Anthropic. API keys live in the Keychain.

The full policy is in [docs/privacy-policy.md](docs/privacy-policy.md).

## Roadmap

Done: the macOS app with its menu bar, Dock badge, Command shortcuts and
Keychain; a DMG that carries its own GTK and updates through Sparkle; the
winged-envelope identity; Google and Microsoft clients; the Italian
translation.

1. **Start at login** through `SMAppService`.
2. **A Developer ID and notarization**, so the DMG opens without the
   Privacy & Security step.
3. **macOS before 26 and Intel Macs**, with GTK built for them rather
   than taken from Homebrew.
4. **Device sync without a cloud.** Rules, settings and the local store
   kept in step between your own computers, peer to peer.

## Help and feedback

Questions, bug reports and ideas are welcome in
[Issues](https://github.com/AlbertoBarrago/iris/issues).
[CONTRIBUTING.md](CONTRIBUTING.md) says what a useful report holds.
[SECURITY.md](SECURITY.md) says where to send a vulnerability.

## License

Iris is free software under the
[GNU General Public License, version 3 or later](LICENSE).

Iris includes code from
[Penguin Mail](https://github.com/c9dev/penguin-mail) by David Santos,
under the same license.
