//! The release notes script: `changelog.sh`, which `macos-publish.sh`
//! runs for the notes of each GitHub release.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scripts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts")
}

fn succeeded(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

const CHANGELOG: &str = "# Changelog

Each release, newest first.

## Unreleased

## 1.1.0 (2026-01-02)

### New

- One.

### Fixed

- Two.


## 1.0.0 (2026-01-01)

- Zero.


";

/// Runs `changelog.sh section <version>` against `CHANGELOG` in a copy of
/// the repository's layout, since the script reads the file beside its own
/// folder. The copies run through bash: executing a file just written can
/// fail with "text file busy" while another test forks.
fn section(version: &str) -> Output {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("scripts")).unwrap();
    let script = dir.path().join("scripts/changelog.sh");
    std::fs::copy(scripts().join("changelog.sh"), &script).unwrap();
    std::fs::write(dir.path().join("CHANGELOG.md"), CHANGELOG).unwrap();
    Command::new("bash")
        .arg(&script)
        .args(["section", version])
        .output()
        .unwrap()
}

#[test]
fn a_changelog_section_is_its_body_without_the_blank_lines_around_it() {
    assert_eq!(
        succeeded(&section("1.1.0")),
        "### New\n\n- One.\n\n### Fixed\n\n- Two.\n"
    );
    assert_eq!(succeeded(&section("1.0.0")), "- Zero.\n");
    assert_eq!(succeeded(&section("Unreleased")), "");
}

/// The publish script writes the notes with this command, so a version
/// with no section must fail the run rather than publish empty notes. A
/// prefix of a version is not that version.
#[test]
fn a_version_with_no_changelog_section_fails() {
    for version in ["9.9.9", "1.1"] {
        let output = section(version);
        assert!(!output.status.success(), "{version} found a section");
        assert!(output.stdout.is_empty(), "{version}");
    }
}
