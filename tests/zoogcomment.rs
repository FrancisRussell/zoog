#![cfg(feature = "integration-tests")]

mod common;

use std::fs;

use common::{
    make_tone_opus, make_tone_opus_with_tags, make_tone_vorbis, make_tone_vorbis_with_tags, opusinfo_tags,
    run_and_stdout, run_ok, vorbiscomment_tags, zoogcomment,
};
use tempfile::TempDir;

#[test]
// Listing tags on an Opus file outputs the expected tag.
fn list_opus() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus_with_tags(dir.path(), &[("ARTIST", "Test Artist")]);

    assert!(run_and_stdout(zoogcomment().arg("-l").arg(&file)).contains("ARTIST=Test Artist"));
}

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
// Dry-run mode does not create the output file in out-of-place mode.
fn dry_run_does_not_create_output() {
    let dir = TempDir::new().expect("create temp dir");
    let input = make_tone_opus(dir.path());
    let output = dir.path().join("output.opus");

    run_ok(zoogcomment().args(["-n", "-m", "-t", "ARTIST=Test"]).arg(&input).arg(&output));

    assert!(!output.exists());
}

#[test]
// Dry-run mode leaves the file contents completely unchanged.
fn dry_run_does_not_modify() {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_tone_opus(dir.path());
    let before = fs::read(&file).expect("read file");

    run_ok(zoogcomment().args(["-n", "-m", "-t", "ARTIST=Test"]).arg(&file));

    assert_eq!(before, fs::read(&file).expect("read file"));
}
