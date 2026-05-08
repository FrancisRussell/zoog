#![cfg(feature = "integration-tests")]

mod common;

use std::path::PathBuf;

use common::{
    make_reference_opus, opusgain, opusinfo_output_gain_q78, opusinfo_r128_album_gain, opusinfo_r128_track_gain, run_ok,
};
use tempfile::TempDir;
use zoog::R128_LUFS;

// The reference tone is calibrated to this loudness per ITU-R BS.1770.
const REFERENCE_LUFS: f64 = -20.0;

// A second loudness level used in album mode tests, distinct from
// REFERENCE_LUFS.
const SECOND_LUFS: f64 = -26.0;

fn reference_file() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
    let file = make_reference_opus(dir.path(), REFERENCE_LUFS);
    (dir, file)
}

// Maximum deviation in Q7.8 units from the expected R128_TRACK_GAIN under the
// original preset, to allow for Opus lossy encoding of the reference signal.
// 0.1 dB was chosen as it is well below the ~1 dB threshold of human loudness
// perception.
const ENCODING_TOLERANCE_Q78: i32 = 26; // 0.1 dB * 256

#[test]
// rg preset targets -18 LUFS. R128_TRACK_GAIN is always (R128_LUFS - (-18)) *
// 256 = -1280: the delta from -18 LUFS to the R128 reference, regardless of
// source loudness.
fn rg_preset_single_file() {
    let (_dir, file) = reference_file();

    run_ok(opusgain().args(["--preset=rg"]).arg(&file));

    assert_eq!(opusinfo_r128_track_gain(&file), Some(-1280));
}

#[test]
// r128 preset targets R128_LUFS. R128_TRACK_GAIN is always 0: the output gain
// already brings playback to the R128 reference level, regardless of source
// loudness.
fn r128_preset_single_file() {
    let (_dir, file) = reference_file();

    run_ok(opusgain().args(["--preset=r128"]).arg(&file));

    assert_eq!(opusinfo_r128_track_gain(&file), Some(0));
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

    assert_eq!(opusinfo_output_gain_q78(&file), 0);
    let expected_track_gain = ((R128_LUFS.as_f64() - REFERENCE_LUFS) * 256.0).round() as i32;
    let track_gain = opusinfo_r128_track_gain(&file).expect("R128_TRACK_GAIN should be present");
    assert!(
        (track_gain - expected_track_gain).abs() <= ENCODING_TOLERANCE_Q78,
        "R128_TRACK_GAIN {track_gain} differs from expected {expected_track_gain} by more than ±{ENCODING_TOLERANCE_Q78}"
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
    assert!(album_gain1.abs() <= ENCODING_TOLERANCE_Q78);

    assert_eq!(opusinfo_output_gain_q78(&file1), opusinfo_output_gain_q78(&file2));

    let track_gain1 = opusinfo_r128_track_gain(&file1).expect("R128_TRACK_GAIN should be present");
    let track_gain2 = opusinfo_r128_track_gain(&file2).expect("R128_TRACK_GAIN should be present");
    let expected_diff = ((SECOND_LUFS - REFERENCE_LUFS) * 256.0).round() as i32;
    assert!(
        (track_gain1 - track_gain2 - expected_diff).abs() <= 2 * ENCODING_TOLERANCE_Q78,
        "track gain difference {} differs from expected {} by more than ±{}",
        track_gain1 - track_gain2,
        expected_diff,
        2 * ENCODING_TOLERANCE_Q78
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
    assert!(track_gain1.abs() <= ENCODING_TOLERANCE_Q78);
    assert!(track_gain2.abs() <= ENCODING_TOLERANCE_Q78);

    let output_gain1 = opusinfo_output_gain_q78(&file1);
    let output_gain2 = opusinfo_output_gain_q78(&file2);
    let expected_output_diff = ((SECOND_LUFS - REFERENCE_LUFS) * 256.0).round() as i32;
    assert!(
        (output_gain1 - output_gain2 - expected_output_diff).abs() <= 2 * ENCODING_TOLERANCE_Q78,
        "output gain difference {} differs from expected {} by more than ±{}",
        output_gain1 - output_gain2,
        expected_output_diff,
        2 * ENCODING_TOLERANCE_Q78
    );

    let album_gain1 = opusinfo_r128_album_gain(&file1).expect("R128_ALBUM_GAIN should be present");
    let album_gain2 = opusinfo_r128_album_gain(&file2).expect("R128_ALBUM_GAIN should be present");
    assert!(
        (album_gain1 - album_gain2 + expected_output_diff).abs() <= 2 * ENCODING_TOLERANCE_Q78,
        "album gain difference {} differs from expected {} by more than ±{}",
        album_gain1 - album_gain2,
        -expected_output_diff,
        2 * ENCODING_TOLERANCE_Q78
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
    let output_gain_before = opusinfo_output_gain_q78(&file);

    run_ok(opusgain().args(["--clear"]).arg(&file));

    assert!(opusinfo_r128_track_gain(&file).is_none(), "R128_TRACK_GAIN should be absent after --clear");
    assert!(opusinfo_r128_album_gain(&file).is_none(), "R128_ALBUM_GAIN should be absent after --clear");
    assert_eq!(opusinfo_output_gain_q78(&file), output_gain_before, "output gain should be unchanged by --clear");
}
