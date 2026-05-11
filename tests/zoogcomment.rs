#![cfg(feature = "integration-tests")]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{opusinfo_tags, run_and_stdout, run_ok};
use tempfile::TempDir;

fn zoogcomment() -> Command { Command::new(env!("CARGO_BIN_EXE_zoogcomment")) }

fn make_tone_opus_with_tags(dir: &Path, tags: &[(&str, &str)]) -> PathBuf {
    common::build_opus(dir, "tone.opus", 440, 1, 1, None, tags)
}

fn make_tone_opus(dir: &Path) -> PathBuf { make_tone_opus_with_tags(dir, &[]) }

fn make_tone_vorbis(dir: &Path) -> PathBuf {
    let wav = dir.join("tone.wav");
    let ogg = dir.join("tone.ogg");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-ar",
            "44100",
            "-ac",
            "1",
        ])
        .arg(&wav)
        .status()
        .expect("ffmpeg must be installed");
    assert!(status.success(), "ffmpeg failed to generate WAV");
    let status = Command::new("oggenc")
        .args(["-Q", "-o"])
        .arg(&ogg)
        .arg(&wav)
        .status()
        .expect("oggenc must be installed (vorbis-tools)");
    assert!(status.success(), "oggenc failed to encode Vorbis fixture");
    ogg
}

/// Generate a Vorbis tone fixture then add tags via vorbiscomment.
fn make_tone_vorbis_with_tags(dir: &Path, tags: &[(&str, &str)]) -> PathBuf {
    let path = make_tone_vorbis(dir);
    let mut cmd = Command::new("vorbiscomment");
    cmd.arg("-a");
    for (k, v) in tags {
        cmd.arg("-t").arg(format!("{k}={v}"));
    }
    cmd.arg(&path);
    assert!(cmd.status().expect("vorbiscomment must be installed").success(), "vorbiscomment failed");
    path
}

/// Read tags from a Vorbis file using vorbiscomment as an independent verifier.
fn vorbiscomment_tags(path: &Path) -> String {
    let output = Command::new("vorbiscomment")
        .arg("-l")
        .arg(path)
        .output()
        .expect("vorbiscomment must be installed (vorbis-tools)");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
// Listing tags on an Opus file outputs the expected tag.
fn list_opus() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus_with_tags(dir.path(), &[("ARTIST", "Test Artist")]);

    assert!(run_and_stdout(zoogcomment().arg("-l").arg(&file)).contains("ARTIST=Test Artist"));
}

#[cfg(not(target_family = "windows"))]
#[test]
// Listing tags on an Ogg Vorbis file outputs the expected tag.
fn list_vorbis() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_vorbis_with_tags(dir.path(), &[("ARTIST", "Test Artist")]);

    assert!(run_and_stdout(zoogcomment().arg("-l").arg(&file)).contains("ARTIST=Test Artist"));
}

#[test]
// A tag added to an Opus file is visible to opusinfo.
fn add_tag_opus() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus(dir.path());

    run_ok(zoogcomment().args(["-m", "-t", "ARTIST=Test Artist"]).arg(&file));

    assert!(opusinfo_tags(&file).contains("ARTIST=Test Artist"));
}

#[cfg(not(target_family = "windows"))]
#[test]
// A tag added to an Ogg Vorbis file is visible to vorbiscomment.
fn add_tag_vorbis() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_vorbis(dir.path());

    run_ok(zoogcomment().args(["-m", "-t", "ARTIST=Test Artist"]).arg(&file));

    assert!(vorbiscomment_tags(&file).contains("ARTIST=Test Artist"));
}

#[test]
// Replace mode removes all pre-existing tags from an Opus file.
fn replace_clears_existing_tags_opus() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus_with_tags(dir.path(), &[("ARTIST", "Original")]);

    run_ok(zoogcomment().args(["-r", "-t", "TITLE=New Title"]).arg(&file));

    let tags = opusinfo_tags(&file);
    assert!(tags.contains("TITLE=New Title"));
    assert!(!tags.contains("ARTIST=Original"));
}

#[cfg(not(target_family = "windows"))]
#[test]
// Replace mode removes all pre-existing tags from an Ogg Vorbis file.
fn replace_clears_existing_tags_vorbis() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_vorbis_with_tags(dir.path(), &[("ARTIST", "Original")]);

    run_ok(zoogcomment().args(["-r", "-t", "TITLE=New Title"]).arg(&file));

    let tags = vorbiscomment_tags(&file);
    assert!(tags.contains("TITLE=New Title"));
    assert!(!tags.contains("ARTIST=Original"));
}

#[test]
// Deleting a tag by name from an Opus file removes it without affecting other
// tags.
fn delete_tag_opus() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus_with_tags(dir.path(), &[("ARTIST", "Test"), ("TITLE", "Song")]);

    run_ok(zoogcomment().args(["-m", "-d", "ARTIST"]).arg(&file));

    let tags = opusinfo_tags(&file);
    assert!(!tags.contains("ARTIST=Test"));
    assert!(tags.contains("TITLE=Song"));
}

#[cfg(not(target_family = "windows"))]
#[test]
// Deleting a tag by name from an Ogg Vorbis file removes it without affecting
// other tags.
fn delete_tag_vorbis() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_vorbis_with_tags(dir.path(), &[("ARTIST", "Test"), ("TITLE", "Song")]);

    run_ok(zoogcomment().args(["-m", "-d", "ARTIST"]).arg(&file));

    let tags = vorbiscomment_tags(&file);
    assert!(!tags.contains("ARTIST=Test"));
    assert!(tags.contains("TITLE=Song"));
}

#[test]
// All supported escape sequences (\n, \r, \\, \0) round-trip correctly through
// -e mode.
fn escaped_tag_values() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus(dir.path());

    // Each escape sequence as a separate tag for clarity
    run_ok(
        zoogcomment()
            .args([
                "-m",
                "-e",
                "-t",
                "A=before\\nafter",
                "-t",
                "B=before\\rafter",
                "-t",
                "C=before\\\\after",
                "-t",
                "D=before\\0after",
            ])
            .arg(&file),
    );

    let stdout = run_and_stdout(zoogcomment().args(["-l", "-e"]).arg(&file));
    assert!(stdout.contains("A=before\\nafter"));
    assert!(stdout.contains("B=before\\rafter"));
    assert!(stdout.contains("C=before\\\\after"));
    assert!(stdout.contains("D=before\\0after"));
}

#[test]
// Out-of-place mode writes tags to the output file and leaves the input
// unchanged.
fn out_of_place_write() {
    let dir = TempDir::new().expect("create temp dir");
    let input = make_tone_opus_with_tags(dir.path(), &[("ARTIST", "Original")]);
    let output = dir.path().join("output.opus");
    let input_before = fs::read(&input).expect("read input");

    run_ok(zoogcomment().args(["-m", "-t", "TITLE=Added"]).arg(&input).arg(&output));

    assert!(opusinfo_tags(&output).contains("TITLE=Added"));
    assert!(opusinfo_tags(&output).contains("ARTIST=Original"));
    assert_eq!(input_before, fs::read(&input).expect("read input"));
}

#[test]
// Dry-run mode (--dry-run and its alias -n) does not create the output file in
// out-of-place mode.
fn dry_run_does_not_create_output() {
    for flag in ["--dry-run", "-n"] {
        let dir = TempDir::new().expect("create temp dir");
        let input = make_tone_opus(dir.path());
        let output = dir.path().join("output.opus");

        run_ok(zoogcomment().args([flag, "-m", "-t", "ARTIST=Test"]).arg(&input).arg(&output));

        assert!(!output.exists(), "{flag} should not create output file in dry-run mode");
    }
}

#[test]
// Dry-run mode (--dry-run and its alias -n) leaves the file contents completely
// unchanged.
fn dry_run_does_not_modify() {
    for flag in ["--dry-run", "-n"] {
        let dir = TempDir::new().expect("create temp dir");
        let file = make_tone_opus(dir.path());
        let before = fs::read(&file).expect("read file");

        run_ok(zoogcomment().args([flag, "-m", "-t", "ARTIST=Test"]).arg(&file));

        assert_eq!(before, fs::read(&file).expect("read file"), "{flag} modified file in dry-run mode");
    }
}

fn tone_opus_file() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus(dir.path());
    (dir, file)
}

fn zoogcomment_cmd(flag: &str) -> Command {
    let mut cmd = zoogcomment();
    cmd.args(["-m", "-t", "ARTIST=Test", flag]);
    cmd
}

#[test]
// --mtime-strategy=preserve leaves the file's modification time unchanged after
// rewriting.
fn preserve_mtime_strategy() { common::test_preserve_mtime_strategy(tone_opus_file, zoogcomment_cmd); }

#[test]
// --mtime-strategy=present updates the modification time to approximately the
// current system time.
fn present_mtime_strategy() { common::test_present_mtime_strategy(tone_opus_file, zoogcomment_cmd); }

#[test]
// --mtime-strategy=minimal-increment (and its alias -M) nudges the modification
// time by the smallest filesystem-detectable delta.
fn minimal_increment_mtime_strategy() {
    common::test_minimal_increment_mtime_strategy(tone_opus_file, zoogcomment_cmd);
}
