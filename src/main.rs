use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use derivative::Derivative;
use ogg::reading::{OggReadError, PacketReader};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::collections::VecDeque;
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
const R128_LUFS: f64 = -23.0;

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
        self.value.checked_add(rhs.value).map(|value| OutputGain { value })
    }

    fn checked_neg(&self) -> Option<OutputGain> {
        self.value.checked_neg().map(|value| OutputGain { value })
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

    fn adjust_output_gain(&mut self, adjustment: OutputGain) -> Result<(), ZoopError> {
        let gain = self.get_output_gain();
        if let Some(gain) = gain.checked_add(&adjustment) {
            self.set_output_gain(gain);
            Ok(())
        } else {
            Err(ZoopError::GainOutOfBounds)
        }
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

    fn get_gain_from_tag(&self, tag: &str) -> Result<Option<OutputGain>, ZoopError> {
        let parsed = self.get_first(tag)
            .map(|v| v.parse::<OutputGain>().map_err(|_| ZoopError::InvalidR128Tag));
        match parsed {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    fn get_album_or_track_gain(&self) -> Result<Option<OutputGain>, ZoopError> {
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(*tag)? {
                return Ok(Some(gain))
            }
        }
        return Ok(None)
    }

    fn adjust_gains(&mut self, adjustment: OutputGain) -> Result<(), ZoopError> {
        if adjustment.is_none() { return Ok(()); }
        for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
            if let Some(gain) = self.get_gain_from_tag(*tag)? {
                let gain = if let Some(gain) = gain.checked_add(&adjustment) {
                    gain
                } else {
                    return Err(ZoopError::GainOutOfBounds);
                };
                self.replace(*tag, &format!("{}", gain.as_fixed_point()));
            }
        }
        Ok(())
    }

    fn commit(&mut self) {
        //TODO: Look more into why we can't use https://github.com/rust-lang/rust/pull/46830
        let mut writer = Cursor::new(Vec::new());
        writer.write_all(COMMENT_MAGIC).unwrap();
        let vendor = self.vendor.as_bytes();
        writer.write_u32::<LittleEndian>(vendor.len() as u32).unwrap();
        writer.write_all(vendor).unwrap();
        writer.write_u32::<LittleEndian>(self.user_comments.len() as u32).unwrap();
        let equals: &[u8] = &[0x3d];
        for (k, v) in self.user_comments.iter().map(|(k, v)| (k.as_bytes(), v.as_bytes())) {
            let len = k.len() + v.len() + 1;
            writer.write_u32::<LittleEndian>(len as u32).unwrap();
            writer.write_all(k).unwrap();
            writer.write_all(equals).unwrap();
            writer.write_all(v).unwrap();
        }
        *self.data = writer.into_inner();
    }
}

impl<'a> Drop for CommentHeader<'a> {
    fn drop(&mut self) {
        self.commit();
    }
}

fn print_gains<'a>(opus_header: &OpusHeader<'a>, comment_header: &CommentHeader<'a>) -> Result<(), ZoopError> {
    println!("{}: {}db", "Output Gain", opus_header.get_output_gain().as_decibels());
    for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
        if let Some(gain) = comment_header.get_gain_from_tag(tag)? {
            println!("{}: {}dB", tag, gain.as_decibels());
        }
    }
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

enum OperationMode {
    ZeroInputGain,
    TargetLUFS(f64),
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
    let mode = OperationMode::TargetLUFS(-18.0);
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
        let mut header_packet = None;
        let mut packet_queue = VecDeque::new();
        let result = loop {
            let packet = match ogg_reader.read_packet() {
                Err(e) => break Err(ZoopError::OggDecode(e)),
                Ok(packet) => packet,
            };
            let mut packet = match packet {
                None => break Ok(RewriteResult::Replace),
                Some(packet) => packet,
            };
            match state {
                State::AwaitingHeader => {
                    header_packet = Some(packet);
                    state = State::AwaitingComments;
                },
                State::AwaitingComments => {
                    // Parse Opus header
                    let mut opus_header_packet = header_packet.take().expect("Missing header packet");
                    {
                        let mut opus_header = if let Some(header) = OpusHeader::try_new(&mut opus_header_packet.data) {
                            header
                        } else {
                            break Err(ZoopError::MissingOpusStream)
                        };

                        // Parse comment header
                        let mut comment_header = match CommentHeader::try_new(&mut packet.data) {
                            Ok(Some(header)) => header,
                            Ok(None) => break Err(ZoopError::MissingCommentHeader),
                            Err(e) => break Err(e),
                        };

                        let header_gain = opus_header.get_output_gain();
                        let comment_gain = match comment_header.get_album_or_track_gain() {
                            Err(e) => break Err(e),
                            Ok(None) => {
                                eprintln!("No R128 tags detected so doing nothing to this file");
                                break Ok(RewriteResult::DoNothing);
                            },
                            Ok(Some(gain)) => gain,
                        };

                        println!("\nOriginal gain values:");
                        if let Err(e) = print_gains(&opus_header, &comment_header) { break Err(e); }
                        match mode {
                            OperationMode::ZeroInputGain => {
                                // Set Opus header gain
                                opus_header.set_output_gain(OutputGain::default());
                                // Set comment header gain
                                if header_gain.is_none() {
                                    println!("Output gain is already 0dB so not making any changes");
                                    break Ok(RewriteResult::DoNothing);
                                } else {
                                    if let Err(e) = comment_header.adjust_gains(header_gain) { break Err(e); }
                                }
                            }
                            OperationMode::TargetLUFS(target_lufs) => {
                                // FIXME: Check this conversion is valid
                                let header_delta = OutputGain::from_decibels(comment_gain.as_decibels() + target_lufs - R128_LUFS);
                                let comment_delta = if let Some(negated) = header_delta.checked_neg() {
                                    negated
                                } else {
                                    break Err(ZoopError::GainOutOfBounds);
                                };
                                if let Err(e) = opus_header.adjust_output_gain(header_delta) { break Err(e); }
                                if let Err(e) = comment_header.adjust_gains(comment_delta) { break Err(e); }
                            }
                        }
                        println!("\nNew gain values:");
                        if let Err(e) = print_gains(&opus_header, &comment_header) { break Err(e); }
                    }
                    packet_queue.push_back(opus_header_packet);
                    packet_queue.push_back(packet);
                    state = State::Forwarding;
                }
                State::Forwarding => {
                    packet_queue.push_back(packet);
                },
            }

            while let Some(packet) = packet_queue.pop_front() {
                let packet_info = if packet.last_in_stream() {
                    PacketWriteEndInfo::EndStream
                } else if packet.last_in_page() {
                    PacketWriteEndInfo::EndPage
                } else {
                    PacketWriteEndInfo::NormalPacket
                };
                let packet_serial = packet.stream_serial();
                let packet_granule = packet.absgp_page();

                ogg_writer.write_packet(packet.data.into_boxed_slice(),
                    packet_serial,
                    packet_info,
                    packet_granule,
                ).map_err(|e| ZoopError::WriteError(e))?;
            }
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
