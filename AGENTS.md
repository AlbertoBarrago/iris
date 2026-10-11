# Iris

Mail and calendar for macOS: a SwiftUI and AppKit app (`macos/`) over a
Rust core it reaches through UniFFI (`ffi/`), several accounts synced into
SQLite. Targets macOS 26 on Apple silicon, Swift 6 and Rust 1.98. The crate map is in `CONTRIBUTING.md` under "How it is built"; the words
the code uses are in `CONTEXT.md`. Read both before changing behaviour.

## Where to look

- GitHub Issues: the public backlog. Start here when asked what is left
  or what to do next.
- `CONTEXT.md`: the glossary. A new domain term goes in here in the same
  change that introduces it.
- `docs/accessibility.md`, `docs/assistant.md`, `docs/setup.md`,
  `po/README.md`: read the one matching the area you touch.
- On the owner's machine only, and gitignored: `docs/remaining-work.md`,
  their own backlog, which you update when you close or find an item;
  `docs/superpowers/`, the design specs and build plans, history rather
  than instructions; and `docs/agents/`, notes for their agent skills. A
  contributor's checkout has none of these. Never commit them.

## The gate

A change is done when all three pass on the tree you are about to commit,
run after your last edit:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
scripts/update-po.sh --check      # stale? run scripts/update-po.sh, commit the result
```

A change to the Swift app builds with Xcode as well
(`scripts/dev-macos.sh`, or `xcodebuild` in `macos/`), and gives every
control it adds a name VoiceOver reads (`accessibilityLabel` where the
visible text does not say it); see `docs/accessibility.md`.

Any edit to a file holding translatable strings moves line numbers in
`po/iris.pot`, so `--check` goes stale from edits that change no
words. Run it last.

Read exit statuses from the command itself. `cargo test | tail` reports
`tail`'s status; use `${pipestatus[1]}` in zsh, or send the output to a
file and check `$?`.

Installing for the owner: `scripts/macos-install.sh`, which builds a
signed `Iris.app` into `/Applications`.

## Testing traps

- **Logic in Rust, tested there.** What a view decides (words, rules,
  layout numbers) lives in `appcore`, `render`, `view` or `ffi` with
  tests; the Swift views draw what it answers. A Swift view that starts
  deciding is drifting.
- **GnuPG tests** build a throwaway keyring and skip when `gpg` or
  `gpgsm` is missing. `IRIS_REQUIRE_CRYPTO=1` turns the skip into
  a failure. Fixtures write `pinentry-program /bin/false` into
  `gpg-agent.conf`; keep that in any new fixture, or each run puts a
  trust dialog on the owner's screen. In product code, every `gpg` and
  `gpgsm` run goes through `mailrs_pgp::gnupg::Program::run`, which takes
  `Pinentry::Never` (`--pinentry-mode error`) or `Pinentry::MayAsk`; only
  decrypting, signing and importing a file the person picked may ask for
  a passphrase. A test that imports a passphrase-protected PKCS#12 file
  names a pinentry script that answers from the fixture
  (`smime/tests/import.rs`), so nobody is asked.
- **Sandbox tests** for skill scripts run real `bwrap` and skip when it
  is missing, which on macOS it always is: skill scripts do not run on
  macOS until the sandbox moves to the Mac's own.
  `IRIS_REQUIRE_SANDBOX=1` turns the skip into a failure.
- **Docker tests** (`testmail/`, `imap/tests/dovecot*.rs`,
  `sync/tests/dovecot.rs`) start Dovecot and Mailpit and skip when Docker
  is missing or cannot start (on the owner's Mac, Docker runs through
  Colima). `IRIS_REQUIRE_IMAP=1` turns the skip into a failure; the gate
  does not set it. The tests
  never pull an image (`docker create --pull never`); run
  `scripts/test-images.sh` once on a new computer. Radicale serves the
  CalDAV and CardDAV tests, and `Profile::Sieve` Dovecot's ManageSieve. The
  files under `imap/tests/` and `sync/tests/` hold one test each, because
  that test points `SSL_CERT_FILE` at a root made for the run: add a step
  to it rather than a second test. The sync suite spawns its body on a
  runtime built with `mailrs_sync::WORKER_STACK`, as the app does, so a
  stack too small for a debug build fails there. Containers
  go by id when a test ends; one left by a killed run carries the label
  `io.github.AlbertoBarrago.iris.test`. Remove it by its id.
- **Migrations** live in one ordered array in `store/src/schema.rs`,
  numbered by position and tracked with `PRAGMA user_version`. Append
  only. Two branches that each add one collide on the number: renumber
  the later one on merge. Tests that hand-build an old schema must
  contain every table a later migration touches.

## Seeing the UI

`scripts/dev-macos.sh` builds the debug app and opens it on the owner's
own accounts. It quits the copy already running first, so it takes over
the owner's screen: say so before running it. `screencapture -o -l <window id> out.png` takes a picture of one
window. A recipe that moves `HOME` still reaches the owner's own
gpg-agent unless it sets `GNUPGHOME` too, and a secret key it imports
lands in their keyring.
Render SVGs with `rsvg-convert`: ImageMagick mangles gradients and makes
a good icon look broken.

## How the code is shaped

- **Mail actions live in `sync`**, not in windows. Archive, flag,
  label, list a mailbox, change an automatic reply: one module each in
  `sync`, called by both the window and the assistant. A window that
  starts doing its own Gmail work is drifting.
- **Late answers.** Any `await` in the UI can finish after the reader
  has moved to another conversation. A run that talks to the window
  holds a `Wanted` (`appcore/src/wanted.rs`) for the `Target` it started on
  and reaches its effect port only through it: `wait` and `ask` drop an
  answer once that target has left the screen, and `on_screen` makes a
  change only while it is there. In the Swift app, a task that awaits the
  core keeps the conversation it started on and checks
  `model.openThread == key` before it changes what is on screen.
- **Architecture vocabulary** is the `codebase-design` skill's: module,
  interface, depth, seam, adapter, leverage, locality. A seam gets
  introduced when a second adapter exists, not before.

## Words a person reads

- Every user-facing string goes through `mailrs_domain::translate`
  (`gettext`, `ngettext`, `fill`, `fill_plural`) in Rust, and through
  `tr` and `trPlural` in Swift, with a literal string so
  `update-po.sh` finds it. Placeholders are named like `{reason}`. A new
  file with such strings goes into `po/POTFILES.in`; `update-po.sh` warns
  when the list drifts.
- The owner reads the UI in Italian. `po/it_IT.po` is kept complete:
  translate new strings into it in the same change.
- Write source strings in American English (color, organize, canceled).
  British English is `po/en_GB.po`, generated by `scripts/en-gb.py` when
  `scripts/update-po.sh` runs; never edit it by hand, change the rules.
- Prose in the UI, comments, docs, and commit messages follows the
  stop-slop and unslop rules: plain statements, no em dashes, no
  adverbs propping up verbs. Comments explain why, in full sentences,
  matching the density of the code around them.

## The changelog

A change someone using the app would notice gets a line under
`## Unreleased` in `CHANGELOG.md`, in the same commit, under `### New`,
`### Improved` or `### Fixed`. Write it for that person: what they can now
do or what stopped going wrong, in plain words, one line. "Search finds
mail in every account again", not "Pass the account filter through to the
listing". Refactors, tests, CI and docs get no line.
`scripts/macos-publish.sh` turns the version's section into the release
notes, through `scripts/changelog.sh section`.

## Commits

Subject: one plain sentence saying what changed for a person or for the
code, capitalised, no prefix, no full stop ("Let a recipient field take
the cursor back", "Put the engine run behind a desk and an effect
port"). Body: why, and anything a reviewer would otherwise have to
rediscover. Commit on `main` in small steps; push when asked.

Parallel agents work in worktrees under `.claude/worktrees/`
(gitignored). After merging one, remove its worktree and delete both
its branches; `git log main..<branch>` must be empty before a delete.

## Working with the owner

The standing instruction is to decide on taste and keep going without
stopping to ask: build a complete, good-looking client. Ask only for
decisions that are theirs, such as the licence or anything outward
facing.

The app icon is a winged envelope: a cream envelope (`#fbf7f0` paper, a
night-violet frame from `#33267a` to `#1d1547`) with an iridescent wing of
five feathers rising from behind its top-left corner, cyan `#5ad1e8` to
violet `#8b6cf0` to pink `#f07ab8` to amber `#f5c46b`. The app icon sets it
on a macOS squircle shaded from `#5b3fd1` to `#1b1340`; the symbolic icon is
the same shape in one colour. The Add Account poses are the same envelope
with the wing raised, spread or drooped. `scripts/iris-art.py` draws all of
them into `data/` from the geometry `macos/Iris/PostBand.swift` uses, so
change the two together and regenerate rather than editing the SVGs by
hand; `scripts/macos-icon.sh` then draws the app icon set from them.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for AlbertoBarrago/iris, through the `gh` CLI. Anyone can open one, through the forms in `.github/ISSUE_TEMPLATE/`.

### Triage labels

The five triage labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. The issue forms add `needs-triage`.

### Domain docs

Single-context: one `CONTEXT.md` at the repo root. Architecture decision
records live in `docs/adr/` on the owner's machine only; it is gitignored.
