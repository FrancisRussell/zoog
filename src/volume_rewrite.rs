use std::convert::{Into, TryFrom};

use crate::header::{CommentList, FixedPointGain};
use crate::header_rewriter::{CodecHeaders, HeaderRewrite, HeaderSummarize};
use crate::opus::{TAG_ALBUM_GAIN, TAG_TRACK_GAIN};
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

/// Configuration type for `VolumeRewriter`
#[derive(Clone, Copy, Debug)]
pub struct VolumeRewriterConfig {
    /// The target output gain
    pub output_gain: VolumeTarget,

    /// Whether the rewritten output gain should target track or album volume
    pub output_gain_mode: OutputGainMode,

    /// The pre-computed volume of the track to be rewritten (if available)
    pub track_volume: Option<Decibels>,

    /// The pre-computed volume of the album the track belongs to (if available)
    pub album_volume: Option<Decibels>,
}

impl VolumeRewriterConfig {
    /// Computes the source volume that will be used for the output gain
    /// calculation
    pub fn volume_for_output_gain_calculation(&self) -> Option<Decibels> {
        match self.output_gain_mode {
            OutputGainMode::Album => self.album_volume,
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

/// Returns the gains from the codec headers
#[derive(Debug, Default)]
pub struct GainsSummary {}

impl HeaderSummarize for GainsSummary {
    type Error = Error;
    type Summary = OpusGains;

    fn summarize(&self, headers: &CodecHeaders) -> Result<OpusGains, Error> {
        match headers {
            CodecHeaders::Opus(opus_header, comment_header) => {
                let gains = OpusGains {
                    output: opus_header.get_output_gain().into(),
                    track_r128: comment_header.get_gain_from_tag(TAG_TRACK_GAIN).unwrap_or(None).map(Into::into),
                    album_r128: comment_header.get_gain_from_tag(TAG_ALBUM_GAIN).unwrap_or(None).map(Into::into),
                };
                Ok(gains)
            }
            #[allow(unreachable_patterns)]
            _ => Err(Error::UnsupportedCodec(headers.codec())),
        }
    }
}

/// Parameterization struct for `HeaderRewriter` to rewrite ouput gain and R128
/// tags.
#[derive(Debug)]
pub struct VolumeHeaderRewrite {
    config: VolumeRewriterConfig,
}

impl VolumeHeaderRewrite {
    pub fn new(config: VolumeRewriterConfig) -> VolumeHeaderRewrite { VolumeHeaderRewrite { config } }
}

impl HeaderRewrite for VolumeHeaderRewrite {
    type Error = Error;

    fn rewrite(&self, headers: &mut CodecHeaders) -> Result<(), Error> {
        match headers {
            CodecHeaders::Opus(opus_header, comment_header) => {
                let new_header_gain = match self.config.output_gain {
                    VolumeTarget::ZeroGain => FixedPointGain::default(),
                    VolumeTarget::LUFS(target_lufs) => {
                        let volume_for_output_gain = self
                            .config
                            .volume_for_output_gain_calculation()
                            .expect("Precomputed volume unexpectedly missing");
                        FixedPointGain::try_from(target_lufs - volume_for_output_gain)?
                    }
                    VolumeTarget::NoChange => opus_header.get_output_gain(),
                };
                opus_header.set_output_gain(new_header_gain);
                let compute_gain = |volume| -> Result<Option<FixedPointGain>, Error> {
                    if let Some(volume) = volume {
                        FixedPointGain::try_from(R128_LUFS - volume - new_header_gain.into()).map(Some)
                    } else {
                        Ok(None)
                    }
                };
                let track_gain_r128 = compute_gain(self.config.track_volume)?;
                let album_gain_r128 = compute_gain(self.config.album_volume)?;
                for (tag, gain) in [(TAG_TRACK_GAIN, track_gain_r128), (TAG_ALBUM_GAIN, album_gain_r128)] {
                    if let Some(gain) = gain {
                        comment_header.set_tag_to_gain(tag, gain)?;
                    } else {
                        comment_header.remove_all(tag);
                    }
                }
                Ok(())
            }
            #[allow(unreachable_patterns)]
            _ => Err(Error::UnsupportedCodec(headers.codec())),
        }
    }
}
