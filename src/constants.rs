pub mod global {
    use crate::Decibels;

    pub const R128_LUFS: Decibels = Decibels::from(-23.0);
    pub const REPLAY_GAIN_LUFS: Decibels = Decibels::from(-18.0);
}

pub mod opus {
    pub const TAG_TRACK_GAIN: &str = "R128_TRACK_GAIN";
    pub const TAG_ALBUM_GAIN: &str = "R128_ALBUM_GAIN";
}
