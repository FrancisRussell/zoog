use std::io::Write;

use crate::Error;

/// Trait for codec identification headers
pub trait IdHeader {
    /// Attempts to parse the supplied slice as an identification header
    fn try_parse(data: &[u8]) -> Result<Option<Self>, Error>
    where
        Self: Sized;

    /// The number of output channels
    fn num_output_channels(&self) -> usize;

    /// The sample rate of the original source (may not be available)
    fn input_sample_rate(&self) -> Option<usize>;

    /// The sample rate audio should be decoded at
    fn output_sample_rate(&self) -> usize;

    /// Serializes the header into a `Write`
    fn serialize_into<W: Write>(&self, writer: &mut W) -> Result<(), Error>;

    /// Converts the header into a `Vec`
    fn into_vec(self) -> Vec<u8>;
}
