use ogg::reading::{OggReadError, PacketReader};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor};
use std::path::PathBuf;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use thiserror::Error;

const OPUS_MIN_HEADER_SIZE: usize = 19;
const OPUS_MAGIC: &'static [u8] = &[0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64];

enum State {
    AwaitingHeader,
    Forwarding,
}

#[derive(Debug, Error)]
enum ZoopError {
    #[error("Unable to open file `{0}` due to `{1}`")]
    FileOpenError(PathBuf, std::io::Error),
    #[error("Unable to open temporary file `{0}` due to `{1}`")]
    TempFileOpenError(PathBuf, std::io::Error),
    #[error("Ogg decoding error: `{0}`")]
    OggDecode(OggReadError),
    #[error("Error writing to file: `{0}`")]
    WriteError(std::io::Error),
    #[error("Not an Opus stream")]
    MissingOpusStream,
}

struct OpusHeader<'a> {
    data: &'a mut Vec<u8>,
}

#[derive(Debug, Copy, Clone)]
struct OutputGain {
    value: i16,
}

impl OutputGain {
    fn as_decibels(&self) -> f64 {
        self.value as f64 / 256.0
    }

    fn from_decibels(value: f64) -> OutputGain {
        OutputGain {
            value: (value * 256.0) as i16,
        }
    }
}

impl<'a> OpusHeader<'a> {
    fn try_new(data: &'a Vec<u8>) -> Option<OpusHeader<'a>> {
        if data.len() < OPUS_MIN_HEADER_SIZE { return None; }
        let identical = data.iter().take(OPUS_MAGIC.len()).eq(OPUS_MAGIC.iter());
        if !identical { return None; }
        let header = OpusHeader {
            data,
        };
        Some(header)
    }

    fn get_output_gain(&self) -> OutputGain {
        let mut reader = Cursor::new(&self.data[16..18]);
        let value = reader.read_i16::<LittleEndian>().expect("Error reading gain");
        OutputGain {
            value,
        }
    }

    fn set_output_gain(&mut self, gain: OutputGain) {
        let mut writer = Cursor::new(&mut self.data[16..18]);
        writer.write_i16::<LittleEndian>(gain.value).expect("Error writing gain");
    }
}

fn usage() {
    let args: Vec<String> = env::args().collect();
    eprintln!("Usage: {} input.ogg", args[0]);
    std::process::exit(1);
}

fn main() {
    match main_impl() {
        Ok(()) => {},
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        },
    }
}

fn main_impl() -> Result<(), ZoopError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { usage(); }
    let input_path = PathBuf::from(&args[1]);
    let input_file = File::open(&input_path).map_err(|e| ZoopError::FileOpenError(input_path.clone(), e))?;
    let input_file = BufReader::new(input_file);

    let output_path = {
        let mut path = input_path.clone();
        path.set_extension("zoog-tmp");
        path
    };
    let output_file = OpenOptions::new().write(true)
        //.create_new(true)
        .create(true) //FIXME: delete me
        .open(&output_path)
        .map_err(|e| ZoopError::TempFileOpenError(output_path.clone(), e))?;
    let output_file = BufWriter::new(output_file);

    let rewrite_result = {
        let mut ogg_reader = PacketReader::new(input_file);
        let mut ogg_writer = PacketWriter::new(output_file);
        let mut state = State::AwaitingHeader;
        loop {
            let packet = match ogg_reader.read_packet() {
                Err(e) => break Err(ZoopError::OggDecode(e)),
                Ok(packet) => packet,
            };
            let packet = match packet {
                None => break Ok(()),
                Some(packet) => packet,
            };
            let mut packet_data = packet.data.clone();

            match state {
                State::AwaitingHeader => {
                    let mut header = if let Some(header) = OpusHeader::try_new(&mut packet_data) {
                        header
                    } else {
                        break Err(ZoopError::MissingOpusStream)
                    };
                    println!("Output gain was: {}", header.get_output_gain().as_decibels());
                    header.set_output_gain(OutputGain::from_decibels(0.0));
                    println!("Output gain is now: {}", header.get_output_gain().as_decibels());
                    state = State::Forwarding;
                },
                State::Forwarding => {},
            }

            let packet_info = if packet.last_in_stream() {
                PacketWriteEndInfo::EndStream
            } else if packet.last_in_page() {
                PacketWriteEndInfo::EndPage
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            let packet_serial = packet.stream_serial();
            let packet_granule = packet.absgp_page();

            ogg_writer.write_packet(packet_data.into_boxed_slice(),
                packet_serial,
                packet_info,
                packet_granule,
            ).map_err(|e| ZoopError::WriteError(e))?;
        }
    };
    if rewrite_result.is_err() {
        if let Err(e) = std::fs::remove_file(&output_path) {
            eprintln!("Unable to delete temporary file: {}", e);
            eprintln!("Please delete {:?} manually.", output_path);
        }
    }
    rewrite_result
}
