# Accessibility

What a screen reader gets from Iris, how to check it, and how a new
control stays reachable.

Iris is a SwiftUI and AppKit app, so VoiceOver reads its controls the way
it reads any Mac app's. Until 1.1 the app was GTK, which VoiceOver could
not read on macOS; this file describes the Swift app.

## Names

Every control a person acts on has a name VoiceOver reads.

- A `Button` or `Toggle` with a text label is named by its label. One
  drawn as an icon only (`.labelStyle(.iconOnly)`, a `systemImage` with no
  words beside it) still gets its words through `Label`, so a screen
  reader hears them; give it a `.help` too, which the pointer shows.
- A control whose visible words do not say what it does gets an
  `.accessibilityLabel`, such as the close button on a notice or the
  Receive Mail switch beside each Hide My Email address.
- A card that reads as one thing, such as the signature card, the
  translation card or the day strip around an invitation, combines its
  children (`.accessibilityElement(children: .combine)`) and names the
  whole, so VoiceOver reads it once rather than word by word.
- Pictures that only decorate, such as the app icon in About or a
  contact's photo beside their name, are hidden from the reader
  (`.accessibilityHidden(true)`).

Every word a reader hears comes from the same catalogues as the rest of
the interface, through `tr`, so it is read in the interface's language.

## The keyboard

- The menus carry the main actions and show their shortcuts, as a Mac
  app's do: reply, archive, delete, junk, flag, mark read, search, the
  Mail and Calendar spaces, and the assistant.
- Tab moves between the sidebar, the list and the conversation; the
  arrow keys move inside a list.
- A sheet's default button answers Return and its Cancel answers Escape
  (`.keyboardShortcut(.defaultAction)` and `.cancelAction`), so every
  question can be answered from the keyboard.

## Motion

Animations are short and follow the hand. SwiftUI drops them when the
system asks to reduce motion.

## Checking it

- Turn on VoiceOver (⌘F5) and move through a new screen with
  VoiceOver's keys: every control should say what it is and what it does.
- The Accessibility Inspector in Xcode's developer tools lists each
  element of a window with its label, and flags one with none.
- Try the screen with the keyboard alone, and with Reduce Motion and
  Increase Contrast turned on in System Settings.
