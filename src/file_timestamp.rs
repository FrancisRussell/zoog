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

/// Sets the modification time of a file to the one specified but with a small
/// increment.
///
/// This function aims not to set the modification timestamp to a time beyond
/// the one currently present on the supplied file. This function returns `true`
/// if this operation was successfully applied. If it returns `false` then
/// the file timestamp may either be unchanged, or set to the supplied
/// timestamp, but with no increment applied.
pub fn set_mtime_with_minimal_increment(file: &std::fs::File, base_mtime: SystemTime) -> std::io::Result<bool> {
    // Record the original mtime and avoid setting the timestamp ahead of this
    // (though it could also occur due to timestamp rounding).
    let existing_mtime = file.metadata()?.modified()?;
    if base_mtime > existing_mtime {
        // Previous modification timestamp is ahead of the current one
        return Ok(false);
    }

    // We include the zero increment just in case we are copying to a filesystem
    // which has some sort of timestamp rounding.
    for increment in std::iter::once(Duration::ZERO).chain(SORTED_MODIFICATION_GRANULARITIES.iter().copied()) {
        let candidate_mtime = base_mtime + increment;
        if candidate_mtime > existing_mtime {
            return Ok(false);
        }
        file.set_modified(candidate_mtime)?;
        let new_mtime = file.metadata()?.modified()?;
        if new_mtime > base_mtime {
            return Ok(true);
        }
    }
    Ok(false)
}
