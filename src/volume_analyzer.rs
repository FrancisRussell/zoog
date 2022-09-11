use crate::comment_header::CommentHeader;
use crate::error::ZoogError;
use crate::gain::Gain;
use crate::opus_header::OpusHeader;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use ogg::Packet;
use opus::{Channels, Decoder};

// Opus uses this internally so we decode to this regardless of the input file sampling rate
const OPUS_DECODE_SAMPLE_RATE: u32 = 48000;

// Specified in RFC6716
const OPUS_MAX_PACKET_DURATION_MS: usize = 120;

#[derive(Clone, Copy, Debug)]
enum State {
    AwaitingHeader,
    AwaitingComments,
    Analyzing,
}

struct DecodeState {
    channel_count: usize,
    decoder: Decoder,
}

pub struct VolumeAnalyzer {
    decode_state: Option<DecodeState>,
    state: State,
    verbose: bool,
}



impl VolumeAnalyzer {
    pub fn new(verbose: bool) -> VolumeAnalyzer {
        VolumeAnalyzer {
            decode_state: None,
            state: State::AwaitingHeader,
            verbose,
        }
    }

    pub fn submit(&mut self, mut packet: Packet) -> Result<(), ZoogError> {
        match self.state {
            State::AwaitingHeader => {
                let header = OpusHeader::try_new(&mut packet.data)
                        .ok_or(ZoogError::MissingOpusStream)?;
                let channel_count = header.num_output_channels()?;
                let channels = match channel_count {
                    1 => Channels::Mono,
                    2 => Channels::Stereo,
                    n => return Err(ZoogError::InvalidChannelCount(n)),
                };
                let decoder = Decoder::new(OPUS_DECODE_SAMPLE_RATE, channels)
                    .map_err(|e| ZoogError::OpusError(e))?;
                self.decode_state = Some(DecodeState {
                    channel_count,
                    decoder,
                });
                self.state = State::AwaitingComments;
            }
            State::AwaitingComments => {
                // Check comment header is valid
                match CommentHeader::try_parse(&mut packet.data) {
                    Ok(Some(header)) => (),
                    Ok(None) => return Err(ZoogError::MissingCommentHeader),
                    Err(e) => return Err(e),
                }
                self.state = State::Analyzing;
            }
            State::Analyzing => {
                let decode_state = self.decode_state.as_mut().expect("Decode state unexpectedly missing");
                let decoder = &mut decode_state.decoder;
                let decode_fec = true;
                let num_samples = decoder.get_nb_samples(&packet.data)
                    .map_err(|e| ZoogError::OpusError(e))?;
                let mut decode_buffer = vec![0.0f32; num_samples * decode_state.channel_count];
                let num_decoded_samples = decoder.decode_float(&packet.data, &mut decode_buffer, decode_fec)
                    .map_err(|e| ZoogError::OpusError(e))?;
                assert_eq!(num_samples, num_decoded_samples, "Decoded incorrect number of samples");
                println!("Decoded {} samples", num_samples);
            }
        }
        Ok(())
    }
}
