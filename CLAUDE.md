# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

`AGENTS.md` above is the agent guide (the gate, testing traps, how the
code is shaped, string and changelog rules). The rest of this file covers
the owner's own workflow, and wins where the two disagree.

## What this repo is

Iris is a mail and calendar app for macOS: a SwiftUI and AppKit app
(`macos/`) over a Rust core (`crates/ffi/`, UniFFI). Until 1.1 its front end was
GTK4 and libadwaita, removed when the Swift app replaced it. It started from [Penguin Mail](https://github.com/c9dev/penguin-mail)
1.0.0, a Linux app by David Santos; the first commit imports that tree,
and Iris has been its own project since. The README, `LICENSE` and the
last link of the About window credit Penguin Mail, as the GPL asks of a
modified copy.

- Remote: `origin` is `AlbertoBarrago/iris`, public. There is no upstream
  to merge from.
- App ID `io.github.AlbertoBarrago.Iris`, app `Iris.app` and binary
  `iris-cli`, env vars `IRIS_*`, mail headers `X-Iris-*`.
- Names that stay as they are because renaming them buys nothing: the
  crate names `mailrs-*` and the SQLite functions `penguin_fold` and
  `penguin_notes_text`.
- `CHANGELOG.md` starts at Iris 1.0.1, its first release. Iris is published by
  Alberto Barrago (albz, albertobarrago@gmail.com).
- Iris has its own identity, the winged envelope (see AGENTS.md's icon
  section and `scripts/iris-art.py`).

## Workflow

- VCS is `jj`, colocated with git. Work directly on `main`, no bookmarks
  per feature. Push only when asked.
- Commit messages use Conventional Commits (`feat:`, `fix:`,
  `refactor:`, `chore:`), not AGENTS.md's prefix-free style.
- Present a plan and wait for confirmation before implementing, despite
  AGENTS.md's "decide on taste and keep going".
- `po/it_IT.po` is kept complete: translate every new string into it in
  the same change. The owner reads the UI in Italian.
- Something that does not work on macOS gets made to work the macOS way
  (AppKit, Launch Services, the Keychain, `SMAppService`, Sparkle), not
  worked around.
- Base every solution on the current source and the technologies already
  in it. Read the code before proposing, reuse the modules, crates and
  patterns that exist, and invent nothing: no API, flag, behaviour or
  dependency that the code or its documentation does not show. When
  something is unknown, check it in the code or ask.
- Centralize what can be centralized: a rule or a piece of work lives in
  one place (the Rust core, per AGENTS.md) and every caller goes through
  it, rather than each front end doing its own copy.

## Commands

The workspace needs Rust 1.98. If the default toolchain is older, use
`cargo +1.98`.

```sh
scripts/dev-macos.sh                                # Xcode debug build, quit the old copy, open
cargo test --workspace                              # full suite
cargo test -p mailrs-sync some_test_name            # one test, by crate and name filter
cargo clippy --workspace --all-targets -- -D warnings
scripts/update-po.sh --check                        # run last; any edit to a file with strings moves .pot line numbers
```

Releases: `scripts/macos-dmg.sh`, commit and push, then
`scripts/macos-publish.sh` (GitHub release plus the page and the
appcast on GitHub Pages, served at https://albz.it/iris/). See
CONTRIBUTING.md, "Releasing".

Google and Microsoft sign-in need OAuth clients compiled in through
`IRIS_GOOGLE_CLIENT_ID`, `IRIS_GOOGLE_CLIENT_SECRET` and
`IRIS_MICROSOFT_CLIENT_ID` (see `crates/gmail/src/oauth.rs`). Local builds read
them from `packaging/secrets.env`, which is gitignored.

## macOS specifics

- XcodeGen writes `macos/Iris.xcodeproj` from `macos/project.yml`
  (`xcodegen generate` after adding a Swift file). Xcode builds the Rust
  core first through `scripts/build-ffi.sh`, which also writes the Swift
  bindings into `macos/Iris/Generated/`.
- Sparkle comes in as a Swift package; `macos/Info.plist` names the
  appcast, and `scripts/macos-dmg.sh` adds the public EdDSA key.
- Secrets go through `keyring` in `crates/gmail/src/token_store.rs` and
  `crates/sync/src/passwords.rs`, which stores them in the Keychain.
- Not on macOS yet, each to be made to work the macOS way: the action
  buttons on new-mail notifications (`UNUserNotificationCenter`
  categories) and the sandbox for skill scripts, which is bubblewrap
  today.
- Development needs Xcode 26 and `brew install xcodegen gettext librsvg`.
  Low on disk, build with
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`.
