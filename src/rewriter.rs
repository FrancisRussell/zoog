use crate::comment_header::CommentHeader;
use crate::constants::{R128_LUFS, TAG_ALBUM_GAIN, TAG_TRACK_GAIN};
use crate::error::ZoogError;
use crate::gain::Gain;
use crate::opus_header::OpusHeader;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use ogg::Packet;
use std::collections::VecDeque;
use std::io::Write;

#[derive(Clone, Copy, Debug)]
pub enum OperationMode {
    ZeroOutputGain,
    TargetLUFS(f64),
}

impl OperationMode {
    pub fn to_friendly_string(&self) -> String {
        match *self {
            OperationMode::ZeroOutputGain => String::from("original input"),
            OperationMode::TargetLUFS(lufs) => format!("{} LUFS", lufs),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RewriteResult {
    Ready,
    NoR128Tags,
    AlreadyNormalized,
}

#[derive(Clone, Copy, Debug)]
enum State {
    AwaitingHeader,
    AwaitingComments,
    Forwarding,
}

fn print_gains<'a>(opus_header: &OpusHeader<'a>, comment_header: &CommentHeader<'a>) -> Result<(), ZoogError> {
    println!("\tOutput Gain: {}dB", opus_header.get_output_gain().as_decibels());
    for tag in [TAG_ALBUM_GAIN, TAG_TRACK_GAIN].iter() {
        if let Some(gain) = comment_header.get_gain_from_tag(tag)? {
            println!("\t{}: {}dB", tag, gain.as_decibels());
        }
    }
    Ok(())
}

pub struct Rewriter<W: Write> {
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
            }
            State::AwaitingComments => {
                // Parse Opus header
                let mut opus_header_packet = self.header_packet.take().expect("Missing header packet");
                {
                    let mut opus_header = OpusHeader::try_new(&mut opus_header_packet.data)
                        .ok_or(ZoogError::MissingOpusStream)?;
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
                            if header_gain.is_zero() {
                                return Ok(RewriteResult::AlreadyNormalized);
                            } else {
                                comment_header.adjust_gains(header_gain)?;
                            }
                        }
                        OperationMode::TargetLUFS(target_lufs) => {
                            let header_delta = Gain::from_decibels(comment_gain.as_decibels() + target_lufs - R128_LUFS);
                            let header_delta = header_delta.ok_or(ZoogError::GainOutOfBounds)?;
                            if header_delta.is_zero() { return Ok(RewriteResult::AlreadyNormalized); }
                            let comment_delta = header_delta.checked_neg().ok_or(ZoogError::GainOutOfBounds)?;
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
            }
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
            ).map_err(ZoogError::WriteError)?;
        }
        Ok(RewriteResult::Ready)
    }
}


