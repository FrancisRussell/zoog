use std::fmt;
use std::time::{Duration, SystemTime};

/// Modification timestamp granularities from various filesystems.
const SORTED_MODIFICATION_GRANULARITIES: &[Duration] = &[
    Duration::from_nanos(1),    // btrfs, ZFS, APFS, ext4 (256-bit inodes)
    Duration::from_nanos(100),  // NTFS
    Duration::from_micros(100), // UDF
    Duration::from_millis(10),  // exFAT
    Duration::from_secs(1),     // HFS+, ext3, ext4 (128-bit inodes)
    Duration::from_secs(2),     // FAT32
];

/// Possible outcomes of a modification time update opeation.
#[derive(Copy, Clone, Debug)]
pub enum SetMtimeOutcome {
    /// Update was successful.
    Success,

    /// The original modification time was already in the future.
    OriginalMtimeAhead,

    /// Could not find a minimal increment for the timestamp.
    NoSuitableIncrement,
}

impl fmt::Display for SetMtimeOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetMtimeOutcome::Success => write!(f, "updated successfully"),
            SetMtimeOutcome::OriginalMtimeAhead => {
                write!(f, "original modification time is in the future")
            }
            SetMtimeOutcome::NoSuitableIncrement => {
                write!(f, "no suitable increment found for filesystem timestamp granularity")
            }
        }
    }
}

/// Sets the modification time of a file to the one specified but with a small
/// increment.
///
/// This function aims not to set the modification timestamp to a time beyond
/// the one currently present on the supplied file.
pub fn set_mtime_with_minimal_increment(
    file: &std::fs::File, base_mtime: SystemTime,
) -> std::io::Result<SetMtimeOutcome> {
    // Record the original mtime and avoid setting the timestamp ahead of this
    // (though it could also occur due to timestamp rounding).
    let existing_mtime = file.metadata()?.modified()?;
    if base_mtime > existing_mtime {
        // Previous modification timestamp is ahead of the current one
        return Ok(SetMtimeOutcome::OriginalMtimeAhead);
    }

    // We include the zero increment just in case we are copying to a filesystem
    // which has some sort of timestamp rounding.
    for increment in std::iter::once(Duration::ZERO).chain(SORTED_MODIFICATION_GRANULARITIES.iter().copied()) {
        let candidate_mtime = base_mtime + increment;
        if candidate_mtime > existing_mtime {
            return Ok(SetMtimeOutcome::NoSuitableIncrement);
        }
        file.set_modified(candidate_mtime)?;
        let new_mtime = file.metadata()?.modified()?;
        if new_mtime > base_mtime {
            return Ok(SetMtimeOutcome::Success);
        }
    }
    Ok(SetMtimeOutcome::NoSuitableIncrement)
}
