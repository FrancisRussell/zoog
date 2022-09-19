use crate::{FixedPointGain, ZoogError};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

const OPUS_MIN_HEADER_SIZE: usize = 19;
const OPUS_MAGIC: &[u8] = &[0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64];

pub struct OpusHeader<'a> {
    data: &'a mut Vec<u8>,
}

impl<'a> OpusHeader<'a> {
    pub fn try_new(data: &'a mut Vec<u8>) -> Option<OpusHeader<'a>> {
        if data.len() < OPUS_MIN_HEADER_SIZE { return None; }
        let identical = data.iter().take(OPUS_MAGIC.len()).eq(OPUS_MAGIC.iter());
        if !identical { return None; }
        Some(OpusHeader {
            data,
        })
    }

    pub fn get_output_gain(&self) -> FixedPointGain {
        let mut reader = Cursor::new(&self.data[16..18]);
        let value = reader.read_i16::<LittleEndian>().expect("Error reading gain");
        FixedPointGain { value }
    }

    pub fn set_output_gain(&mut self, gain: FixedPointGain) {
        let mut writer = Cursor::new(&mut self.data[16..18]);
        writer.write_i16::<LittleEndian>(gain.value).expect("Error writing gain");
    }

    pub fn adjust_output_gain(&mut self, adjustment: FixedPointGain) -> Result<(), ZoogError> {
        let gain = self.get_output_gain();
        let gain = gain.checked_add(adjustment).ok_or(ZoogError::GainOutOfBounds)?;
        self.set_output_gain(gain);
        Ok(())
    }

    pub fn num_output_channels(&self) -> Result<usize, ZoogError> {
        let mut reader = Cursor::new(&self.data[9..10]);
        let value = reader.read_u8().expect("Error reading output channel count");
        Ok(value.into())
    }
}

impl<'a> PartialEq for OpusHeader<'a> {
    fn eq(&self, other: &OpusHeader) -> bool {
        self.data == other.data
    }
}
