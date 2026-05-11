use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

pub fn build_opus(
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

/// Read tags from an Opus file using opusinfo as an independent verifier.
pub fn opusinfo_tags(path: &Path) -> String {
    let output = Command::new("opusinfo").arg(path).output().expect("opusinfo must be installed (opus-tools)");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn get_mtime(path: &Path) -> SystemTime { std::fs::metadata(path).expect("metadata").modified().expect("modified") }

fn set_mtime(path: &Path, time: SystemTime) {
    let file = std::fs::OpenOptions::new().write(true).open(path).expect("open for set_modified");
    file.set_modified(time).expect("set_modified");
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
