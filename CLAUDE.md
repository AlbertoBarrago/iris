# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

`AGENTS.md` above is upstream's agent guide (the gate, testing traps, how
the code is shaped, string and changelog rules) and applies here. The rest
of this file covers what differs in Iris, and wins where the two disagree.

## What this repo is

Iris is a fork of [Penguin Mail](https://github.com/c9dev/penguin-mail)
1.0.0, rebranded and being ported to macOS while Linux keeps working.

- Remotes: `origin` is `AlbertoBarrago/iris`, `upstream` is
  `c9dev/penguin-mail`. Pull upstream fixes with
  `jj git fetch --remote upstream`, then rebase or merge onto `main`.
- App ID `io.github.AlbertoBarrago.Iris`, binaries `iris` and `iris-cli`,
  env vars `IRIS_*`, mail headers `X-Iris-*`.
- Kept on purpose, to keep upstream merges cheap: the crate names
  `mailrs-*` (the app package is `mailrs`), the SQLite functions
  `penguin_fold` and `penguin_notes_text`, and the historical IDs
  `dev.penguinmail.PenguinMail` and `dev.mailrs.Mailrs` used by the
  migrations in `app/src/old_id.rs` and `scripts/install-files.sh`.
- Left as upstream's: `CHANGELOG.md` before the fork and the apt signing
  key in `packaging/apt/`. Iris is published by Alberto Barrago (albz,
  albertobarrago@gmail.com); the About window and the README credit
  Penguin Mail, as the GPL asks of a modified copy.
- Iris has its own identity, the winged envelope (see AGENTS.md's icon
  section and `scripts/iris-art.py`); the penguin is upstream's.

## Workflow overrides

- VCS is `jj`, colocated with git. Work directly on `main`, no bookmarks
  per feature. Push only when asked.
- Commit messages use Conventional Commits (`feat:`, `fix:`,
  `refactor:`, `chore:`), not AGENTS.md's prefix-free style.
- Present a plan and wait for confirmation before implementing, despite
  AGENTS.md's "decide on taste and keep going".
- `po/it_IT.po` is kept complete like `po/pt_PT.po`: translate every
  new string into both in the same change. The owner reads the UI in
  Italian too.
- `.github/workflows` and `packaging/` are inherited and inactive for
  Iris (no apt, rpm, snap or Flatpak publishing yet). Flag before
  changing them.

## Commands

The workspace needs Rust 1.98. If the default toolchain is older, use
`cargo +1.98`.

```sh
scripts/dev-macos.sh [--demo]                       # macOS: build, quit the old copy, run
cargo test --workspace                              # full suite (Linux)
cargo test --workspace --exclude mailrs             # macOS today: everything but the GTK app
cargo test -p mailrs-sync some_test_name            # one test, by crate and name filter
cargo clippy --workspace --all-targets -- -D warnings
scripts/update-po.sh --check                        # run last; any edit to a file with strings moves .pot line numbers
cargo run -p mailrs -- --demo                       # the app on sample accounts, no sign-in
```

Google and Microsoft sign-in need OAuth clients compiled in through
`IRIS_GOOGLE_CLIENT_ID`, `IRIS_GOOGLE_CLIENT_SECRET` and
`IRIS_MICROSOFT_CLIENT_ID` (see `gmail/src/oauth.rs`). Iris has none of
its own yet.

## macOS port

Platform-specific code is confined to the `app` crate and two secret
stores; the other crates build and test on macOS as they are.

- The app names `webkit` only through the facade crate `webkit/`
  (`mailrs-webkit`): webkit6 re-exported on Linux, and on macOS the same
  API over a WKWebView laid on the GTK window (`webkit/src/macos/`).
  WebKitGTK does not run on macOS; Homebrew's `webkitgtk` is a Linux-only
  GTK 3 build. When the app starts calling a webkit6 method the shim
  lacks, add it there with webkit6's exact signature rather than a `cfg`
  in the app.
- `ksni` (`app/src/tray.rs`) is a D-Bus StatusNotifierItem: no macOS
  equivalent, gate it out first, menu bar item later.
- `app/src/autostart.rs` writes an XDG autostart `.desktop` file; macOS
  needs a LaunchAgent or `SMAppService`.
- `app/src/update/` installs through apt, `pkexec` and tarballs; disable
  on macOS.
- Secrets go through `keyring` in `gmail/src/token_store.rs` and
  `sync/src/passwords.rs`; its default features already use the macOS
  Keychain. `oo7` and the Secret portal are Flatpak only
  (`packaging-flatpak` feature).
- macOS development needs `brew install gtk4 libadwaita
  adwaita-icon-theme pkgconf`. Low on disk, build with
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`.
- `app/src/packaging.rs` says which Linux package a build is for; macOS
  needs its own case.

Keep each of these behind `cfg(target_os = ...)` so Linux behavior and
upstream merges stay untouched.
