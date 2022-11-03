use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::header::{self, FixedPointGain, IdHeader as _};
use crate::Error;

const OPUS_MIN_HEADER_SIZE: usize = 19;
const OPUS_MAGIC: &[u8] = b"OpusHead";

/// The internal and preferred Opus sample rate (RFC 7845, section 5.1)
const OPUS_DECODE_SAMPLE_RATE: usize = 48000;

/// Allows querying and modification of an Opus identification header
#[derive(Debug)]
pub struct IdHeader<'a> {
    data: &'a mut Vec<u8>,
}

impl header::IdHeader for IdHeader<'_> {
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

impl<'a> IdHeader<'a> {
    /// Attempts to parse the supplied `Vec` as an Opus header
    pub fn try_parse(data: &'a mut Vec<u8>) -> Result<Option<IdHeader<'a>>, Error> {
        if data.len() < OPUS_MIN_HEADER_SIZE {
            return Ok(None);
        }
        let identical = data.iter().take(OPUS_MAGIC.len()).eq(OPUS_MAGIC.iter());
        if !identical {
            return Ok(None);
        }
        let result = IdHeader { data };
        if result.num_output_channels() == 0 {
            return Err(Error::MalformedIdentificationHeader);
        }
        Ok(Some(result))
    }

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
}

impl<'a> PartialEq for IdHeader<'a> {
    fn eq(&self, other: &IdHeader) -> bool { self.data == other.data }
}