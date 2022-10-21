use std::collections::VecDeque;
use std::io::{Read, Seek, Write};

use derivative::Derivative;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use ogg::{Packet, PacketReader};

use crate::opus::{CommentHeader, OpusHeader};
use crate::Error;

#[derive(Debug)]
pub enum SubmitResult<S> {
    /// Packet was accepted
    Good,

    /// The stream is already normalized so there is no need to rewrite it. The
    /// existing gains are returned.
    HeadersUnchanged(S),

    /// The gains of the stream will be changed from `from` to `to`.
    HeadersChanged { from: S, to: S },
}

#[derive(Clone, Copy, Debug)]
enum State {
    AwaitingHeader,
    AwaitingComments,
    Forwarding,
}

pub trait HeaderRewrite {
    type Summary;
    type Error;
    fn summarize(&self, opus_header: &OpusHeader, comment_header: &CommentHeader)
        -> Result<Self::Summary, Self::Error>;
    fn rewrite(&self, opus_header: &mut OpusHeader, comment_header: &mut CommentHeader) -> Result<(), Self::Error>;
}

/// Re-writes an Ogg Opus stream with new output gain and comment gain values
#[derive(Derivative)]
#[derivative(Debug)]
pub struct HeaderRewriter<'a, HR: HeaderRewrite, W: Write> {
    #[derivative(Debug = "ignore")]
    packet_writer: PacketWriter<'a, W>,
    #[derivative(Debug = "ignore")]
    header_packet: Option<Packet>,
    state: State,
    #[derivative(Debug = "ignore")]
    packet_queue: VecDeque<Packet>,
    header_rewrite: HR,
}

impl<HR: HeaderRewrite, W: Write> HeaderRewriter<'_, HR, W> {
    /// Constructs a new rewriter
    /// - `config` - the configuration for volume rewriting.
    /// - `packet_writer` - the Ogg stream writer that the rewritten packets
    ///   will be sent to.
    pub fn new(rewrite: HR, packet_writer: PacketWriter<W>) -> HeaderRewriter<HR, W> {
        HeaderRewriter {
            packet_writer,
            header_packet: None,
            state: State::AwaitingHeader,
            packet_queue: VecDeque::new(),
            header_rewrite: rewrite,
        }
    }

    /// Submits a new packet to the rewriter. If `Ready` is returned, another
    /// packet from the same stream should continue to be submitted. If
    /// `HeadersUnchanged` is returned, the supplied stream did not need
    /// any alterations. In this case, the partial output should be discarded
    /// and no further packets submitted.
    pub fn submit(&mut self, mut packet: Packet) -> Result<SubmitResult<HR::Summary>, HR::Error>
    where
        HR::Error: From<Error>,
    {
        match self.state {
            State::AwaitingHeader => {
                self.header_packet = Some(packet);
                self.state = State::AwaitingComments;
            }
            State::AwaitingComments => {
                // Parse Opus header
                let mut opus_header_packet = self.header_packet.take().expect("Missing header packet");
                let (summary_before, summary_after, changed) = {
                    // Create copies of Opus and comment header to check if they have changed
                    let mut opus_header_packet_data_orig = opus_header_packet.data.clone();
                    let mut comment_header_data_orig = packet.data.clone();

                    // Parse Opus header
                    let mut opus_header =
                        OpusHeader::try_parse(&mut opus_header_packet.data)?.ok_or(Error::MissingOpusStream)?;
                    // Parse comment header
                    let mut comment_header = match CommentHeader::try_parse(&mut packet.data) {
                        Ok(Some(header)) => header,
                        Ok(None) => return Err(Error::MissingCommentHeader.into()),
                        Err(e) => return Err(e.into()),
                    };
                    let summary_before = self.header_rewrite.summarize(&opus_header, &comment_header)?;
                    self.header_rewrite.rewrite(&mut opus_header, &mut comment_header)?;
                    let summary_after = self.header_rewrite.summarize(&opus_header, &comment_header)?;

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
                    (summary_before, summary_after, changed)
                };
                self.packet_queue.push_back(opus_header_packet);
                self.packet_queue.push_back(packet);
                self.state = State::Forwarding;

                return Ok(if changed {
                    SubmitResult::HeadersChanged { from: summary_before, to: summary_after }
                } else {
                    SubmitResult::HeadersUnchanged(summary_before)
                });
            }
            State::Forwarding => {
                self.packet_queue.push_back(packet);
            }
        }

        while let Some(packet) = self.packet_queue.pop_front() {
            let packet_info = Self::packet_write_end_info(&packet);
            let packet_serial = packet.stream_serial();
            let packet_granule = packet.absgp_page();

            self.packet_writer
                .write_packet(packet.data, packet_serial, packet_info, packet_granule)
                .map_err(Error::WriteError)?;
        }
        Ok(SubmitResult::Good)
    }

    fn packet_write_end_info(packet: &Packet) -> PacketWriteEndInfo {
        if packet.last_in_stream() {
            PacketWriteEndInfo::EndStream
        } else if packet.last_in_page() {
            PacketWriteEndInfo::EndPage
        } else {
            PacketWriteEndInfo::NormalPacket
        }
    }
}

/// Convenience function for performing a rewrite. Rewrites the headers of an
/// Ogg Opus stream using the supplied `HeaderRewrite`. If `abort_on_unchanged`
/// is set, the function will terminate immediately if it is detected that no
/// headers were modified, otherwise it will continue to rewrite the stream
/// until the input stream is exhausted or an error occurs.
pub fn rewrite_stream<HR: HeaderRewrite, R: Read + Seek, W: Write>(
    rewrite: HR, input: R, mut output: W, abort_on_unchanged: bool,
) -> Result<SubmitResult<HR::Summary>, HR::Error>
where
    HR::Error: From<Error>,
{
    let mut ogg_reader = PacketReader::new(input);
    let ogg_writer = PacketWriter::new(&mut output);
    let mut rewriter = HeaderRewriter::new(rewrite, ogg_writer);
    let mut result = SubmitResult::Good;
    loop {
        match ogg_reader.read_packet() {
            Err(e) => break Err(Error::OggDecode(e).into()),
            Ok(None) => {
                // Make sure to flush any buffered data
                break output.flush().map(|_| result).map_err(|e| Error::WriteError(e).into());
            }
            Ok(Some(packet)) => {
                let submit_result = rewriter.submit(packet);
                match submit_result {
                    Ok(SubmitResult::Good) => {
                        // We can continue submitting packets
                    }
                    Ok(r @ SubmitResult::HeadersChanged { .. }) => {
                        // We can continue submitting packets, but want to save the changed
                        // gains to return as a result
                        result = r;
                    }
                    Ok(r @ SubmitResult::HeadersUnchanged(_)) => {
                        if abort_on_unchanged {
                            break Ok(r);
                        } else {
                            result = r;
                        }
                    }
                    Err(_) => break submit_result,
                }
            }
        }
    }
}
