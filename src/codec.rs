use std::fmt::{self, Display, Formatter};

/// Known audio codecs
#[derive(Debug, Clone, Copy)]
pub enum Codec {
    Opus,
}

impl Display for Codec {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        let name = match self {
            Codec::Opus => "Opus",
        };
        write!(formatter, "{}", name)
    }
}
