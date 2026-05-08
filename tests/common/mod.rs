use std::path::{Path, PathBuf};
use std::process::Command;

pub fn zoogcomment() -> Command { Command::new(env!("CARGO_BIN_EXE_zoogcomment")) }

pub fn opusgain() -> Command { Command::new(env!("CARGO_BIN_EXE_opusgain")) }

pub fn make_tone_opus(dir: &Path) -> PathBuf { make_tone_opus_with_tags(dir, &[]) }

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
    let path = dir.join("tone.opus");
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        "-ar",
        "48000",
        "-ac",
        "1",
        "-c:a",
        "libopus",
        "-b:a",
        "64k",
    ]);
    for (k, v) in tags {
        cmd.arg("-metadata").arg(format!("{k}={v}"));
    }
    cmd.arg(&path);
    assert!(cmd.status().expect("ffmpeg must be installed").success(), "ffmpeg failed");
    path
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

/// Read tags from a Vorbis file using vorbiscomment as an independent verifier.
pub fn vorbiscomment_tags(path: &Path) -> String {
    let output = Command::new("vorbiscomment")
        .arg("-l")
        .arg(path)
        .output()
        .expect("vorbiscomment must be installed (vorbis-tools)");
    String::from_utf8_lossy(&output.stdout).into_owned()
}
