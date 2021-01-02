use ogg::Packet;
use ogg::reading::PacketReader;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use zoog::{Gain, OpusHeader, ZoogError, CommentHeader};
use zoog::constants::{TAG_TRACK_GAIN, TAG_ALBUM_GAIN, R128_LUFS, REPLAY_GAIN_LUFS};
use clap::{App,Arg};

pub const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
pub const AUTHORS: Option<&'static str> = option_env!("CARGO_PKG_AUTHORS");

fn get_version() -> String {
    VERSION.map(String::from).unwrap_or(String::from("Unknown version"))
}

fn get_authors() -> String {
    AUTHORS.map(String::from).unwrap_or(String::from("Unknown author"))
}

enum State {
    AwaitingHeader,
    AwaitingComments,
    Forwarding,
}

fn print_gains<'a>(opus_header: &OpusHeader<'a>, comment_header: &CommentHeader<'a>) -> Result<(), ZoogError> {
    println!("\t{}: {}db", "Output Gain", opus_header.get_output_gain().as_decibels());
    for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
        if let Some(gain) = comment_header.get_gain_from_tag(tag)? {
            println!("\t{}: {}dB", tag, gain.as_decibels());
        }
    }
    Ok(())
}

fn main() {
    match main_impl() {
        Ok(()) => {},
        Err(e) => {
            eprintln!("Error was: {}", e);
            std::process::exit(1);
        },
    }
}

#[derive(Debug)]
enum RewriteResult {
    Ready,
    NoR128Tags,
    AlreadyNormalized,
}

#[derive(Clone, Copy, Debug)]
enum OperationMode {
    ZeroOutputGain,
    TargetLUFS(f64),
}

impl OperationMode {
    fn to_friendly_string(&self) -> String {
        match *self {
            OperationMode::ZeroOutputGain => String::from("original input"),
            OperationMode::TargetLUFS(lufs) => format!("{} LUFS", lufs),
        }
    }
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

struct Rewriter<W: Write> {
    packet_writer: PacketWriter<W>,
    header_packet: Option<Packet>,
    state: State,
    packet_queue: VecDeque<Packet>,
    mode: OperationMode,
    verbose: bool,
}

impl<W: Write> Rewriter<W> {
    pub fn new(mode: OperationMode, packet_writer: PacketWriter<W>, verbose: bool) -> Rewriter<W> {
        Rewriter {
            packet_writer,
            header_packet: None,
            state: State::AwaitingHeader,
            packet_queue: VecDeque::new(),
            mode,
            verbose,
        }
    }

    pub fn submit(&mut self, mut packet: Packet) -> Result<RewriteResult, ZoogError> {
        match self.state {
            State::AwaitingHeader => {
                self.header_packet = Some(packet);
                self.state = State::AwaitingComments;
            },
            State::AwaitingComments => {
                // Parse Opus header
                let mut opus_header_packet = self.header_packet.take().expect("Missing header packet");
                {
                    let mut opus_header = if let Some(header) = OpusHeader::try_new(&mut opus_header_packet.data) {
                        header
                    } else {
                        return Err(ZoogError::MissingOpusStream)
                    };

                    // Parse comment header
                    let mut comment_header = match CommentHeader::try_new(&mut packet.data) {
                        Ok(Some(header)) => header,
                        Ok(None) => return Err(ZoogError::MissingCommentHeader),
                        Err(e) => return Err(e),
                    };

                    let header_gain = opus_header.get_output_gain();
                    let comment_gain = match comment_header.get_album_or_track_gain() {
                        Err(e) => return Err(e),
                        Ok(None) => return Ok(RewriteResult::NoR128Tags),
                        Ok(Some(gain)) => gain,
                    };
                    if self.verbose {
                        println!("Original gain values:");
                        print_gains(&opus_header, &comment_header)?;
                    }
                    match self.mode {
                        OperationMode::ZeroOutputGain => {
                            // Set Opus header gain
                            opus_header.set_output_gain(Gain::default());
                            // Set comment header gain
                            if header_gain.is_none() {
                                return Ok(RewriteResult::AlreadyNormalized);
                            } else {
                                comment_header.adjust_gains(header_gain)?;
                            }
                        }
                        OperationMode::TargetLUFS(target_lufs) => {
                            // FIXME: Check this conversion is valid
                            let header_delta = Gain::from_decibels(comment_gain.as_decibels() + target_lufs - R128_LUFS);
                            if header_delta.is_none() { return Ok(RewriteResult::AlreadyNormalized); }
                            let comment_delta = if let Some(negated) = header_delta.checked_neg() {
                                negated
                            } else {
                                return Err(ZoogError::GainOutOfBounds);
                            };
                            opus_header.adjust_output_gain(header_delta)?;
                            comment_header.adjust_gains(comment_delta)?;
                        }
                    }
                    if self.verbose {
                        println!("New gain values:");
                        print_gains(&opus_header, &comment_header)?;
                    }
                }
                self.packet_queue.push_back(opus_header_packet);
                self.packet_queue.push_back(packet);
                self.state = State::Forwarding;
            }
            State::Forwarding => {
                self.packet_queue.push_back(packet);
            },
        }

        while let Some(packet) = self.packet_queue.pop_front() {
            let packet_info = if packet.last_in_stream() {
                PacketWriteEndInfo::EndStream
            } else if packet.last_in_page() {
                PacketWriteEndInfo::EndPage
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            let packet_serial = packet.stream_serial();
            let packet_granule = packet.absgp_page();

            self.packet_writer.write_packet(packet.data.into_boxed_slice(),
                packet_serial,
                packet_info,
                packet_granule,
            ).map_err(|e| ZoogError::WriteError(e))?;
        }
        Ok(RewriteResult::Ready)
    }
}

fn main_impl() -> Result<(), ZoogError> {
    let matches = App::new("Zoog")
        .author(get_authors().as_str())
        .about("Modifies Opus output gain values and R128 tags")
        .version(get_version().as_str())
        .arg(Arg::with_name("preset")
            .long("preset")
            .possible_values(&["rg", "r128", "none"])
            .default_value("rg")
            .multiple(false)
            .help("Normalizes to loudness used by ReplayGain (rg), EBU R 128 (r128) or original (none)"))
        .arg(Arg::with_name("input_files")
            .multiple(true)
            .required(true)
            .help("The Opus files to process"))
        .get_matches();

    let mode = match matches.value_of("preset").unwrap() {
        "rg" => OperationMode::TargetLUFS(REPLAY_GAIN_LUFS),
        "r128" => OperationMode::TargetLUFS(R128_LUFS),
        "none" => OperationMode::ZeroOutputGain,
        p => panic!("Unknown preset: {}", p),
    };

    let mut num_processed: usize = 0;
    let mut num_already_normalized: usize = 0;
    let mut num_missing_r128: usize = 0;

    let input_files = matches.values_of("input_files").expect("No input files");
    for input_path in input_files {
        let input_path = PathBuf::from(input_path);
        println!("Processing file {:#?} with target loudness of {}...", input_path, mode.to_friendly_string());
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
            let mut ogg_reader = PacketReader::new(input_file);
            let mut output_file = BufWriter::new(&mut output_file);
            let ogg_writer = PacketWriter::new(&mut output_file);
            let mut rewriter = Rewriter::new(mode, ogg_writer, true);
            loop {
                match ogg_reader.read_packet() {
                    Err(e) => break Err(ZoogError::OggDecode(e)),
                    Ok(None) => {
                        // Make sure to flush the buffered writer
                        break output_file.flush()
                            .map(|_| RewriteResult::Ready)
                            .map_err(|e| ZoogError::WriteError(e));
                    },
                    Ok(Some(packet)) => {
                        let submit_result = rewriter.submit(packet);
                        match submit_result {
                            Ok(RewriteResult::Ready) => {},
                            _ => break submit_result,
                        }
                    },
                }
            }
        };

        num_processed += 1;
        match rewrite_result {
            Err(e) => {
                println!("Failure during processing of {:#?}.", input_path);
                return Err(e)
            },
            Ok(RewriteResult::Ready) => {
                let mut backup_path = input_path.clone();
                backup_path.set_extension("zoog-orig");
                rename_file(&input_path, &backup_path)?;
                output_file.persist_noclobber(&input_path)?;
                remove_file_verbose(&backup_path);
            }
            Ok(RewriteResult::NoR128Tags) => {
                println!("No R128 tags found in file so doing nothing.");
                num_missing_r128 += 1;
            },
            Ok(RewriteResult::AlreadyNormalized) => {
                println!("All gains are already correct so doing nothing.");
                num_already_normalized += 1;
            },
        }
        println!("");
    }
    println!("Processing complete.");
    println!("Total files processed: {}", num_processed);
    println!("Files processed but already normalized: {}", num_already_normalized);
    println!("Files skipped due to missing R128 tags: {}", num_missing_r128);
    Ok(())
}
