# Changelog

Each release of Iris, newest first. GitHub shows the same text on the
[releases page](https://github.com/AlbertoBarrago/iris/releases).

## Unreleased

### New

- Expand a conversation from its message count to preview each reply in the
  list, then open the message you want to read.

### Fixed

- Delete moves selected mail to the Trash from the message list or reader,
  instead of playing the system alert sound.
- Text pasted into the composer takes Iris's font and text color, including
  plain text, and typing after it keeps the same style.

## 1.1.13 (2026-10-15)

### Improved

- In Italian, All Inboxes now reads "In arrivo (tutti)", short enough for the
  sidebar.

## 1.1.12 (2026-10-14)

### New

- An Unread mailbox in the sidebar gathers unread mail from every folder, for
  all accounts together and for each one.
- Show Only Unread (⌘L, or the filter button over the list) narrows any
  mailbox to its unread mail. A conversation you open stays in the list
  while you read it.

## 1.1.11 (2026-10-14)

### Fixed

- A reply sent from a Microsoft account shows in its conversation within
  seconds, instead of up to five minutes later.

## 1.1.10 (2026-10-14)

### New

- Always load images from a sender, or from anyone at their domain, from the
  menu on Load Images.
- Iris's icon turns black when macOS shows icons dark, and follows the clear
  and tinted icon styles too.

### Fixed

- Text pasted from a web page or a chat takes the composer's own colors and
  font instead of staying black, and keeps its bold, italics and links.
- A conversation is drawn light when Style is set to Light on a Mac in dark
  mode, and dark when it is set to Dark on a light one.

## 1.1.9 (2026-10-13)

### New

- Take an account's color dot away from its menu, and a single account no
  longer shows one.

### Improved

- Report a problem from the Help menu.
- The About window is shorter: the star on GitHub, then the credits.

## 1.1.8 (2026-10-12)

### New

- Reply, Reply All or Forward any message of a conversation from the buttons
  on its header, not only the newest.

## 1.1.7 (2026-10-12)

### New

- Conversations and events the assistant mentions open with a click: the
  conversation in your mail, the event on your calendar.
- Drag files onto a message you are writing to attach them, from Finder, the
  desktop, Photos, Mail or a picture in another app, including onto the text.

### Improved

- A message you forward stays in the conversation it came from, as in Mail
  and Gmail, instead of starting a new one.
- Every icon button says what it does when you point at it, so Reply, Reply
  All and Forward no longer look alike.

### Fixed

- When a newer Iris has updated your mail, an older copy says so in plain
  words and offers to check for updates, instead of naming schema versions.
- Discard closes the message and says it was discarded, or why the draft
  saved of it stays in Drafts.
- New Message always opens a new, empty message, also while a reply is
  open, and each press opens a window of its own.
- Reply answers the newest message that is not your own unsent draft, and
  goes to the address a message asks replies to go to (Reply-To), as Mail
  does.
- Send on a message with no recipient, or with an address that is not one,
  says so under the recipients and keeps the message open, instead of
  closing it and bringing it back a few seconds later.
- Mail you send and drafts you save show up in Sent and Drafts right away on
  Microsoft and IMAP accounts, instead of a few minutes later.

## 1.1.6 (2026-10-12)

### Fixed

- The assistant can write, save and send mail again, instead of saying the
  mail window closed.

## 1.1.5 (2026-10-12)

### Fixed

- Typing in the search field no longer loses letters or the cursor when
  suggestions appear.

## 1.1.4 (2026-10-12)

### Fixed

- Adding an account works again, and no longer stops your other accounts
  from syncing until Iris is restarted.

## 1.1.3 (2026-10-12)

### New

- Translating a message happens on your Mac by default, with Apple's own
  translator, so the message never leaves it. A model can still be chosen
  in Settings, AI, Used For.

## 1.1.2 (2026-10-12)

### New

- Help > Keyboard Shortcuts (⇧⌘/) lists every key Iris answers.

### Fixed

- Iris opens on a Mac it never ran on; a first install stopped at
  "unable to open database file".
- Some mail, such as an invitation with many events, no longer stretches
  the window to the height of the screen.

## 1.1.1 (2026-10-12)

### Fixed

- A conversation moved from Follow Up to the Trash on a Microsoft or IMAP
  account leaves Follow Up; it used to stay there.

## 1.1.0 (2026-10-12)

### New

- Iris is now a native Mac app, built with SwiftUI over the same core. It
  updates from the version you have and keeps your accounts, mail and
  settings.
- Open at Login, in Settings, through macOS's Login Items.
- Contact cards: click a sender to see their photo, addresses and phone,
  write to them, make them a VIP or see all their mail.
- After sending, Iris offers to save new recipients to your contacts.
- The About window has a short poem for Iris and a way to star it on
  GitHub.
- How long notices stay in the window's corner can be chosen in Settings.
- A company signature with logos, badges and a table can be pasted or
  imported in Settings, and goes out looking the way it was made.

### Improved

- The interface comes in English and Italian; the Portuguese
  translation is gone.
- Search suggestions show the address when one name writes from
  several, such as a shop's info@ and no-reply@.

### Fixed

- Send Later works on Microsoft accounts: a scheduled message used to
  fail at its hour and wait in the Outbox.
- Search keeps working when an account cannot connect, such as while it
  waits for the Keychain; it used to stop answering after the first one.
- A Gmail signature with pictures or formatting no longer goes out as
  plain lines with its pictures missing.
- Attached files go out with their exact type, such as `text/plain` for
  a text file, so the recipient's mail app opens them right.
- The assistant can attach files from temporary folders and external
  disks.

## 1.0.11 (2026-10-08)

### Fixed

- On macOS, a trackpad scrolls an open message, as the mouse wheel does.

### Improved

- On macOS, the notices over a message, such as remote images hidden or
  a mailing list's Unsubscribe, float as rounded islands.

## 1.0.10 (2026-10-08)

### Fixed

- An email link fills in its subject, text, Cc and Bcc as well as the
  address, instead of opening a message with the address alone.

## 1.0.9 (2026-10-08)

### New

- On macOS, Iris opens email links from websites and other apps in a new
  message: choose Make Default under Default Mail App in Preferences.

### Improved

- On macOS, ⌘⌫ deletes to the start of the line and ⌥⌫ the word before
  the cursor in every text field and the composer, as in other Mac apps.

## 1.0.8 (2026-10-08)

### Fixed

- On macOS, the open message stays on screen around a dialog, such as
  Add Account or a confirmation, instead of disappearing until it closes.

## 1.0.7 (2026-10-08)

### Fixed

- On macOS, About Iris opens in a small window of its own, and the open
  message stays on screen behind it instead of disappearing.

## 1.0.6 (2026-10-08)

### Fixed

- On macOS, a message stays whole when the window goes full screen or
  grows, instead of losing a corner to the window's background.

## 1.0.5 (2026-10-08)

### Fixed

- On macOS, closing the window with the red button no longer leaves Iris
  stuck now and then, with neither the window nor the Dock answering.
- The composer's From, To and Subject labels sit level with the words
  beside them.

## 1.0.4 (2026-10-08)

### Fixed

- The arrow beside Send in the composer lights up inside the button's
  rounded end, instead of as a square past it.
- On macOS, the menu a right click opens on a message shows every item
  and its whole title, instead of opening short with the rest to scroll.
- The macOS installer no longer stops with an xattr error before it
  opens Iris.

## 1.0.3 (2026-10-08)

### Improved

- On macOS, the mailboxes take about 100 pixels less: the Mail and
  Calendar switch above them shows its icons, with the names in a
  tooltip, so the message list keeps the mailboxes beside it longer as you
  widen it.

## 1.0.2 (2026-10-08)

### Fixed

- On macOS, Check for Updates in Preferences and in About Iris now asks
  the update feed, as the Iris menu does, and the daily check follows the
  switch in Preferences.

### Improved

- The mailboxes keep a fixed width; the message list and the assistant
  still resize.

## 1.0.1 (2026-10-08)

The first release of Iris for macOS.

### New

- Fold a sidebar section, such as Favorites or Accounts, by clicking its
  title. Iris remembers which sections and accounts you left open.
- Iris speaks Italian: choose Italiano under Language in Preferences.
- On macOS, the app carries its translations, so Português and
  Italiano work there too.
- Iris for macOS comes as a DMG that carries everything it needs, so it
  installs on a Mac without Homebrew, and keeps itself up to date: choose
  Check for Updates… in the Iris menu, or let it look once a day.
- Drag the edge of the mailboxes, the message list or the assistant to
  make it wider or narrower; a double click puts it back. Iris remembers
  the widths and the window's size for the next start.

### Improved

- On macOS, Preferences opens in a window of its own, which you can move
  and resize.
- On macOS, shortcuts take Command where Linux takes Control, and
  tooltips show them in the Mac's own symbols, such as ⇧⌘J for Junk.

### Fixed

- On macOS, a click on a new-mail notification brings Iris forward and
  opens the conversation.
- On macOS, closing the window with the red button no longer freezes
  Iris: a click on the Dock icon brings the window back as you left it.
- On macOS, a dialog opened over an invitation no longer shows a dark
  block where the invitation's card sits under it.
