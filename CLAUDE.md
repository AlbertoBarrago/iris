# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

`AGENTS.md` above is the agent guide (the gate, testing traps, how the
code is shaped, string and changelog rules). The rest of this file covers
the owner's own workflow, and wins where the two disagree.

## What this repo is

Iris is a mail and calendar app for macOS, in Rust with GTK4 and
libadwaita. It started from [Penguin Mail](https://github.com/c9dev/penguin-mail)
1.0.0, a Linux app by David Santos; the first commit imports that tree,
and Iris has been its own project since. The About window and the README
credit Penguin Mail, as the GPL asks of a modified copy.

- Remote: `origin` is `AlbertoBarrago/iris`, public. There is no upstream
  to merge from.
- App ID `io.github.AlbertoBarrago.Iris`, binaries `iris` and `iris-cli`,
  env vars `IRIS_*`, mail headers `X-Iris-*`.
- Names that stay as they are because renaming them buys nothing: the
  crate names `mailrs-*` (the app package is `mailrs`) and the SQLite
  functions `penguin_fold` and `penguin_notes_text`.
- `CHANGELOG.md` before the fork is Penguin Mail's. Iris is published by
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
- `po/it_IT.po` is kept complete like `po/pt_PT.po`: translate every
  new string into both in the same change. The owner reads the UI in
  Italian too.
- Something that does not work on macOS gets made to work the macOS way
  (AppKit, Launch Services, the Keychain, `SMAppService`, Sparkle), not
  worked around.

## Commands

The workspace needs Rust 1.98. If the default toolchain is older, use
`cargo +1.98`.

```sh
scripts/dev-macos.sh [--demo]                       # build, quit the old copy, run
cargo test --workspace                              # full suite
cargo test -p mailrs-sync some_test_name            # one test, by crate and name filter
cargo clippy --workspace --all-targets -- -D warnings
scripts/update-po.sh --check                        # run last; any edit to a file with strings moves .pot line numbers
cargo run -p mailrs -- --demo                       # the app on sample accounts, no sign-in
```

Releases: `scripts/macos-dmg.sh`, commit and push, then
`scripts/macos-publish.sh` (GitHub release plus the page and the
appcast on GitHub Pages, served at https://albz.it/iris/). See
CONTRIBUTING.md, "Releasing".

Google and Microsoft sign-in need OAuth clients compiled in through
`IRIS_GOOGLE_CLIENT_ID`, `IRIS_GOOGLE_CLIENT_SECRET` and
`IRIS_MICROSOFT_CLIENT_ID` (see `gmail/src/oauth.rs`). Local builds read
them from `packaging/secrets.env`, which is gitignored.

## macOS specifics

- The app names `webkit` only through the crate `webkit/`
  (`mailrs-webkit`): webkit6's API, which the app was written against,
  over a WKWebView laid on the GTK window (`webkit/src/macos/`).
  WebKitGTK does not run on macOS. When the app starts calling a webkit6
  method the crate lacks, add it there with webkit6's exact signature.
- AppKit work lives in the `macos_*` modules of the app (`macos_menu`,
  `macos_bundle`, `macos_pasteboard`), `dock.rs`, `sparkle.rs` and
  `file_type.rs` (media types through `UTType`).
- Secrets go through `keyring` in `gmail/src/token_store.rs` and
  `sync/src/passwords.rs`, which stores them in the Keychain.
- Not on macOS yet, each to be made to work the macOS way: Start at
  login (`SMAppService`), the action buttons on new-mail notifications
  (`UNUserNotificationCenter` categories), and the sandbox for skill
  scripts, which is bubblewrap today.
- Development needs `brew install gtk4 libadwaita adwaita-icon-theme
  pkgconf`. Low on disk, build with
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`.
