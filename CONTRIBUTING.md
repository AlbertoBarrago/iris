# Contributing

Iris is one person's mail client, published so others can use it
and learn from it. It does not take pull requests: GitHub lets only
collaborators open them here. This file is for anyone who wants to report
a problem, build the app, or read how it works.

## Reporting a problem

Bug reports and ideas are welcome in
[Issues](https://github.com/AlbertoBarrago/iris/issues). A useful bug
report says which version you run (**About Iris** in the Iris menu), what you did,
what you expected, and what happened instead. Leave out message content
and addresses you would not post in public.

To report a security problem, see [SECURITY.md](SECURITY.md) instead of
opening an issue.

## Building from source

Iris builds on a Mac with Apple silicon and macOS 26 or later. You need
Rust 1.98 (`cargo +1.98` when your default toolchain is older) and GTK from
Homebrew. `scripts/dev-macos.sh` builds the app and runs it from its bundle:

```sh
brew install gtk4 libadwaita adwaita-icon-theme librsvg gettext pkgconf
scripts/dev-macos.sh              # with your accounts
scripts/dev-macos.sh --demo       # on sample accounts
```

`scripts/macos-install.sh` builds a release and installs it as
`/Applications/Iris.app`, loading GTK from Homebrew. A copy built from
source signs in to Google and Microsoft only with OAuth clients compiled
in; [docs/setup.md](docs/setup.md#building-your-own-copy) says how to give
it one. The demo needs no client.

## How it is built

```
domain/   shared types, Gmail's label names, categories
mime/     reading mail: charsets, address lists, HTML to text
gmail/    Gmail REST client, OAuth, quota limiter
graph/    Microsoft Graph client and Microsoft's sign-in
imap/     IMAP and SMTP clients for one account
pop3/     POP3 client
discover/ from an address to its mail servers: the provider table, MX,
          autoconfig files, SRV records and a probe
dav/      CalDAV and CardDAV: WebDAV's XML, a client, and calendars and
          contacts read from iCalendar and vCard
sieve/    Sieve rules and the automatic reply, and a ManageSieve client
store/    SQLite schema and queries
sync/     one sync loop per account: bootstrap, history replay, backfill,
          mail actions, mailbox listing, and each account's Gmail settings
pgp/      OpenPGP mail through the person's own gpg
smime/    S/MIME mail through their gpgsm
ai/       model providers, tool calls, the Claude Code bridge
cli/      iris-cli
webkit/   webkit6's API, as the app uses it, over WKWebView
app/      the GTK 4 and libadwaita app, with the calendar view in
          app/src/ui/calendar/
testmail/ Dovecot and Mailpit in Docker, for the IMAP and SMTP tests
```

Windows and dialogs stay thin. Archiving, flagging, listing a mailbox and
changing an automatic reply each live in one module in `sync`, which the
window and the assistant both call, so the two cannot drift apart. Those
modules take an account lookup and the store, so their tests run against an
in-memory database and a fake Gmail with no window on screen. The terms the
code uses are defined in [CONTEXT.md](CONTEXT.md), and
[AGENTS.md](AGENTS.md) has the conventions and the testing traps.

Sync follows Gmail's history API, polling every 30 seconds per account, so a
change made on your phone shows up within half a minute. When history runs
out, the account re-lists its mail and removes anything deleted in the gap.

## Checks

```sh
cargo test --workspace                                # no network
cargo clippy --workspace --all-targets -- -D warnings
scripts/update-po.sh --check                          # translation template current
```

CI runs those three on a macOS runner on every push to `main` and every pull request
(`.github/workflows/ci.yml`). The OpenPGP and S/MIME tests
build a throwaway GnuPG keyring and skip when `gpg` or `gpgsm` is missing.
`IRIS_REQUIRE_CRYPTO=1` turns that skip into a failure. The IMAP and SMTP
tests start Dovecot and Mailpit in Docker and skip without it (Colima
gives a Mac one); `scripts/test-images.sh` pulls their images once, and
`IRIS_REQUIRE_IMAP=1` turns the skip into a failure. The one GTK test, in
the composer, is ignored on macOS, where GTK starts only on the main
thread and the test harness keeps that thread for itself.

Release builds mask email addresses in the log, as `d…@example.com`.
Debug builds keep them whole, and `IRIS_LOG_DETAILS=1` does the
same for an installed copy while you look into a problem.

## Releasing

1. Bump `version` in `Cargo.toml` and write the `## Unreleased` section
   of `CHANGELOG.md` into a release. Sparkle offers a new DMG only when its
   version is higher than the one a tester runs.
2. Run `scripts/macos-dmg.sh`. It builds `target/dmg/Iris-<version>.dmg`
   with GTK and Sparkle inside, and rewrites `target/dmg/appcast.xml` for
   every DMG in that folder, signed with the Sparkle key in your login
   Keychain. Keep the older DMGs there, so the feed keeps listing them.
3. Commit the release and push `main`, then run `scripts/macos-publish.sh`.
   It creates the GitHub release `v<version>` with the DMG and its deltas,
   its notes taken from the changelog by `scripts/changelog.sh section`,
   and pushes `site/`, the appcast and `install.sh` to the `gh-pages`
   branch, which GitHub Pages serves at `https://albz.it/iris/`.
   `IRIS_DOWNLOAD_URL` and `IRIS_APPCAST_URL` change where the feed and the
   app look.

Every copy installed from a DMG checks the feed once a day, and
**Check for Updates…** in the Iris menu checks at once.

## License

The code is GPL-3.0-or-later. You may fork it, change it, and publish
your changes under the same license.
