use std::collections::VecDeque;
use std::convert::TryFrom;
use std::io::Write;

use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use ogg::Packet;

use crate::opus::{CommentHeader, FixedPointGain, OpusHeader, TAG_ALBUM_GAIN, TAG_TRACK_GAIN};
use crate::{Decibels, Error, R128_LUFS};

/// Represents a target gain for an audio stream
#[derive(Clone, Copy, Debug)]
pub enum VolumeTarget {
    /// No gain relative to the original stream
    ZeroGain,

    /// A target volume for a track or album relative to full scale.
    LUFS(Decibels),

    /// The gain should remain the same as it already is
    NoChange,
}

/// Represents whether output gain relative to full scale should be targetted to
/// track volume or album volume
#[derive(Clone, Copy, Debug)]
pub enum OutputGainMode {
    Album,
    Track,
}

/// Configuration type for `Rewriter`
#[derive(Clone, Copy, Debug)]
pub struct RewriterConfig {
    /// The target output gain
    pub output_gain: VolumeTarget,

    /// Whether the rewritten output gain should target track or album volume
    pub output_gain_mode: OutputGainMode,

    /// The pre-computed volume of the track to be rewritten
    pub track_volume: Decibels,

    /// The precomputed volume of the album the track belongs to (if available)
    pub album_volume: Option<Decibels>,
}

impl RewriterConfig {
    /// Computes the source volume that will be used for the output gain
    /// calculation
    pub fn volume_for_output_gain_calculation(&self) -> Decibels {
        match self.output_gain_mode {
            OutputGainMode::Album => self.album_volume.unwrap_or(self.track_volume),
            OutputGainMode::Track => self.track_volume,
        }
    }
}

impl VolumeTarget {
    /// A description intended to be friendly for printing
    pub fn to_friendly_string(&self) -> String {
        match *self {
            VolumeTarget::ZeroGain => String::from("original input"),
            VolumeTarget::LUFS(lufs) => format!("{:.2} LUFS", lufs.as_f64()),
            VolumeTarget::NoChange => String::from("existing gain value"),
        }
    }
}

/// The gain values of an Opus file
#[derive(Clone, Copy, Debug)]
pub struct OpusGains {
    /// The output gain that is always applied to the decoded audio
    pub output: Decibels,

    /// The track gain from the Opus comment header to reach -23 LUFS
    pub track_r128: Option<Decibels>,

    /// The album gain from the Opus comment header to reach -23 LUFS
    pub album_r128: Option<Decibels>,
}

/// Represents the non-erroneous result of a packet submit operation
#[derive(Clone, Copy, Debug)]
pub enum SubmitResult {
    /// Packet was accepted
    Good,

    /// The stream is already normalized so there is no need to rewrite it. The
    /// existing gains are returned.
    AlreadyNormalized(OpusGains),

    /// The gains of the stream will be changed from `from` to `to`.
    ChangingGains { from: OpusGains, to: OpusGains },
}

#[derive(Clone, Copy, Debug)]
enum State {
    AwaitingHeader,
    AwaitingComments,
    Forwarding,
}

/// Re-writes an Ogg Opus stream with new output gain and comment gain values
pub struct Rewriter<'a, W: Write> {
    packet_writer: PacketWriter<'a, W>,
    header_packet: Option<Packet>,
    state: State,
    packet_queue: VecDeque<Packet>,
    config: RewriterConfig,
}

impl<W: Write> Rewriter<'_, W> {
    /// Constructs a new rewriter
    /// - `config` - the configuration for volume rewriting.
    /// - `packet_writer` - the Ogg stream writer that the rewritten packets
    ///   will be sent to.
    pub fn new<'a>(config: &RewriterConfig, packet_writer: PacketWriter<'a, W>) -> Rewriter<'a, W> {
        Rewriter {
            packet_writer,
            header_packet: None,
            state: State::AwaitingHeader,
            packet_queue: VecDeque::new(),
            config: *config,
        }
    }

    /// Submits a new packet to the rewriter. If `Ready` is returned, another
    /// packet from the same stream should continue to be submitted. If
    /// `AlreadyNormalized` is returned, the supplied stream did not need
    /// any alterations. In this case, the partial output should be discarded
    /// and no further packets submitted.
    pub fn submit(&mut self, mut packet: Packet) -> Result<SubmitResult, Error> {
        match self.state {
            State::AwaitingHeader => {
                self.header_packet = Some(packet);
                self.state = State::AwaitingComments;
            }
            State::AwaitingComments => {
                // Parse Opus header
                let mut opus_header_packet = self.header_packet.take().expect("Missing header packet");
                let (existing_gains, new_gains, changed) = {
                    // Create copies of Opus and comment header to check if they have changed
                    let mut opus_header_packet_data_orig = opus_header_packet.data.clone();
                    let mut comment_header_data_orig = packet.data.clone();

                    // Parse Opus header
                    let mut opus_header =
                        OpusHeader::try_parse(&mut opus_header_packet.data)?.ok_or(Error::MissingOpusStream)?;
                    // Parse comment header
                    let mut comment_header = match CommentHeader::try_parse(&mut packet.data) {
                        Ok(Some(header)) => header,
                        Ok(None) => return Err(Error::MissingCommentHeader),
                        Err(e) => return Err(e),
                    };
                    let existing_gains = OpusGains {
                        output: opus_header.get_output_gain().as_decibels(),
                        track_r128: comment_header
                            .get_gain_from_tag(TAG_TRACK_GAIN)
                            .unwrap_or(None)
                            .map(|g| g.as_decibels()),
                        album_r128: comment_header
                            .get_gain_from_tag(TAG_ALBUM_GAIN)
                            .unwrap_or(None)
                            .map(|g| g.as_decibels()),
                    };
                    let volume_for_output_gain = self.config.volume_for_output_gain_calculation();
                    let new_header_gain = match self.config.output_gain {
                        VolumeTarget::ZeroGain => FixedPointGain::default(),
                        VolumeTarget::LUFS(target_lufs) => {
                            FixedPointGain::try_from(target_lufs - volume_for_output_gain)?
                        }
                        VolumeTarget::NoChange => opus_header.get_output_gain(),
                    };
                    let track_gain_r128 =
                        FixedPointGain::try_from(R128_LUFS - self.config.track_volume - new_header_gain.as_decibels())?;
                    let album_gain_r128 = if let Some(album_volume) = self.config.album_volume {
                        Some(FixedPointGain::try_from(R128_LUFS - album_volume - new_header_gain.as_decibels())?)
                    } else {
                        None
                    };
                    let new_gains = OpusGains {
                        output: new_header_gain.as_decibels(),
                        track_r128: Some(track_gain_r128.as_decibels()),
                        album_r128: album_gain_r128.map(|g| g.as_decibels()),
                    };
                    opus_header.set_output_gain(new_header_gain);
                    comment_header.replace(TAG_TRACK_GAIN, &format!("{}", track_gain_r128.as_fixed_point()));
                    if let Some(album_gain_r128) = album_gain_r128 {
                        comment_header.replace(TAG_ALBUM_GAIN, &format!("{}", album_gain_r128.as_fixed_point()));
                    } else {
                        comment_header.remove_all(TAG_ALBUM_GAIN);
                    }

                    // We have decoded both of these already, so these should never fail
                    let opus_header_orig = OpusHeader::try_parse(&mut opus_header_packet_data_orig)
                        .expect("Opus header unexpectedly invalid")
                        .expect("Unexpectedly failed to find Opus header");
                    let comment_header_orig = CommentHeader::try_parse(&mut comment_header_data_orig)
                        .expect("Unexpectedly failed to decode comment header")
                        .expect("Comment header unexpectedly missing");

                    // We compare headers rather than the values of the `OpusGains` structs because
                    // using the latter glosses over issues such as duplicate or invalid gain tags
                    // which we will fix if present.
                    let changed = (opus_header != opus_header_orig) || (comment_header != comment_header_orig);
                    (existing_gains, new_gains, changed)
                };
                self.packet_queue.push_back(opus_header_packet);
                self.packet_queue.push_back(packet);
                self.state = State::Forwarding;

                return Ok(if changed {
                    SubmitResult::ChangingGains { from: existing_gains, to: new_gains }
                } else {
                    SubmitResult::AlreadyNormalized(existing_gains)
                });
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

            self.packet_writer
                .write_packet(packet.data, packet_serial, packet_info, packet_granule)
                .map_err(Error::WriteError)?;
        }
        Ok(SubmitResult::Good)
    }
}
