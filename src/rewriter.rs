use crate::{CommentHeader, Error, FixedPointGain, OpusHeader, R128_LUFS, TAG_ALBUM_GAIN, TAG_TRACK_GAIN};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use ogg::Packet;
use std::collections::VecDeque;
use std::io::Write;

#[derive(Clone, Copy, Debug)]
pub enum VolumeTarget {
    ZeroGain,
    LUFS(f64),
}

#[derive(Clone, Copy, Debug)]
pub struct RewriterConfig {
    internal_gain: VolumeTarget,
    track_volume: f64,
    album_volume: Option<f64>,
}

impl RewriterConfig {
    pub fn new(internal_gain: VolumeTarget, track_volume: f64, album_volume: Option<f64>) -> RewriterConfig {
        RewriterConfig {
            internal_gain,
            track_volume,
            album_volume,
        }
    }

    pub fn volume_for_internal_gain_calculation(&self) -> f64 {
        self.album_volume.unwrap_or(self.track_volume)
    }
}

impl VolumeTarget {
    pub fn to_friendly_string(&self) -> String {
        match *self {
            VolumeTarget::ZeroGain => String::from("original input"),
            VolumeTarget::LUFS(lufs) => format!("{} LUFS", lufs),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RewriteResult {
    Ready,
    AlreadyNormalized,
}

#[derive(Clone, Copy, Debug)]
enum State {
    AwaitingHeader,
    AwaitingComments,
    Forwarding,
}

fn print_gains<'a>(opus_header: &OpusHeader<'a>, comment_header: &CommentHeader<'a>) -> Result<(), Error> {
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
    config: RewriterConfig,
    verbose: bool,
}

impl<W: Write> Rewriter<W> {
    pub fn new(config: &RewriterConfig, packet_writer: PacketWriter<W>, verbose: bool) -> Rewriter<W> {
        Rewriter {
            packet_writer,
            header_packet: None,
            state: State::AwaitingHeader,
            packet_queue: VecDeque::new(),
            config: *config,
            verbose,
        }
    }

    pub fn submit(&mut self, mut packet: Packet) -> Result<RewriteResult, Error> {
        match self.state {
            State::AwaitingHeader => {
                self.header_packet = Some(packet);
                self.state = State::AwaitingComments;
            }
            State::AwaitingComments => {
                // Parse Opus header
                let mut opus_header_packet = self.header_packet.take().expect("Missing header packet");
                {
                    // Create copies of Opus and comment header to check if they have changed
                    let mut opus_header_packet_data_orig = opus_header_packet.data.clone();
                    let mut comment_header_data_orig = packet.data.clone();

                    // Parse Opus header
                    let mut opus_header = OpusHeader::try_new(&mut opus_header_packet.data)
                        .ok_or(Error::MissingOpusStream)?;
                    // Parse comment header
                    let mut comment_header = match CommentHeader::try_parse(&mut packet.data) {
                        Ok(Some(header)) => header,
                        Ok(None) => return Err(Error::MissingCommentHeader),
                        Err(e) => return Err(e),
                    };
                    let volume_for_internal_gain = self.config.volume_for_internal_gain_calculation();
                    let new_header_gain = match self.config.internal_gain {
                        VolumeTarget::ZeroGain => FixedPointGain::default(),
                        VolumeTarget::LUFS(target_lufs) => {
                            FixedPointGain::from_decibels(target_lufs - volume_for_internal_gain)
                                .expect("Header gain out of bounds")
                        }
                    };
                    let track_gain_r128 = FixedPointGain::from_decibels(
                        R128_LUFS - self.config.track_volume - new_header_gain.as_decibels()
                    ).expect("Track gain out of bounds");
                    let album_gain_r128 = self.config.album_volume.map(|album_volume| {
                        FixedPointGain::from_decibels(R128_LUFS - album_volume - new_header_gain.as_decibels())
                            .expect("Album gain out of bounds")
                    });
                    opus_header.set_output_gain(new_header_gain);
                    comment_header.replace(TAG_TRACK_GAIN, &format!("{}", track_gain_r128.as_fixed_point()));
                    if let Some(album_gain_r128) = album_gain_r128 {
                        comment_header.replace(TAG_ALBUM_GAIN, &format!("{}", album_gain_r128.as_fixed_point()));
                    } else {
                        comment_header.remove_all(TAG_ALBUM_GAIN);
                    }

                    // We have decoded both of these already, so these should never fail
                    let opus_header_orig = OpusHeader::try_new(&mut opus_header_packet_data_orig)
                        .expect("Unexpectedly failed to decode Opus header");
                    let comment_header_orig = CommentHeader::try_parse(&mut comment_header_data_orig)
                        .expect("Unexpectedly failed to decode comment header")
                        .expect("Comment header unexpectedly missing");

                    if opus_header == opus_header_orig && comment_header == comment_header_orig {
                        return Ok(RewriteResult::AlreadyNormalized);
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
            ).map_err(Error::WriteError)?;
        }
        Ok(RewriteResult::Ready)
    }
}
