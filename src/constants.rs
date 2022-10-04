pub mod global {
    use crate::Decibels;

    /// The LUFS value specified by EBU R 128 (-23 LUFS)
    pub const R128_LUFS: Decibels = Decibels::from(-23.0);

    /// The LUFS value to use for ReplayGain (-18 LUFS). This is approximate
    /// since ReplayGain does not use LUFS.
    pub const REPLAY_GAIN_LUFS: Decibels = Decibels::from(-18.0);
}

pub mod opus {
    /// The name of the tag used to identify the track gain in Opus comment
    /// headers
    pub const TAG_TRACK_GAIN: &str = "R128_TRACK_GAIN";

    /// The name of the tag used to identify the album gain in Opus comment
    /// headers
    pub const TAG_ALBUM_GAIN: &str = "R128_ALBUM_GAIN";
}
