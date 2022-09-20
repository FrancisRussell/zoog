# Zoog: Zero Opus Output Gain

Zoog is a Rust library that consists of functionality that can be used
to determine the loudness of an Ogg Opus file and also to rewrite that
file with new internal gain information as well as loudness-related comment
tags.

Zoog currently contains a single tool, `opusgain` which can be used to:

* set the output gain value located in the Opus binary header inside Opus files
  so that the file plays at the loudness of the original encoded audio, or of
  that consistent with the
  [ReplayGain](https://en.wikipedia.org/wiki/ReplayGain)  or [EBU R
  128](https://en.wikipedia.org/wiki/EBU_R_128) standards.

* write the Opus comment tags used by some music players to decide
what volume to play an Opus-encoded audio file at.

It is intended to solve the "Opus plays too quietly" problem.

## Background

Opus-encoded audio files contain an [‘output
gain’](https://tools.ietf.org/html/rfc7845) value which describes a gain to be
applied when decoding the audio. This value appears to exist in order to ensure
that loudness changes to Opus files are *always* applied, rather than being
dependent on decoder support for tags such as `REPLAYGAIN_TRACK_GAIN` and
`REPLAYGAIN_ALBUM_GAIN` which are used in Ogg Vorbis, but *not* Opus.

The in-header value was intended to correspond to the album gain with
[RFC 7845](https://tools.ietf.org/html/rfc7845) defining the tag
`R128_TRACK_GAIN` for single-track normalization. It seems the original intent
of the output gain was to eliminate the need for an album gain tag, however
`R128_ALBUM_GAIN` was later added for album normalization.

## The problem

When encoding an Opus stream using `opusenc` from a FLAC stream which has
embedded ReplayGain tags, the resulting Opus stream will have the output-gain
field set in the Opus header. The gain value will be chosen using [EBU R
128](https://en.wikipedia.org/wiki/EBU_R_128) with a loudness value of -23
[LUFS](https://en.wikipedia.org/wiki/LKFS), which is 5 dB quieter than
ReplayGain.

The presence of either `R128_TRACK_GAIN` or `R128_ALBUM_GAIN` tags will allow
players that support these to play tracks at an appropriate volume. However, in
audio players that do not support these tags, track will likely sound extremely
quiet (unless your entire music collection is normalized to -23 LUFS).

Even more problematically, using `opusenc` with a FLAC file that does not have
embedded ReplayGain tags will produce a file that plays at the original volume
of the source audio. This difference in behaviour means that it's not possible
for players that do not support `R128` tags to assume that different Opus files will
play at a similar volume, despite the presence of the internal gain header.

Even if a player does support the `R128` tags, this is not enough to correctly
play Opus files at the right volume. In the case described above, `opusenc`
will use the internal gain to apply album normalization, meaning that it does
not generate a `R128_ALBUM_GAIN` tag. Without this, it's not possible for a
music player to play a track at album volume without again assuming that the
internal gain corresponds to a album normalization at -23 LUFS.

## What `opusgain` does

`opusgain` adjusts the Opus binary header for playback at a specific volume and
will generate the `R128_TRACK_GAIN` and `R128_ALBUM_GAIN` tags (the latter only
if in album mode) such that files will play at an appropriate volume in players
  that support these tags, and at a more appropriate volume in players that
  don't.

`opusgain` (unlike its predecessor `zoog`) decodes Opus audio in order to
determine its volume so that it's possible to be certain that all generated
gain values are correct without making assumptions about their existing values.

The following options are available:

* `--preset=none`: In this mode, `opusgain` will set the output gain in the
  Opus binary header to 0dB. In players that do not support `R128` tags, this
  will cause the Opus file to play back at the volume of the originally encoded
  source. You may want this if you prefer volume normalization to only occur via
  tags.

* `--preset=rg`: In this mode, `opusgain` will set the output gain in the Opus binary
  header to the value that ensures playback will occur at -18 LUFS, which
  should match the loudness of ReplayGain normalized files.  This is probably
  the best option when you have a player that doesn't know about Opus `R128`
  tags, but:
    * does support ReplayGain for the other file formats you use, and/or
    * the files you play have been adjusted in a player-agnostic way
      ([mp3gain](http://mp3gain.sourceforge.net/) and
      [aacgain](http://aacgain.altosdesign.com/) can do this) to the ReplayGain
      reference volume.

* `--preset=r128`: In this mode, `opusgain` will set the output gain in the Opus
  binary header to the value that ensures playback will occur at -23 LUFS,
  which should match the loudness of files produced by `opusenc` from FLAC
  files which contained ReplayGain information. You're unlikely to want this
  option as the main use of `opusgain` is modify files which were generated this way.

* `-a`: Enables album mode. In this case, the internal gain will be set to an
  identical value for all specified files with the files being considered as if
  they were a single audio file. `R128_ALBUM_GAIN` tags will also be generated.

If the internal gain and tag values are already correct for the specified files,
`opusgain` will avoid rewriting them.

## Build Instructions 

If you do not have Cargo, install it by following the instructions
[here](https://doc.rust-lang.org/cargo/getting-started/installation.html).

Clone the Git repository:

```$ git clone https://github.com/FrancisRussell/zoog.git```

Inside the cloned repository:

```cargo build```

or 

```cargo build --release```

for a release build.

Built binaries can be found in `target/debug` or `target/release`.

## Releases

Zoog binaries for Windows, MacOS and Linux can be found on the [releases
page](https://github.com/FrancisRussell/zoog/releases/). Only the Linux
binaries have undergone any testing at present.

## Disclaimer

Please see LICENSE. Unless you have a source you can easily reconstruct your Opus files
from, the author recommends making a backup of any files you intend to modify first, and
running `opusinfo` afterwards on any processed files.
