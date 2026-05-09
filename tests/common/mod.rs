#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;
use zoog::header::FixedPointGain;

pub fn zoogcomment() -> Command { Command::new(env!("CARGO_BIN_EXE_zoogcomment")) }

pub fn opusgain() -> Command { Command::new(env!("CARGO_BIN_EXE_opusgain")) }

pub fn make_tone_opus(dir: &Path) -> PathBuf { make_tone_opus_with_tags(dir, &[]) }

/// Generate a stereo 1kHz / 48kHz Opus tone calibrated to a target BS.1770
/// integrated loudness.
///
/// ## Derivation
///
/// The ffmpeg `sine` lavfi source generates a sine wave at amplitude 1/8 by
/// default (documented in the ffmpeg lavfi sine filter).
///
/// The K-weighting filter in ITU-R BS.1770 has a magnitude response of |H| =
/// 1.083640 at 1 kHz / 48 kHz. For a sine wave at amplitude A the mean square
/// is A²/2, so the BS.1770 formula gives:
///     LUFS = −0.691 + 10·log10(A²/2 · |H|²)
///
/// Stereo is used so that loudgain (libebur128) and opusgain agree on the
/// measurement without ambiguity about mono dual-channel weighting.
///
/// Solving for A and expressing as a volume scale relative to the ffmpeg
/// default of 1/8:     A      = sqrt(2 · 10^((LUFS + 0.691) / 10)) / |H|
///     volume = A / (1/8) = 8A
pub fn make_reference_opus(dir: &Path, target_lufs: zoog::Decibels) -> PathBuf {
    // K-weighting filter magnitude at 1 kHz / 48 kHz per ITU-R BS.1770
    const K_WEIGHT_1KHZ_48KHZ: f64 = 1.083640;
    let lufs = target_lufs.as_f64();
    let a = f64::sqrt(2.0 * 10.0_f64.powf((lufs + 0.691) / 10.0)) / K_WEIGHT_1KHZ_48KHZ;
    let volume = a * 8.0;
    let filename = format!("{lufs}lufs.opus");
    build_opus(dir, &filename, 1000, 5, 2, Some(volume), &[])
}

pub fn make_silence_opus(dir: &Path) -> PathBuf {
    let path = dir.join("silence.opus");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=mono",
            "-t",
            "1",
            "-c:a",
            "libopus",
            "-b:a",
            "32k",
        ])
        .arg(&path)
        .status()
        .expect("ffmpeg must be installed");
    assert!(status.success(), "ffmpeg failed to generate silence fixture");
    path
}

fn build_opus(
    dir: &Path, filename: &str, freq: u32, duration: u32, channels: u32, volume: Option<f64>, tags: &[(&str, &str)],
) -> PathBuf {
    let path = dir.join(filename);
    let sine = format!("sine=frequency={freq}:duration={duration}");
    let channels = channels.to_string();
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", &sine]);
    if let Some(vol) = volume {
        cmd.args(["-af", &format!("volume={vol}")]);
    }
    cmd.args(["-ar", "48000", "-ac", &channels, "-c:a", "libopus", "-b:a", "64k"]);
    for (k, v) in tags {
        cmd.arg("-metadata").arg(format!("{k}={v}"));
    }
    cmd.arg(&path);
    assert!(cmd.status().expect("ffmpeg must be installed").success(), "ffmpeg failed to generate Opus fixture");
    path
}

/// Read the output gain from an Opus file.
pub fn opusinfo_output_gain(path: &Path) -> FixedPointGain {
    let info = opusinfo_tags(path);
    for line in info.lines() {
        if let Some(rest) = line.trim().strip_prefix("Playback gain:") {
            let db: f64 = rest.trim().trim_end_matches("dB").trim().parse().expect("parse playback gain");
            return FixedPointGain::try_from(zoog::Decibels::from(db)).expect("playback gain out of range");
        }
    }
    panic!("Playback gain not found in opusinfo output:\n{info}");
}

/// Read the R128_TRACK_GAIN tag from an Opus file, returning None if absent.
pub fn opusinfo_r128_track_gain(path: &Path) -> Option<FixedPointGain> {
    opusinfo_r128_tag(path, zoog::opus::TAG_TRACK_GAIN)
}

/// Read the R128_ALBUM_GAIN tag from an Opus file, returning None if absent.
pub fn opusinfo_r128_album_gain(path: &Path) -> Option<FixedPointGain> {
    opusinfo_r128_tag(path, zoog::opus::TAG_ALBUM_GAIN)
}

fn opusinfo_r128_tag(path: &Path, tag: &str) -> Option<FixedPointGain> {
    let info = opusinfo_tags(path);
    let prefix = format!("{tag}=");
    for line in info.lines() {
        if let Some(rest) = line.trim().strip_prefix(&prefix) {
            return Some(rest.trim().parse().unwrap_or_else(|_| panic!("parse {tag}")));
        }
    }
    None
}

pub fn make_tone_vorbis(dir: &Path) -> PathBuf {
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

/// Generate an Opus tone fixture with tags embedded via ffmpeg.
pub fn make_tone_opus_with_tags(dir: &Path, tags: &[(&str, &str)]) -> PathBuf {
    build_opus(dir, "tone.opus", 440, 1, 1, None, tags)
}

/// Generate a Vorbis tone fixture then add tags via vorbiscomment.
pub fn make_tone_vorbis_with_tags(dir: &Path, tags: &[(&str, &str)]) -> PathBuf {
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

/// Run a command, panicking with its stderr output if it exits non-zero.
pub fn run_ok(cmd: &mut std::process::Command) { run_and_stdout(cmd); }

/// Run a command, returning stdout, panicking with stderr if it exits non-zero.
pub fn run_and_stdout(cmd: &mut std::process::Command) -> String {
    let output = cmd.output().expect("failed to spawn process");
    if !output.status.success() {
        panic!("process failed ({}): {}", output.status, String::from_utf8_lossy(&output.stderr).trim());
    }
    String::from_utf8(output.stdout).expect("stdout is valid UTF-8")
}

/// Read tags from an Opus file using opusinfo as an independent verifier.
pub fn opusinfo_tags(path: &Path) -> String {
    let output = Command::new("opusinfo").arg(path).output().expect("opusinfo must be installed (opus-tools)");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn get_mtime(path: &Path) -> SystemTime { std::fs::metadata(path).expect("metadata").modified().expect("modified") }

pub fn set_mtime(path: &Path, time: SystemTime) {
    let file = std::fs::OpenOptions::new().write(true).open(path).expect("open for set_modified");
    file.set_modified(time).expect("set_modified");
}

/// Read tags from a Vorbis file using vorbiscomment as an independent verifier.
pub fn vorbiscomment_tags(path: &Path) -> String {
    let output = Command::new("vorbiscomment")
        .arg("-l")
        .arg(path)
        .output()
        .expect("vorbiscomment must be installed (vorbis-tools)");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn test_preserve_mtime_strategy(make_file: impl Fn() -> (TempDir, PathBuf), make_cmd: impl Fn(&str) -> Command) {
    let (_dir, file) = make_file();
    let original_mtime = SystemTime::now() - Duration::from_secs(3600);
    set_mtime(&file, original_mtime);
    run_ok(make_cmd("--mtime-strategy=preserve").arg(&file));
    assert_eq!(get_mtime(&file), original_mtime, "--mtime-strategy=preserve should not change mtime");
}

pub fn test_present_mtime_strategy(make_file: impl Fn() -> (TempDir, PathBuf), make_cmd: impl Fn(&str) -> Command) {
    let (_dir, file) = make_file();
    let now = SystemTime::now();
    let original_mtime = now - Duration::from_secs(3600);
    set_mtime(&file, original_mtime);
    run_ok(make_cmd("--mtime-strategy=present").arg(&file));
    let new_mtime = get_mtime(&file);
    assert!(new_mtime >= now, "--mtime-strategy=present should update mtime to now");
    assert!(new_mtime <= now + Duration::from_secs(5), "--mtime-strategy=present should not set mtime into the future");
}

pub fn test_minimal_increment_mtime_strategy(
    make_file: impl Fn() -> (TempDir, PathBuf), make_cmd: impl Fn(&str) -> Command,
) {
    for flag in ["--mtime-strategy=minimal-increment", "-M"] {
        let (_dir, file) = make_file();
        let now = SystemTime::now();
        let original_mtime = now - Duration::from_secs(3600);
        set_mtime(&file, original_mtime);
        run_ok(make_cmd(flag).arg(&file));
        let new_mtime = get_mtime(&file);
        assert!(new_mtime > original_mtime, "{flag} should increase mtime");
        assert!(
            new_mtime < original_mtime + Duration::from_secs(5),
            "{flag} should apply only a tiny delta, not update to present"
        );
    }
}
