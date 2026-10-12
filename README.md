<div align="center">

<img src="data/icons/scalable/apps/io.github.AlbertoBarrago.Iris.svg" width="128" height="128" alt="The Iris icon, a winged envelope">

# Iris

A native Mac app for mail and calendar, with a core written in Rust.

[![CI](https://github.com/AlbertoBarrago/iris/actions/workflows/ci.yml/badge.svg)](https://github.com/AlbertoBarrago/iris/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)
[![SwiftUI on macOS 26](https://img.shields.io/badge/SwiftUI-macOS_26-0a84ff?logo=apple)](https://developer.apple.com/xcode/swiftui/)
[![Rust 1.98](https://img.shields.io/badge/rust-1.98-orange?logo=rust)](https://www.rust-lang.org)

[Features](#features) · [Install](#install) · [Build](#build) · [Keyboard](#keyboard) · [Privacy](#privacy)

</div>

Iris is mail and calendar for your Mac. It reads Gmail accounts, Microsoft
accounts (Outlook.com, Hotmail, Live and Microsoft 365) and any IMAP, SMTP
or POP3 account, such as Fastmail, iCloud and Yahoo, and finds the server
settings for you. It shows your accounts in one inbox or one at a time,
and keeps your mail on your own computer. Gmail and Google Calendar talk
to Google directly, and Microsoft accounts to Microsoft Graph, so no other
server sees your mail.

The window is SwiftUI and AppKit; the work behind it, syncing, search,
signing, the calendar's copy and the assistant, is a Rust core the app
reaches through UniFFI. Iris runs on a Mac with Apple silicon and macOS 26
or later.

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
  Gmail. Delete in the Trash erases for good after asking.
- **Translation.** A message in another language offers to translate
  itself into yours, through the model you choose, and turns back with one
  click.

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
- **Templates** you drop in at the cursor, with placeholders such as
  `{{first_name}}` filled in for each message.
- **Save to contacts.** After sending, Iris offers to add new recipients to
  the account's contacts.

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
  size or attachments, for all accounts or one, from File > New Smart
  Mailbox.
- **Remind Me** takes a conversation out of the inbox and brings it back,
  unread, when you choose. **Follow Up** lists mail you sent that has had no
  answer for three days. **Mute** keeps a noisy thread out of the inbox.
- **Labels** from the toolbar or with `l`, nested as a tree under their
  account. Drag mail onto any mailbox or label to move it there.
- **A sidebar you arrange.** Fold Favorites, Mailboxes or Accounts, and
  each account under its heading. Iris remembers what you left open.
- **Contacts.** A click on a sender opens their card: photo, addresses,
  phone, a new message, the VIP switch and all their mail.
- **Sender actions.** Categorize Sender sorts someone's mail into a
  category for good, and Block Sender sends it to the Trash.
- **Export** conversations as mbox, and **View Source** (⌥⌘U) shows a
  message as it arrived, with Save As for an `.eml` file.

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
  day, week or month, read from this computer. Drag an event to move or
  resize it, create one by dragging on empty time, and answer Yes, Maybe
  or No on an invitation from its event.

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
- **Invitations** show as a card above the message with the hours around
  the event, and you can accept, decline or propose another time.

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

- **The unread count on the Dock icon**, the Mac's own menus, and Settings
  in a window of their own. Closing the window keeps Iris running, and a
  click on the Dock icon brings it back.
- **New-mail notifications**, which open the conversation they are about.
- **Open at Login**, through macOS's Login Items.
- **Mac shortcuts**, plus a few of Gmail's single keys, all listed under
  Help > Keyboard Shortcuts (⇧⌘/).
- **English and Italian**, chosen in Settings, with the window's controls
  named for screen readers.

## Install

The quickest way in is the installer, which fetches the newest build,
puts it in Applications and opens it:

```sh
curl -fsSL https://albz.it/iris/install.sh | sh
```

The DMG is also on the
[releases page](https://github.com/AlbertoBarrago/iris/releases) and at
[albz.it/iris](https://albz.it/iris/).

The builds are not signed with an Apple Developer ID yet. A copy the
installer puts in place opens at once; a DMG downloaded in a browser
carries macOS's quarantine mark, and macOS stops Iris until you choose
**Open Anyway** in **System Settings > Privacy & Security**, or clear the
mark yourself:

```sh
xattr -cr /Applications/Iris.app
```

Iris then keeps itself up to date through Sparkle: it checks once a day,
and **Iris > Check for Updates…** checks at once.

A build signs in to Google and Microsoft only with OAuth clients compiled
in. The DMGs carry Iris's own; a build from source needs yours, as
[Build](#build) describes. Without one, sign in with an IMAP account.

## Build

You need Xcode 26, Rust 1.98 (`cargo +1.98` when your default toolchain
is older) and XcodeGen, which writes the Xcode project from
`macos/project.yml`. Xcode builds the Rust core itself, through
`scripts/build-ffi.sh`:

```sh
brew install xcodegen gettext librsvg
cargo install xtr                 # for scripts/update-po.sh
scripts/dev-macos.sh              # build the debug app and run it
scripts/macos-install.sh          # build a release and install Iris.app
scripts/macos-dmg.sh              # build the DMG and appcast in target/dmg/
```

`project.yml` signs with the author's Apple Development team; set your own
`DEVELOPMENT_TEAM` there. Short on disk, build the core with
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

## Keyboard

Help > Keyboard Shortcuts (⇧⌘/) lists every key Iris answers. The main
ones:

| Key | Action | Key | Action |
|---|---|---|---|
| `⌥⌘A` or `e` | Archive | `⌘N` | New message |
| `Delete` | Move to trash | `⌘R` | Reply |
| `⇧⌘J` | Junk | `⇧⌘R` | Reply all |
| `⇧⌘U` or `u` | Mark read or unread | `⇧⌘F` | Forward |
| `⇧M` | Mute or unmute | `⌘↩` | Send |
| `l` | Labels | `⇧⌘A` | Attach files |
| `⌘Z` | Undo | `⌘B` / `⌘I` / `⌘K` | Bold, italic, link |
| `⌘F` | Search | `⌥1` / `⌥2` | Mail / Calendar |
| `⌘O` | Open in a new window | `⌘T` | Calendar: today |
| `⌥⌘U` | View source | `d` / `w` / `m` | Calendar: day, week, month |
| `⌘P` | Print | `⌘J` | The assistant |

### Command line

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

1. **Sample accounts** to try Iris without signing in.
2. **A Developer ID and notarization**, so the DMG opens without the
   Privacy & Security step.
3. **Device sync without a cloud.** Rules, settings and the local store
   kept in step between your own computers, peer to peer.

## Help and feedback

Questions, bug reports and ideas are welcome in
[Issues](https://github.com/AlbertoBarrago/iris/issues).
[CONTRIBUTING.md](CONTRIBUTING.md) says what a useful report holds.
[SECURITY.md](SECURITY.md) says where to send a vulnerability.

## License

Iris is free software under the
[GNU General Public License, version 3 or later](LICENSE). It is made by
[Alberto Barrago](https://github.com/AlbertoBarrago); if it makes your
day lighter, a star on GitHub is welcome.

### Where Iris comes from

Iris began as a fork of [Penguin Mail](https://github.com/c9dev/penguin-mail),
a GTK mail client for Linux, and kept its licence. It has since become its
own app: a native Mac app with a core rewritten and grown well beyond
where it started.
