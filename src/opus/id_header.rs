use std::io::{Cursor, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::header::{self, FixedPointGain};
use crate::{Codec, Error};

const OPUS_MIN_HEADER_SIZE: usize = 19;
const OPUS_MAGIC: &[u8] = b"OpusHead";

/// The internal and preferred Opus sample rate (RFC 7845, section 5.1)
const OPUS_DECODE_SAMPLE_RATE: usize = 48000;

/// Allows querying and modification of an Opus identification header
#[derive(Clone, Debug, PartialEq)]
pub struct IdHeader {
    data: Vec<u8>,
}

impl header::IdHeader for IdHeader {
    fn try_parse(data: &[u8]) -> Result<Option<IdHeader>, Error> {
        if data.len() < OPUS_MIN_HEADER_SIZE {
            return Ok(None);
        }
        let identical = data.iter().take(OPUS_MAGIC.len()).eq(OPUS_MAGIC.iter());
        if !identical {
            return Ok(None);
        }
        let result = IdHeader { data: data.to_vec() };
        if result.version() != 1 {
            return Err(Error::UnsupportedCodecVersion(Codec::Opus, u64::from(result.version())));
        }
        if result.num_output_channels() == 0 {
            return Err(Error::MalformedIdentificationHeader);
        }
        Ok(Some(result))
    }

    fn into_vec(self) -> Vec<u8> { self.data }

    fn serialize_into<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&self.data).map_err(Error::WriteError)
    }

    fn num_output_channels(&self) -> usize {
        let mut reader = Cursor::new(&self.data[9..10]);
        let value = reader.read_u8().expect("Error reading output channel count");
        value.into()
    }

    fn input_sample_rate(&self) -> Option<usize> {
        let mut reader = Cursor::new(&self.data[12..16]);
        let value = reader.read_u32::<LittleEndian>().expect("Error reading sample rate");
        if value == 0 {
            None
        } else {
            Some(value.try_into().expect("Could not convert sample rate to usize"))
        }
    }

    fn output_sample_rate(&self) -> usize { OPUS_DECODE_SAMPLE_RATE }
}

impl IdHeader {
    /// The current output gain set in the header
    pub fn get_output_gain(&self) -> FixedPointGain {
        let mut reader = Cursor::new(&self.data[16..18]);
        let value = reader.read_i16::<LittleEndian>().expect("Error reading gain");
        FixedPointGain::from_fixed_point(value)
    }

    /// Sets the header's output gain
    pub fn set_output_gain(&mut self, gain: FixedPointGain) {
        let mut writer = Cursor::new(&mut self.data[16..18]);
        writer.write_i16::<LittleEndian>(gain.as_fixed_point()).expect("Error writing gain");
    }

    /// Applies a delta to the header's output gain. This may return an error if
    /// the delta causes the gain to overflow or underflow.
    pub fn adjust_output_gain(&mut self, adjustment: FixedPointGain) -> Result<(), Error> {
        let gain = self.get_output_gain();
        let gain = gain.checked_add(adjustment).ok_or(Error::GainOutOfBounds)?;
        self.set_output_gain(gain);
        Ok(())
    }

    /// Gets the Opus encapsulation version
    pub fn version(&self) -> u8 {
        let mut reader = Cursor::new(&self.data[8..9]);
        reader.read_u8().expect("Error reading output channel count")
    }
}
