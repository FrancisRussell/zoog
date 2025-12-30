use std::fmt;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use clap::ValueEnum;

/// Modification timestamp granularities from various filesystems.
static SORTED_MODIFICATION_GRANULARITIES: LazyLock<Vec<Duration>> = LazyLock::new(|| {
    let mut durations = vec![
        Duration::from_nanos(1),    // btrfs, ZFS, APFS, ext4 (256-bit inodes)
        Duration::from_nanos(100),  // NTFS
        Duration::from_micros(100), // UDF (Linux kernel only writes centiseconds and hundreds-of-microseconds fields)
        Duration::from_millis(10),  // exFAT
        Duration::from_secs(1),     // HFS+, ext3, ext4 (128-bit inodes)
        Duration::from_secs(2),     // FAT32
    ];
    durations.sort();
    durations
});

/// Possible outcomes of a modification time update operation.
#[derive(Copy, Clone, Debug)]
pub enum SetMtimeOutcome {
    /// Update was successful.
    Success,

    /// Update was applied but the resulting timestamp is in the future because
    /// the original modification time was ahead of the system clock.
    SuccessWithFutureTimestamp,

    /// Could not find a minimal increment for the timestamp.
    NoSuitableIncrement,
}

impl fmt::Display for SetMtimeOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetMtimeOutcome::Success => write!(f, "updated successfully"),
            SetMtimeOutcome::SuccessWithFutureTimestamp => {
                write!(f, "updated, but timestamp is in the future (original mtime was ahead of system clock)")
            }
            SetMtimeOutcome::NoSuitableIncrement => {
                write!(f, "no suitable increment found for filesystem timestamp granularity")
            }
        }
    }
}

/// Strategies for updating file timestamps.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TimestampUpdateMode {
    /// Preserve the existing timestamp exactly.
    Preserve,
    /// Apply a minimal increment to the existing timestamp.
    MinimalIncrement,
    /// Update to the current system time.
    Present,
}

/// Sets the modification time of a file to the one calculated using the
/// specified strategy.
pub fn adjust_mtime(
    file: &std::fs::File, original_mtime: SystemTime, now: SystemTime, update_mode: TimestampUpdateMode,
) -> std::io::Result<SetMtimeOutcome> {
    match update_mode {
        TimestampUpdateMode::Present => {
            file.set_modified(now)?;
            Ok(SetMtimeOutcome::Success)
        }
        TimestampUpdateMode::MinimalIncrement => {
            let future_timestamp = original_mtime > now;
            // We include the zero increment just in case we are copying to a filesystem
            // which has some sort of timestamp rounding.
            for increment in std::iter::once(Duration::ZERO).chain(SORTED_MODIFICATION_GRANULARITIES.iter().copied()) {
                file.set_modified(original_mtime + increment)?;
                if file.metadata()?.modified()? > original_mtime {
                    return Ok(if future_timestamp {
                        SetMtimeOutcome::SuccessWithFutureTimestamp
                    } else {
                        SetMtimeOutcome::Success
                    });
                }
            }
            Ok(SetMtimeOutcome::NoSuitableIncrement)
        }
        TimestampUpdateMode::Preserve => {
            file.set_modified(original_mtime)?;
            Ok(SetMtimeOutcome::Success)
        }
    }
}
