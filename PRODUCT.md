# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Iris is a macOS app in Rust with GTK 4 and libadwaita, not a web app. The schema has no value for a GTK desktop
app, so this records the nearest one. Its styling is GTK CSS (`app/data/style.css`). libadwaita's own patterns and
the GNOME Human Interface Guidelines govern the design, not web conventions; the menu bar, the Dock and the
shortcuts follow macOS.

## Users

Privacy-minded power users on a Mac: people who want a keyboard-driven mail client with signing, encryption and an
assistant, and who would otherwise use Apple Mail, Thunderbird or Gmail in the browser. They keep several accounts,
read mail in more than one language, and expect the app to feel part of the system. The owner is one of them and
uses the app every day, reading the interface in English and Italian.

## Product Purpose

A beautiful and functional mail and calendar client for macOS. It reads, sorts and writes mail across several
accounts from a copy kept on the computer, with a calendar beside the mail. Success is people choosing it over the
browser and the older clients because it is both nicer to use and more capable.

## Positioning

- **At home on the Mac, and fast.** A libadwaita app with the Mac's menu bar, Dock badge and Command shortcuts,
  reading from a local copy so mail opens in milliseconds.
- **Private by default.** Remote images blocked until asked for, mail kept on this computer, OpenPGP and S/MIME
  through the person's own GnuPG, no tracking.
- **An assistant that acts.** It searches, sorts, drafts, unsubscribes and changes settings, asks before it acts,
  and works with any model, local ones included.
- **Gmail's features, natively.** Categories, labels, filters, automatic replies, send-as aliases and Google
  Calendar invitations without the browser.

## Operating Context

A Mac with Apple silicon and macOS 26 or later, with the app running on after its window closes and syncing in the
background. Installed from a DMG or the install script at https://albz.it/iris/, and updated through Sparkle. Gmail,
Microsoft, IMAP and SMTP, and POP3 accounts today, with CalDAV and CardDAV for calendars and contacts. Keyboard-only
use is supported; every control is named for screen readers, though VoiceOver does not read GTK's controls on
macOS yet.

## Capabilities and Constraints

- Mail: unified and per-account mailboxes, conversations, Gmail categories, labels, flags in colors, VIPs, Follow
  Up, Remind Me, Send Later with Undo Send, an outbox that works offline, templates, a rich-text composer,
  unsubscribe that finishes the list's own page, Hide My Email aliases, rules and automatic replies.
- Security: OpenPGP and S/MIME signing and encryption through GnuPG, revocation checks, signed updates through
  Sparkle.
- Assistant: models from Anthropic, OpenAI-compatible servers, local servers or a Claude subscription; MCP
  servers; skills, whose scripts do not run on macOS yet; every change asks first unless the person chose Always
  Allow.
- Not on macOS yet: buttons on new-mail notifications, and starting at login.
- Languages: American English source strings, British English generated from them, European Portuguese and
  Italian kept complete.
- Terms are defined in `CONTEXT.md`; the code's rules are in `AGENTS.md`.

## Brand Commitments

- **The icon idiom.** The app icon is a winged envelope: a cream envelope in a night-violet frame with an
  iridescent wing rising from behind its top-left corner, set on a macOS squircle. `scripts/iris-art.py` draws it
  and the Add Account poses, and new artwork follows that idiom.
- **The accent is the system's.** The app follows the accent color set in macOS's System Settings, through
  libadwaita, and has no accent setting of its own. Mockups and screenshots use orange.
- **Plain-spoken voice.** Every string, comment, doc and commit follows the stop-slop and unslop rules: plain
  statements, no filler, no hype.
- **libadwaita first.** libadwaita patterns and the GNOME Human Interface Guidelines win over custom interface,
  and macOS's conventions win for the menu bar, the Dock and the shortcuts.

## Evidence on Hand

- Screenshots of the real app in `docs/screenshots/` (inbox, dark, categories, composer, assistant, flags, VIPs,
  selection, send later, rules, preferences, phone width, welcome, Hide My Email, automatic reply).
- Approved mockups of the calendar and the refreshed mail screen in `docs/superpowers/specs/calendar-mockup/`.
- No user counts, reviews, testimonials, press or benchmarks against other clients exist. Do not invent any.

## Product Principles

1. **Belong on the Mac.** Follow libadwaita's patterns before inventing new ones, and the Mac's where the system
   sets them; the app should feel like part of the system.
2. **The person's mail stays theirs.** Keep it on the computer, load nothing remote without asking, and never let
   the assistant act without consent.
3. **Fast because it is local.** Read from the copy on disk; the network is for keeping it fresh, not for showing
   what is already known.
4. **Capable without clutter.** Power features sit one step deeper than the common path, and undo beats
   confirmation.
5. **Say what happened.** Every message names the actual thing that went wrong and what the person can do next.

## Accessibility & Inclusion

Every control has an accessible name, though VoiceOver does not read GTK's controls on macOS yet. Keyboard
shortcuts cover the main actions and are listed in the Keyboard Shortcuts dialog. Motion follows the system's
animations setting. The interface ships in American English, British English, European Portuguese and Italian. Details in
`docs/accessibility.md`.
