use ogg::reading::PacketReader;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use zoog::{Gain, OpusHeader, ZoogError, CommentHeader};
use zoog::constants::{TAG_TRACK_GAIN, TAG_ALBUM_GAIN, R128_LUFS};

enum State {
    AwaitingHeader,
    AwaitingComments,
    Forwarding,
}

fn print_gains<'a>(opus_header: &OpusHeader<'a>, comment_header: &CommentHeader<'a>) -> Result<(), ZoogError> {
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

fn rename_file<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<(), ZoogError> {
    std::fs::rename(from.as_ref(), to.as_ref()).map_err(|e| {
        ZoogError::FileCopy(PathBuf::from(from.as_ref()), PathBuf::from(to.as_ref()), e)
    })
}

fn main_impl() -> Result<(), ZoogError> {
    let mode = OperationMode::TargetLUFS(-18.0);
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { usage(); }
    let input_path = PathBuf::from(&args[1]);
    let input_file = File::open(&input_path).map_err(|e| ZoogError::FileOpenError(input_path.clone(), e))?;
    let input_file = BufReader::new(input_file);

    let input_dir = input_path.parent().expect("Unable to find parent folder of input file").clone();
    let input_base = input_path.file_name().expect("Unable to find name of input file").clone();
    let mut output_file = tempfile::Builder::new()
        .prefix(input_base)
        .suffix("zoog")
        .tempfile_in(input_dir)
        .map_err(|e| ZoogError::TempFileOpenError(e))?;

    let rewrite_result = {
        let mut output_file = BufWriter::new(&mut output_file);
        let mut ogg_reader = PacketReader::new(input_file);
        let mut ogg_writer = PacketWriter::new(&mut output_file);
        let mut state = State::AwaitingHeader;
        let mut header_packet = None;
        let mut packet_queue = VecDeque::new();
        let result = loop {
            let packet = match ogg_reader.read_packet() {
                Err(e) => break Err(ZoogError::OggDecode(e)),
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
                            break Err(ZoogError::MissingOpusStream)
                        };

                        // Parse comment header
                        let mut comment_header = match CommentHeader::try_new(&mut packet.data) {
                            Ok(Some(header)) => header,
                            Ok(None) => break Err(ZoogError::MissingCommentHeader),
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
                                opus_header.set_output_gain(Gain::default());
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
                                let header_delta = Gain::from_decibels(comment_gain.as_decibels() + target_lufs - R128_LUFS);
                                let comment_delta = if let Some(negated) = header_delta.checked_neg() {
                                    negated
                                } else {
                                    break Err(ZoogError::GainOutOfBounds);
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
                ).map_err(|e| ZoogError::WriteError(e))?;
            }
        };
        if let Err(e) = output_file.flush() {
            Err(ZoogError::WriteError(e))
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
