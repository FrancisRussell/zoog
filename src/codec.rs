use std::fmt::{self, Display, Formatter};

/// Known audio codecs
#[derive(Debug, Clone, Copy)]
pub enum Codec {
    /// Ogg Opus
    Opus,

    /// Ogg Vorbis
    Vorbis,
}

impl Display for Codec {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        let name = match self {
            Codec::Opus => "Opus",
            Codec::Vorbis => "Vorbis",
        };
        write!(formatter, "{}", name)
    }
}
