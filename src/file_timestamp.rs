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
pub fn set_mtime_with_minimal_increment(file: &std::fs::File, base_mtime: SystemTime) -> std::io::Result<bool> {
    // Just in case the timestamp was extracted from a filesystem with different
    // modification timestamp granularity.
    file.set_modified(base_mtime)?;
    let base_mtime = file.metadata()?.modified()?;
    for increment in SORTED_MODIFICATION_GRANULARITIES.iter().copied() {
        let candidate_mtime = base_mtime + increment;
        file.set_modified(candidate_mtime)?;
        let new_mtime = file.metadata()?.modified()?;
        if new_mtime != base_mtime {
            return Ok(true);
        }
    }
    Ok(false)
}
