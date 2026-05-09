#![cfg(feature = "integration-tests")]

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use common::{
    make_reference_opus, make_silence_opus, opusgain, opusinfo_output_gain, opusinfo_r128_album_gain,
    opusinfo_r128_track_gain, run_ok,
};
use tempfile::TempDir;
use zoog::header::FixedPointGain;
use zoog::{Decibels, R128_LUFS, REPLAY_GAIN_LUFS};

// The reference tone is calibrated to this loudness per ITU-R BS.1770.
const REFERENCE_LUFS: Decibels = Decibels::new(-20.0);

// A second loudness level used in album mode tests, distinct from
// REFERENCE_LUFS.
const SECOND_LUFS: Decibels = Decibels::new(-26.0);

fn reference_file() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_reference_opus(dir.path(), REFERENCE_LUFS);
    (dir, file)
}

fn opusgain_cmd(flag: &str) -> Command {
    let mut cmd = opusgain();
    cmd.args(["--preset=rg", flag]);
    cmd
}

// Maximum permissible deviation from the expected gain, covering both
// single-file encoding error and the compound error when comparing two
// independently encoded files. Well below the ~1 dB threshold of human loudness
// perception.
const LOUDNESS_EPSILON: Decibels = Decibels::new(0.2);

fn db_to_fpg(db: Decibels) -> FixedPointGain { FixedPointGain::try_from(db).expect("gain in range") }

static ALL_PRESETS: &[&str] = &["rg", "r128", "original", "no-change"];

#[test]
// rg preset targets -18 LUFS. R128_TRACK_GAIN is always (R128_LUFS - (-18)) *
// 256 = -1280: the delta from -18 LUFS to the R128 reference, regardless of
// source loudness.
fn rg_preset_single_file() {
    let (_dir, file) = reference_file();

    run_ok(opusgain().args(["--preset=rg"]).arg(&file));

    assert_eq!(opusinfo_r128_track_gain(&file), Some(db_to_fpg(R128_LUFS - REPLAY_GAIN_LUFS)));
}

#[test]
// r128 preset targets R128_LUFS. R128_TRACK_GAIN is always 0: the output gain
// already brings playback to the R128 reference level, regardless of source
// loudness.
fn r128_preset_single_file() {
    let (_dir, file) = reference_file();

    run_ok(opusgain().args(["--preset=r128"]).arg(&file));

    assert_eq!(opusinfo_r128_track_gain(&file), Some(FixedPointGain::default()));
}

#[test]
// original preset sets output gain to 0 dB. R128_TRACK_GAIN encodes
// (R128_LUFS - LUFS_measured) * 256, which should be close to
// (R128_LUFS - REFERENCE_LUFS) * 256 for our reference file. The tolerance
// covers Opus lossy encoding error and anchors all tests to the reference being
// genuinely close to REFERENCE_LUFS.
fn original_preset_single_file() {
    let (_dir, file) = reference_file();

    run_ok(opusgain().args(["--preset=original"]).arg(&file));

    assert!(opusinfo_output_gain(&file).is_zero());
    let expected = R128_LUFS - REFERENCE_LUFS;
    let track_gain = opusinfo_r128_track_gain(&file).expect("R128_TRACK_GAIN should be present");
    let diff = (track_gain.as_decibels() - expected).abs();
    assert!(
        diff <= LOUDNESS_EPSILON,
        "R128_TRACK_GAIN {} differs from expected {expected} by more than ±{LOUDNESS_EPSILON}",
        track_gain.as_decibels()
    );
}

#[test]
// Album mode processes files together: all files get the same output gain and
// R128_ALBUM_GAIN. For r128 preset, R128_ALBUM_GAIN ≈ 0 (LUFS-independent, same
// reasoning as the single-file r128 test). Track gains differ since the files
// are at different loudness levels.
fn r128_preset_album_mode() {
    let dir = TempDir::new().expect("create temp dir");
    let file1 = make_reference_opus(dir.path(), REFERENCE_LUFS);
    let file2 = make_reference_opus(dir.path(), SECOND_LUFS);

    run_ok(opusgain().args(["--album", "--preset=r128"]).arg(&file1).arg(&file2));

    let album_gain1 = opusinfo_r128_album_gain(&file1).expect("R128_ALBUM_GAIN should be present");
    let album_gain2 = opusinfo_r128_album_gain(&file2).expect("R128_ALBUM_GAIN should be present");
    assert_eq!(album_gain1, album_gain2);
    assert!(album_gain1.as_decibels().abs() <= LOUDNESS_EPSILON);

    assert_eq!(opusinfo_output_gain(&file1), opusinfo_output_gain(&file2));

    let track_gain1 = opusinfo_r128_track_gain(&file1).expect("R128_TRACK_GAIN should be present");
    let track_gain2 = opusinfo_r128_track_gain(&file2).expect("R128_TRACK_GAIN should be present");
    let expected_diff = SECOND_LUFS - REFERENCE_LUFS;
    let diff = (track_gain1.as_decibels() - track_gain2.as_decibels() - expected_diff).abs();
    assert!(
        diff <= LOUDNESS_EPSILON,
        "track gain difference {} differs from expected {} by more than ±{LOUDNESS_EPSILON}",
        track_gain1.as_decibels() - track_gain2.as_decibels(),
        expected_diff,
    );
}

#[test]
// With --output-gain-mode=track in album mode, each file is track-normalised
// independently: output gains differ by the loudness difference between tracks,
// R128_TRACK_GAIN is the same on all files (≈ 0 for r128), and R128_ALBUM_GAIN
// differs per file since each has a different output gain applied.
fn r128_preset_album_mode_track_output_gain() {
    let dir = TempDir::new().expect("create temp dir");
    let file1 = make_reference_opus(dir.path(), REFERENCE_LUFS);
    let file2 = make_reference_opus(dir.path(), SECOND_LUFS);

    run_ok(opusgain().args(["--album", "--preset=r128", "--output-gain-mode=track"]).arg(&file1).arg(&file2));

    let track_gain1 = opusinfo_r128_track_gain(&file1).expect("R128_TRACK_GAIN should be present");
    let track_gain2 = opusinfo_r128_track_gain(&file2).expect("R128_TRACK_GAIN should be present");
    assert!(track_gain1.as_decibels().abs() <= LOUDNESS_EPSILON);
    assert!(track_gain2.as_decibels().abs() <= LOUDNESS_EPSILON);

    let output_gain1 = opusinfo_output_gain(&file1);
    let output_gain2 = opusinfo_output_gain(&file2);
    let expected_output_diff = SECOND_LUFS - REFERENCE_LUFS;
    let output_diff = (output_gain1.as_decibels() - output_gain2.as_decibels() - expected_output_diff).abs();
    assert!(
        output_diff <= LOUDNESS_EPSILON,
        "output gain difference {} differs from expected {} by more than ±{LOUDNESS_EPSILON}",
        output_gain1.as_decibels() - output_gain2.as_decibels(),
        expected_output_diff,
    );

    let album_gain1 = opusinfo_r128_album_gain(&file1).expect("R128_ALBUM_GAIN should be present");
    let album_gain2 = opusinfo_r128_album_gain(&file2).expect("R128_ALBUM_GAIN should be present");
    let album_diff = (album_gain1.as_decibels() - album_gain2.as_decibels() + expected_output_diff).abs();
    assert!(
        album_diff <= LOUDNESS_EPSILON,
        "album gain difference {} differs from expected {} by more than ±{LOUDNESS_EPSILON}",
        album_gain1.as_decibels() - album_gain2.as_decibels(),
        -expected_output_diff,
    );
}

#[test]
// --clear removes R128_TRACK_GAIN and R128_ALBUM_GAIN without changing the
// output gain. Run album r128 preset first to ensure both tags are present,
// then clear and verify they are absent while the output gain is unchanged.
fn clear_removes_r128_tags() {
    let (_dir, file) = reference_file();

    run_ok(opusgain().args(["--album", "--preset=r128"]).arg(&file));
    assert!(opusinfo_r128_track_gain(&file).is_some(), "R128_TRACK_GAIN should be present after r128 preset");
    assert!(opusinfo_r128_album_gain(&file).is_some(), "R128_ALBUM_GAIN should be present after r128 album preset");
    let output_gain_before = opusinfo_output_gain(&file);

    run_ok(opusgain().args(["--clear"]).arg(&file));

    assert!(opusinfo_r128_track_gain(&file).is_none(), "R128_TRACK_GAIN should be absent after --clear");
    assert!(opusinfo_r128_album_gain(&file).is_none(), "R128_ALBUM_GAIN should be absent after --clear");
    assert_eq!(opusinfo_output_gain(&file), output_gain_before, "output gain should be unchanged by --clear");
}

#[test]
// Silence produces NaN from the BS.1770 gated mean, which the analyzer maps to
// 0 LUFS (peak volume) to avoid applying a massive positive gain. For all
// presets this must result in a non-positive output gain (attenuating, never
// a dangerous boost), in both single-file and album mode.
fn silence_does_not_produce_positive_gain() {
    for preset in ALL_PRESETS {
        let dir = TempDir::new().expect("create temp dir");
        let file = make_silence_opus(dir.path());
        let arg = format!("--preset={preset}");

        run_ok(opusgain().arg(&arg).arg(&file));
        assert!(
            opusinfo_output_gain(&file).as_decibels() <= Decibels::default(),
            "preset {preset} produced a positive boost for silence"
        );

        run_ok(opusgain().args(["--album"]).arg(&arg).arg(&file));
        assert!(
            opusinfo_output_gain(&file).as_decibels() <= Decibels::default(),
            "preset {preset} produced a positive boost for silence in album mode"
        );
    }
}

#[test]
// Dry-run mode (--dry-run and its alias -n) leaves the file bytes completely
// unchanged, for all presets.
fn dry_run_does_not_modify() {
    for flag in ["--dry-run", "-n"] {
        for preset in ALL_PRESETS {
            let (_dir, file) = reference_file();
            let before = fs::read(&file).expect("read file");

            run_ok(opusgain().args([flag, &format!("--preset={preset}")]).arg(&file));

            assert_eq!(
                before,
                fs::read(&file).expect("read file"),
                "{flag} with preset {preset} modified file in dry-run mode"
            );
        }
    }
}

#[test]
// no-change preset preserves the existing output gain and rewrites
// R128_TRACK_GAIN relative to it. After an rg run the output gain is set to
// (REPLAY_GAIN_LUFS - measured). Running no-change then gives:
//   R128_TRACK_GAIN = R128_LUFS - measured - output_gain
//                   = R128_LUFS - REPLAY_GAIN_LUFS
// The measured LUFS cancels so the result is exact with no encoding tolerance.
fn no_change_preserves_output_gain() {
    let (_dir, file) = reference_file();
    run_ok(opusgain().args(["--preset=rg"]).arg(&file));
    let output_gain_after_rg = opusinfo_output_gain(&file);

    run_ok(opusgain().args(["--preset=no-change"]).arg(&file));

    assert_eq!(opusinfo_output_gain(&file), output_gain_after_rg);
    assert_eq!(opusinfo_r128_track_gain(&file), Some(db_to_fpg(R128_LUFS - REPLAY_GAIN_LUFS)));
}

#[test]
// Running opusgain twice with the same preset produces identical file bytes,
// i.e. the tool is idempotent across all presets.
fn presets_are_idempotent() {
    for preset in ALL_PRESETS {
        let (_dir, file) = reference_file();
        let arg = format!("--preset={preset}");

        run_ok(opusgain().arg(&arg).arg(&file));
        let after_first = fs::read(&file).expect("read file");

        run_ok(opusgain().arg(&arg).arg(&file));

        assert_eq!(after_first, fs::read(&file).expect("read file"), "preset {preset} is not idempotent");
    }
}

#[test]
// --mtime-strategy=preserve leaves the file's modification time unchanged after
// rewriting.
fn preserve_mtime_strategy() { common::test_preserve_mtime_strategy(reference_file, opusgain_cmd); }

#[test]
// --mtime-strategy=present updates the modification time to approximately the
// current system time.
fn present_mtime_strategy() { common::test_present_mtime_strategy(reference_file, opusgain_cmd); }

#[test]
// --mtime-strategy=minimal-increment (and its alias -M) nudges the modification
// time by the smallest filesystem-detectable delta.
fn minimal_increment_mtime_strategy() { common::test_minimal_increment_mtime_strategy(reference_file, opusgain_cmd); }
