# Zoog: Zero Opus Output Gain

Zoog is a tool for setting the output gain value located in a binary header
inside Opus files (specifically an Opus-encoded audio stream within an Ogg
file). It is intended to solve the "Opus plays too quietly problem".

## Background

Opus-encoded audio files contain an [‘output
gain’](https://tools.ietf.org/html/rfc7845) value which describes a gain to be
applied when decoding the audio. This value appears to exist in order to ensure
that loudness changes to Opus files are *always* applied, rather than being
dependent on decoder support for tags such as `REPLAYGAIN_TRACK_GAIN` and
`REPLAYGAIN_ALBUM_GAIN` (used in Ogg Vorbis, but *not* Opus).

The in-header value was intended to correspond to the album gain with
[RFC7845](https://tools.ietf.org/html/rfc7845) defining the tag
`R128_TRACK_GAIN` for single-track normalization. It seems the original intent
of the output gain was to eliminate the need for an album gain tag, however
`R128_ALBUM_GAIN` was later added for album normalization.

## The problem

When encoding an Opus stream using `opusenc` from a FLAC stream which has
embedded ReplaygGain tags, the resulting Opus stream will have the output-gain
field set in the Opus header. The gain value will be chosen using
[EBU R 128](https://en.wikipedia.org/wiki/EBU_R_128) with a loudness value
of -23 [LUFS](https://en.wikipedia.org/wiki/LKFS) (ReplayGain uses -18 LUFS).

The presence of either `R128_TRACK_GAIN` or `R128_ALBUM_GAIN` will allow
players that support these to play tracks at the correct volume.  However, in
audio players that do not support these tags, track will likely sound extremely
quiet.

## What Zoog does

Zoog adjusts both the Opus binary header and the `R128_TRACK_GAIN` and
`R128_ALBUM_GAIN` tags such that files will continue to play at the correct
volume in players that support these tags, and at a more appropriate volume in
players that don't.

Zoog doesn't have the ability to decode and do loudness analysis of Opus files,
so it depends on the presence of `R128_ALBUM_GAIN` and/or `R128_TRACK_GAIN`
tags. If you do not have these tags, a tool such as
[loudgain](https://github.com/Moonbase59/loudgain) can be used to generate them.

The following options are available:

* `--preset=none`: In this mode, Zoog will set the output gain in the
  Opus binary header to 0dB. In players that do not support `R128` tags, this
  will cause the Opus file to play back at the volume of the originally encoded
  source. You may want this if you use a player that doesn't support any
  sort of volume normalization.

* `--preset=rg`: In this mode, Zoog will set the output gain in the Opus binary
  header to the value that ensures playback will occur at -18 LUFS, which
  should match the loudness of ReplayGain normalized files.  This is probably
  the best option when you have a player that doesn't know about Opus `R128`
  tags, but:
    * does support ReplayGain for the other file formats you use, and/or
    * the files you play have been adjusted in a player-agnostic way
      ([mp3gain](http://mp3gain.sourceforge.net/) and
      [aacgain](http://aacgain.altosdesign.com/) can do this) to the ReplayGain
      reference volume.

* `--preset=r128`: In this mode, Zoog will set the output gain in the Opus
  binary header to the value that ensures playback will occur at -23 LUFS,
  which should match the loudness of files produced by `opusenc` from FLAC
  files which contained ReplayGain information. You're unlikely to want this
  option as the main use of Zoog is modify files which were generated this way.
  This will use the album normalization value if present, and the track
  normalization value if not.


If neither the `R128_ALBUM_GAIN` or `R128_TRACK_GAIN` tags are found in the
input file, Zoog will not modify the file.

## What Zoog doesn't do

* Zorg doesn't actually compute the loudness of the input file itself, hence the requirements
for either the `R128_ALBUM_GAIN` or `R128_TRACK_GAIN` tags.

* Due to the first point, Zorg cannot do anything about clipping. If
`--preset=none` is set, the clipping will be the same as would have existed
if `opusenc` had been used on an input file without any loudness information.
On audio with high levels of 
[dynamic range compression](https://en.wikipedia.org/wiki/Dynamic_range_compression),
clipping is unlikely to occur on the other presets.

## Disclaimer

Please see LICENSE. Unless you have a source you can easily reconstruct your Opus files
from, the author recommends making a backup of any files you intend to modify first, and
running `opusinfo` afterwards on any processed files.
