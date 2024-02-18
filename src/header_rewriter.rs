use std::collections::VecDeque;
use std::io::{Read, Seek, Write};
use std::marker::PhantomData;

use derivative::Derivative;
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use ogg::{Packet, PacketReader};

use crate::header::{CommentHeader as _, IdHeader as _};
use crate::interrupt::{Interrupt, Never};
use crate::{header, opus, vorbis, Codec, Error};

/// The result of submitting a packet to a `HeaderRewriter`
#[derive(Debug)]
pub enum SubmitResult<S> {
    /// Packet was accepted
    Good,

    /// A rewrite was applied to the stream headers and no changes were made.
    /// A summary of the headers is returned.
    HeadersUnchanged(S),

    /// The stream headers were changed. Summaries of the headers before and
    /// after rewriting are returned.
    HeadersChanged { from: S, to: S },
}

#[derive(Clone, Copy, Debug)]
enum State {
    AwaitingHeader,
    AwaitingComments { serial: u32 },
    Forwarding,
}

/// Enumeration of ID and comment headers for all supported codecs
#[derive(Clone, Debug, PartialEq)]
pub enum CodecHeaders {
    /// Ogg Opus headers
    Opus(opus::IdHeader, opus::CommentHeader),

    /// Ogg Vorbis headers
    Vorbis(vorbis::IdHeader, vorbis::CommentHeader),
}

impl CodecHeaders {
    /// Which codec are the headers for
    #[must_use]
    pub fn codec(&self) -> Codec {
        match self {
            CodecHeaders::Opus(_, _) => Codec::Opus,
            CodecHeaders::Vorbis(_, _) => Codec::Vorbis,
        }
    }

    /// Serializes the identification header into a `Write`
    pub fn serialize_id_header<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        match self {
            CodecHeaders::Opus(i, _) => i.serialize_into(writer),
            CodecHeaders::Vorbis(i, _) => i.serialize_into(writer),
        }
    }

    /// Serializes the comment header into a `Write`
    pub fn serialize_comment_header<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        match self {
            CodecHeaders::Opus(_, c) => c.serialize_into(writer),
            CodecHeaders::Vorbis(_, c) => c.serialize_into(writer),
        }
    }
}

/// Trait for types used to summarize codec headers
pub trait HeaderSummarize {
    /// Type for summarizing header content which is reported back via
    /// `SubmitResult`
    type Summary;

    /// Type for errors thrown during summarization
    type Error;

    /// Summarizes the content of a header to be reported back via
    /// `SubmitResult`
    fn summarize(&self, headers: &CodecHeaders) -> Result<Self::Summary, Self::Error>;
}

/// Trait for implementing `HeaderSummarize` when headers of different
/// codecs can be treated equivalently.
pub trait HeaderSummarizeGeneric {
    /// Type for summarizing header content which is reported back via
    /// `SubmitResult`
    type Summary;

    /// Type for errors thrown during summarization
    type Error;

    /// Summarizes the content of a header to be reported back via
    /// `SubmitResult`
    fn summarize<I: header::IdHeader, C: header::CommentHeader>(
        &self, id_header: &I, comment_header: &C,
    ) -> Result<Self::Summary, Self::Error>;
}

impl<T> HeaderSummarize for T
where
    T: HeaderSummarizeGeneric,
{
    type Error = T::Error;
    type Summary = T::Summary;

    fn summarize(&self, headers: &CodecHeaders) -> Result<Self::Summary, Self::Error> {
        match headers {
            CodecHeaders::Opus(id, comment) => HeaderSummarizeGeneric::summarize(self, id, comment),
            CodecHeaders::Vorbis(id, comment) => HeaderSummarizeGeneric::summarize(self, id, comment),
        }
    }
}

/// Trait for codec header rewriting
pub trait HeaderRewrite {
    /// Type for errors thrown during header update
    type Error;

    /// Rewrites the Opus and Opus comment headers
    fn rewrite(&self, headers: &mut CodecHeaders) -> Result<(), Self::Error>;
}

/// Trait for implementing `HeaderRewrite` when different codecs can be treated
/// equivalently
pub trait HeaderRewriteGeneric {
    /// Type for errors thrown during header update
    type Error;

    /// Rewrites ID and comment headers
    fn rewrite<I: header::IdHeader, C: header::CommentHeader>(
        &self, id_header: &mut I, comment_header: &mut C,
    ) -> Result<(), Self::Error>;
}

impl<T> HeaderRewrite for T
where
    T: HeaderRewriteGeneric,
{
    type Error = T::Error;

    fn rewrite(&self, headers: &mut CodecHeaders) -> Result<(), Self::Error> {
        match headers {
            CodecHeaders::Opus(id, comment) => HeaderRewriteGeneric::rewrite(self, id, comment),
            CodecHeaders::Vorbis(id, comment) => HeaderRewriteGeneric::rewrite(self, id, comment),
        }
    }
}

/// Re-writes an Ogg Opus stream with modified headers
#[derive(Derivative)]
#[derivative(Debug)]
pub struct HeaderRewriter<'a, HR: HeaderRewrite, HS: HeaderSummarize, W: Write, E> {
    #[derivative(Debug = "ignore")]
    packet_writer: PacketWriter<'a, W>,
    #[derivative(Debug = "ignore")]
    header_packet: Option<Packet>,
    state: State,
    #[derivative(Debug = "ignore")]
    packet_queue: VecDeque<Packet>,
    header_rewrite: HR,
    header_summarize: HS,
    _error: PhantomData<E>,
}

impl<HR, HS, W, E> HeaderRewriter<'_, HR, HS, W, E>
where
    HR: HeaderRewrite<Error = E>,
    HS: HeaderSummarize<Error = E>,
    W: Write,
{
    /// Constructs a new rewriter
    /// - `config` - the configuration for volume rewriting.
    /// - `packet_writer` - the Ogg stream writer that the rewritten packets
    ///   will be sent to.
    pub fn new(rewrite: HR, summarize: HS, packet_writer: PacketWriter<W>) -> HeaderRewriter<HR, HS, W, E> {
        HeaderRewriter {
            packet_writer,
            header_packet: None,
            state: State::AwaitingHeader,
            packet_queue: VecDeque::new(),
            header_rewrite: rewrite,
            header_summarize: summarize,
            _error: PhantomData,
        }
    }

    fn parse_codec_headers(identification: &[u8], comment: &[u8]) -> Result<CodecHeaders, Error> {
        if let Some(opus_header) = opus::IdHeader::try_parse(identification)? {
            let comment_header = opus::CommentHeader::try_parse(comment)?;
            return Ok(CodecHeaders::Opus(opus_header, comment_header));
        }
        if let Some(vorbis_header) = vorbis::IdHeader::try_parse(identification)? {
            let comment_header = vorbis::CommentHeader::try_parse(comment)?;
            return Ok(CodecHeaders::Vorbis(vorbis_header, comment_header));
        }
        Err(Error::UnknownCodec)
    }

    /// Submits a new packet to the rewriter. If `Ready` is returned, another
    /// packet from the same stream should continue to be submitted. If
    /// `HeadersUnchanged` is returned, the supplied stream did not need
    /// any alterations. In this case, the partial output should be discarded
    /// and no further packets submitted.
    #[allow(clippy::missing_panics_doc)]
    pub fn submit(&mut self, mut packet: Packet) -> Result<SubmitResult<HS::Summary>, E>
    where
        HR::Error: From<Error>,
    {
        let packet_serial = packet.stream_serial();
        match self.state {
            State::AwaitingHeader => {
                self.header_packet = Some(packet);
                self.state = State::AwaitingComments { serial: packet_serial };
            }
            State::AwaitingComments { serial } if serial == packet_serial => {
                // Parse Opus header
                let mut id_header_packet = self.header_packet.take().expect("Missing header packet");
                let (summary_before, summary_after, changed) = {
                    // Parse headers
                    let original_headers = Self::parse_codec_headers(&id_header_packet.data, &packet.data)?;
                    let mut headers = original_headers.clone();
                    let summary_before = self.header_summarize.summarize(&headers)?;
                    self.header_rewrite.rewrite(&mut headers)?;
                    let summary_after = self.header_summarize.summarize(&headers)?;

                    // We compare headers rather than the values of the `OpusGains` structs because
                    // using the latter glosses over issues such as duplicate or invalid gain tags
                    // which we will fix if present.
                    let changed = headers != original_headers;
                    // Update ID header
                    id_header_packet.data.clear();
                    headers.serialize_id_header(&mut id_header_packet.data)?;
                    // Update comment header
                    packet.data.clear();
                    headers.serialize_comment_header(&mut packet.data)?;
                    (summary_before, summary_after, changed)
                };
                self.packet_queue.push_back(id_header_packet);
                self.packet_queue.push_back(packet);
                self.state = State::Forwarding;

                return Ok(if changed {
                    SubmitResult::HeadersChanged { from: summary_before, to: summary_after }
                } else {
                    SubmitResult::HeadersUnchanged(summary_before)
                });
            }
            State::AwaitingComments { .. } | State::Forwarding => {
                self.packet_queue.push_back(packet);
            }
        }

        while let Some(packet) = self.packet_queue.pop_front() {
            self.write_packet(packet)?;
        }
        Ok(SubmitResult::Good)
    }

    fn write_packet(&mut self, packet: Packet) -> Result<(), Error> {
        // This is an attempt to help polymorphization by moving the writer dependent
        // code into a separate function
        let packet_info = Self::packet_write_end_info(&packet);
        let packet_serial = packet.stream_serial();
        let packet_granule = packet.absgp_page();

        self.packet_writer
            .write_packet(packet.data, packet_serial, packet_info, packet_granule)
            .map_err(Error::WriteError)
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

/// Convenience function for performing a rewrite.
///
/// Rewrites the headers of an Ogg Opus stream using the supplied
/// `HeaderRewrite`. If `abort_on_unchanged` is set, the function will terminate
/// immediately if it is detected that no headers were modified, otherwise it
/// will continue to rewrite the stream until the input stream is exhausted, an
/// error occurs or the interrupt condition is set.
pub fn rewrite_stream_with_interrupt<HR, HS, R, W, I, E>(
    rewrite: HR, summarize: HS, input: R, mut output: W, abort_on_unchanged: bool, interrupt: &I,
) -> Result<SubmitResult<HS::Summary>, E>
where
    HR: HeaderRewrite<Error = E>,
    HS: HeaderSummarize<Error = E>,
    R: Read + Seek,
    W: Write,
    I: Interrupt,
    E: From<Error>,
{
    let mut ogg_reader = PacketReader::new(input);
    let ogg_writer = PacketWriter::new(&mut output);
    let mut rewriter = HeaderRewriter::new(rewrite, summarize, ogg_writer);
    let mut result = SubmitResult::Good;
    loop {
        if interrupt.is_set() {
            return Err(Error::Interrupted.into());
        }
        match ogg_reader.read_packet() {
            Err(e) => break Err(Error::OggDecode(e).into()),
            Ok(None) => {
                // Make sure to flush any buffered data
                break output.flush().map(|()| result).map_err(|e| Error::WriteError(e).into());
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
                        }
                        result = r;
                    }
                    Err(_) => break submit_result,
                }
            }
        }
    }
}

/// Identical to `rewrite_stream_with_interrupt` except the rewrite loop cannot
/// be interrupted.
pub fn rewrite_stream<HR, HS, R, W, E>(
    rewrite: HR, summarize: HS, input: R, output: W, abort_on_unchanged: bool,
) -> Result<SubmitResult<HS::Summary>, E>
where
    HR: HeaderRewrite<Error = E>,
    HS: HeaderSummarize<Error = E>,
    R: Read + Seek,
    W: Write,
    E: From<Error>,
{
    rewrite_stream_with_interrupt(rewrite, summarize, input, output, abort_on_unchanged, &Never::default())
}
