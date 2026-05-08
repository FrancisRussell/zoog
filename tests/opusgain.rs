#![cfg(feature = "integration-tests")]

mod common;

use std::path::PathBuf;

use common::{make_reference_opus, opusgain, opusinfo_output_gain_q78, opusinfo_r128_track_gain, run_ok};
use tempfile::TempDir;
use zoog::R128_LUFS;

// The reference tone is calibrated to this loudness per ITU-R BS.1770.
const REFERENCE_LUFS: f64 = -20.0;

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
