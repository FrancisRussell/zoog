use std::io::{Cursor, Write};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::header::{self, IdHeader as _};
use crate::{Codec, Error};

const VORBIS_MIN_HEADER_SIZE: usize = 30;
const VORBIS_MAGIC: &[u8] = b"\x01vorbis";

/// Allows querying and modification of a Vorbis identification header
#[derive(Clone, Debug, PartialEq)]
pub struct IdHeader {
    data: Vec<u8>,
}

impl header::IdHeader for IdHeader {
    fn num_output_channels(&self) -> usize {
        let mut reader = Cursor::new(&self.data[11..12]);
        let value = reader.read_u8().expect("Error reading output channel count");
        value.into()
    }

    fn input_sample_rate(&self) -> Option<usize> { Some(self.output_sample_rate()) }

    fn output_sample_rate(&self) -> usize {
        let mut reader = Cursor::new(&self.data[12..16]);
        let value = reader.read_u32::<LittleEndian>().expect("Error reading sample rate");
        value.try_into().expect("Could not convert sample rate to usize")
    }
}

impl IdHeader {
    /// Attempts to parse the supplied `Vec` as an Vorbis header
    pub fn try_parse(data: &[u8]) -> Result<Option<IdHeader>, Error> {
        if data.len() < VORBIS_MIN_HEADER_SIZE {
            return Ok(None);
        }
        let identical = data.iter().take(VORBIS_MAGIC.len()).eq(VORBIS_MAGIC.iter());
        if !identical {
            return Ok(None);
        }
        let result = IdHeader { data: data.to_vec() };
        if result.version() != 0 {
            return Err(Error::UnsupportedCodecVersion(Codec::Vorbis, u64::from(result.version())));
        }
        let mut invalid = false;
        invalid &= result.num_output_channels() == 0;
        invalid &= result.output_sample_rate() == 0;
        invalid &= (result.data[29] & 1) != 0;
        if invalid {
            Err(Error::MalformedIdentificationHeader)
        } else {
            Ok(Some(result))
        }
    }

    /// The Vorbis version
    pub fn version(&self) -> u32 {
        let mut reader = Cursor::new(&self.data[7..11]);
        reader.read_u32::<LittleEndian>().expect("Error reading version")
    }

    /// Converts the header into data
    pub fn into_vec(self) -> Vec<u8> { self.data }

    /// Writes the serialized representation of the header
    pub fn serialize_into<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&self.data).map_err(Error::WriteError)
    }
}
