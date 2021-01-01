use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use derivative::Derivative;
use ogg::reading::{OggReadError, PacketReader};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tempfile::PersistError;
use thiserror::Error;

const OPUS_MIN_HEADER_SIZE: usize = 19;
const OPUS_MAGIC: &'static [u8] = &[0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64];
const COMMENT_MAGIC: &'static [u8] = &[0x4f, 0x70, 0x75, 0x73, 0x54, 0x61, 0x67, 0x73];
const TAG_TRACK_GAIN: &'static str = "R128_TRACK_GAIN";
const TAG_ALBUM_GAIN: &'static str = "R128_ALBUM_GAIN";

enum State {
    AwaitingHeader,
    AwaitingComments,
    Forwarding,
}

#[derive(Debug, Error)]
enum ZoopError {
    #[error("Unable to open file `{0}` due to `{1}`")]
    FileOpenError(PathBuf, std::io::Error),
    #[error("Unable to open temporary file due to `{0}`")]
    TempFileOpenError(std::io::Error),
    #[error("Ogg decoding error: `{0}`")]
    OggDecode(OggReadError),
    #[error("Error writing to file: `{0}`")]
    WriteError(std::io::Error),
    #[error("Not an Opus stream")]
    MissingOpusStream,
    #[error("Comment header is missing")]
    MissingCommentHeader,
    #[error("Malformed comment header")]
    MalformedCommentHeader,
    #[error("UTF-8 encoding error")]
    UTF8Error(#[from] std::string::FromUtf8Error),
    #[error("R128 tag has invalid value")]
    InvalidR128Tag,
    #[error("Gain out of bounds")]
    GainOutOfBounds,
    #[error("Failed to rename `{0}` to `{1}` due to `{2}`")]
    FileCopy(PathBuf, PathBuf, std::io::Error),
    #[error("Failed to persist temporary file due to `{0}``")]
    PersistError(#[from] PersistError),
}

struct OpusHeader<'a> {
    data: &'a mut Vec<u8>,
}

#[derive(Default, Copy, Clone, Debug)]
struct OutputGain {
    value: i16,
}

impl OutputGain {
    fn as_decibels(&self) -> f64 {
        self.value as f64 / 256.0
    }

    fn as_fixed_point(&self) -> i16 {
        self.value
    }

    fn from_decibels(value: f64) -> OutputGain {
        OutputGain {
            value: (value * 256.0) as i16,
        }
    }

    fn is_none(&self) -> bool {
        self.value == 0
    }

    fn checked_add(&self, rhs: &OutputGain) -> Option<OutputGain> {
        let new_value = self.value.checked_add(rhs.value);
        if let Some(value) = new_value {
            Some(OutputGain {
                value,
            })
        } else {
            None
        }
    }
}

impl FromStr for OutputGain {
    type Err = <i16 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.parse::<i16>()?;
        Ok(OutputGain {
            value,
        })
    }
}

impl<'a> OpusHeader<'a> {
    fn try_new(data: &'a mut Vec<u8>) -> Option<OpusHeader<'a>> {
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

#[derive(Derivative)]
#[derivative(Debug)]
struct CommentHeader<'a> {
    #[derivative(Debug="ignore")]
    data: &'a mut Vec<u8>,
    vendor: String,
    user_comments: Vec<(String, String)>,
}

impl<'a> CommentHeader<'a> {
    fn try_parse(data: &'a mut Vec<u8>) -> Result<CommentHeader<'a>, ZoopError> {
        let mut reader = Cursor::new(&data[COMMENT_MAGIC.len()..]);
        let vendor_len = reader.read_u32::<LittleEndian>().map_err(|_| ZoopError::MalformedCommentHeader)?;
        let mut vendor = vec![0u8; vendor_len as usize];
        reader.read_exact(&mut vendor[..]).map_err(|_| ZoopError::MalformedCommentHeader)?;
        let vendor = String::from_utf8(vendor)?;
        let num_comments = reader.read_u32::<LittleEndian>().map_err(|_| ZoopError::MalformedCommentHeader)?;
        let mut user_comments = Vec::with_capacity(num_comments as usize);
        for _ in 0..num_comments {
            let comment_len = reader.read_u32::<LittleEndian>().map_err(|_| ZoopError::MalformedCommentHeader)?;
            let mut comment = vec![0u8; comment_len as usize];
            reader.read_exact(&mut comment[..]).map_err(|_| ZoopError::MalformedCommentHeader)?;
            let comment = String::from_utf8(comment)?;
            let offset = if let Some(offset) = comment.find("=") {
                offset
            } else {
                return Err(ZoopError::MalformedCommentHeader);
            };
            let (key, value) = comment.split_at(offset);
            user_comments.push((String::from(key), String::from(&value[1..])));
        }
        let result = CommentHeader {
            data,
            vendor,
            user_comments,
        };
        Ok(result)
    }

    fn try_new(data: &'a mut Vec<u8>) -> Result<Option<CommentHeader<'a>>, ZoopError> {
        let identical = data.iter().take(COMMENT_MAGIC.len()).eq(COMMENT_MAGIC.iter());
        if !identical { return Ok(None); }
         Self::try_parse(data).map(|v| Some(v))
    }

    fn get_first(&self, key: &str) -> Option<&str> {
        for (k, v) in self.user_comments.iter() {
            if k == key { return Some(v); }
        }
        return None;
    }

    fn remove_all(&mut self, key: &str) {
        self.user_comments = self.user_comments.iter().filter(|(k, _)| key != k).cloned().collect();
    }

    fn replace(&mut self, key: &str, value: &str) {
        self.remove_all(key);
        self.user_comments.push((String::from(key), String::from(value)));
    }

    fn get_gain_from_tag(&self, tag: &str) -> Result<OutputGain, ZoopError> {
        self.get_first(tag)
            .map(|v| v.parse::<OutputGain>().map_err(|_| ZoopError::InvalidR128Tag))
            .unwrap_or(Ok(OutputGain::default()))
    }

    fn adjust_gains(&mut self, adjustment: OutputGain) -> Result<(), ZoopError> {
        if adjustment.is_none() { return Ok(()); }
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            let gain = self.get_gain_from_tag(*tag)?;
            let gain = if let Some(gain) = gain.checked_add(&adjustment) {
                gain
            } else {
                return Err(ZoopError::GainOutOfBounds);
            };
            self.replace(*tag, &format!("{}", gain.as_fixed_point()));
        }
        Ok(())
    }
}

fn print_gains<'a>(header: &CommentHeader<'a>) -> Result<(), ZoopError> {
    println!("{}={}dB", TAG_ALBUM_GAIN, header.get_gain_from_tag(TAG_ALBUM_GAIN)?.as_decibels());
    println!("{}={}dB", TAG_TRACK_GAIN, header.get_gain_from_tag(TAG_TRACK_GAIN)?.as_decibels());
    Ok(())
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

#[derive(Debug)]
enum RewriteResult {
    Replace,
    DoNothing,
}

fn remove_file_verbose<P: AsRef<Path>>(path: P) {
    let path = path.as_ref();
    if let Err(e) = std::fs::remove_file(path) {
        eprintln!("Unable to delete {} due to error {}", path.to_string_lossy(), e);
    }
}

fn rename_file<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<(), ZoopError> {
    std::fs::rename(from.as_ref(), to.as_ref()).map_err(|e| {
        ZoopError::FileCopy(PathBuf::from(from.as_ref()), PathBuf::from(to.as_ref()), e)
    })
}

fn main_impl() -> Result<(), ZoopError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { usage(); }
    let input_path = PathBuf::from(&args[1]);
    let input_file = File::open(&input_path).map_err(|e| ZoopError::FileOpenError(input_path.clone(), e))?;
    let input_file = BufReader::new(input_file);

    let input_dir = input_path.parent().expect("Unable to find parent folder of input file").clone();
    let input_base = input_path.file_name().expect("Unable to find name of input file").clone();
    let mut output_file = tempfile::Builder::new()
        .prefix(input_base)
        .suffix("zoog")
        .tempfile_in(input_dir)
        .map_err(|e| ZoopError::TempFileOpenError(e))?;

    let rewrite_result = {
        let mut output_file = BufWriter::new(&mut output_file);
        let mut ogg_reader = PacketReader::new(input_file);
        let mut ogg_writer = PacketWriter::new(&mut output_file);
        let mut state = State::AwaitingHeader;
        let mut header_gain = None;
        let result = loop {
            let packet = match ogg_reader.read_packet() {
                Err(e) => break Err(ZoopError::OggDecode(e)),
                Ok(packet) => packet,
            };
            let packet = match packet {
                None => break Ok(RewriteResult::Replace),
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
                    let original_gain = header.get_output_gain();
                    header_gain = Some(original_gain);
                    println!("Original header gain: {}dB", original_gain.as_decibels());
                    header.set_output_gain(OutputGain::from_decibels(0.0));
                    println!("New header gain: {}dB", header.get_output_gain().as_decibels());
                    state = State::AwaitingComments;
                },
                State::AwaitingComments => {
                    let mut header = match CommentHeader::try_new(&mut packet_data) {
                        Ok(Some(header)) => header,
                        Ok(None) => break Err(ZoopError::MissingCommentHeader),
                        Err(e) => break Err(e),
                    };
                    let header_gain = header_gain.expect("Could not find header output gain");
                    if header_gain.is_none() {
                        println!("Header gain is 0dB so making no changes");
                        break Ok(RewriteResult::DoNothing);
                    } else {
                        println!("\nOriginal tags gain values:");
                        if let Err(e) = print_gains(&header) { break Err(e); }
                        if let Err(e) = header.adjust_gains(header_gain) {
                            break Err(e);
                        }
                        println!("\nNew tags gain values:");
                        if let Err(e) = print_gains(&header) { break Err(e); }
                    }
                    state = State::Forwarding;
                }
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
        };
        if let Err(e) = output_file.flush() {
            Err(ZoopError::WriteError(e))
        } else {
            result
        }
    };
    match rewrite_result {
        Err(_) => {},
        Ok(RewriteResult::DoNothing) => {},
        Ok(RewriteResult::Replace) => {
            let mut backup_path = input_path.clone();
            backup_path.set_extension("zoog-orig");
            rename_file(&input_path, &backup_path)?;
            output_file.persist_noclobber(&input_path)?;
            remove_file_verbose(&backup_path);
        }
    }
    rewrite_result.map(|_| ())
}
