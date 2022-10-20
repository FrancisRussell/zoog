# Zoog: Zero Opus Output Gain

Zoog is a Rust library that consists of functionality that can be used
to determine the loudness of an Ogg Opus file and also to rewrite that
file with new internal gain information as well as loudness-related comment
tags. It also has functionality for purely manipulating comment tags.

Zoog currently contains two tools, `opusgain` and `opuscomment`. `opusgain` can
be used to:

* set the output gain value located in the Opus binary header inside Opus files
  so that the file plays at the loudness of the original encoded audio, or of
  that consistent with the
  [ReplayGain](https://en.wikipedia.org/wiki/ReplayGain)  or [EBU R
  128](https://en.wikipedia.org/wiki/EBU_R_128) standards.

* write the Opus comment tags used by some music players to decide
what volume to play an Opus-encoded audio file at.

It is intended to solve the "Opus plays too quietly" problem.

`opuscomment` can be used to list, replace or modify the comment tags of Ogg
Opus files. Its usage is roughly based on that of `vorbiscomment` though many
options have different naming for improved clarity.

Although `zoog` exposes a library, its API is unstable and this package is
released on [crates.io](https://crates.io/) primarily to allow access to the
`opusgain` tool.  The API is documented however, and the reading the source may
prove useful to anyone else wishing to work with Ogg Opus files.

## `opusgain`

`opusgain` adjusts the Opus binary header for playback at a specific volume and
will always generate the `R128_TRACK_GAIN` tag and the `R128_ALBUM_GAIN` tag
(when in album mode) such that files will play at an appropriate volume in
players that support these tags, and at a more appropriate volume in players
that don't. Existing `R128_ALBUM_GAIN` tags will be stripped when not in album
mode.

`opusgain` (unlike its predecessor `zoog`) decodes Opus audio in order to
determine its volume so that it's possible to be certain that all generated
gain values are correct without making assumptions about their existing values.

The following options are available (run `opusgain --help` for usage):

* `-p PRESET, --preset=PRESET`

  It is recommended to specify this value explicitly, as the default
  may change.

  * `original`: Set the output gain in the Opus binary header to 0dB. In
    players that do not support `R128` tags, this will cause the Opus file to
    play back at the volume of the originally encoded source. You may want this
    if you prefer volume normalization to only occur via tags.

  * `rg`: Set the output gain in the Opus binary header to the value that
    ensures playback will occur at -18 LUFS, which should match the loudness of
    ReplayGain normalized files.  This is probably the best option when you
    have a player that doesn't know about Opus `R128` tags, but:
    * does support ReplayGain for the other file formats you use, and/or
    * the files you play have been adjusted in a player-agnostic way
      ([mp3gain](http://mp3gain.sourceforge.net/) and
      [aacgain](http://aacgain.altosdesign.com/) can do this) to the ReplayGain
      reference volume.

  * `r128`: Set the output gain in the Opus binary header to the value that
    ensures playback will occur at -23 LUFS, which should match the loudness of
    files produced by `opusenc` from FLAC files which contained ReplayGain
    information.

  * `no-change`: Do not change the output gain in the Opus binary header.

* `-o MODE, --output-gain-mode=MODE`

  * `auto`: Set the output gain in the Opus binary header such that each track
    is album-normalized in album mode, or track-normalized otherwise. In album
    mode, this results in all tracks having the same output gain value as well
    as the same `R128_ALBUM_GAIN` tag.

  * `track`: Set the output gain in the Opus binary header such that each track
    is track-normalized, even if album mode is enabled. In album mode, this
    results in all tracks being given different output gain values as well as
    different `R128_ALBUM_GAIN` tags, but their `R128_TRACK_GAIN` tags will be
    identical.  Unless you know what you're doing, you probably don't want this
    option.

* `-a, --album`: Enables album mode. This causes `R128_ALBUM_GAIN` tags to also be
  generated. These tell players that support these tags what gain to apply so
  that each track in the album maintains its relative loudness. By default the
  output gain value for each file will be set to identical values in order to
  apply the calculated album gain, but this behaviour can be overridden using
  the `--output-gain-mode` option.

* `-d, --display-only`: Displays the same output that `opusgain` would otherwise
  produce, but does not make any changes to the supplied files.

* `-j N, --num-threads=N`: Use `N` threads for processing. The default is to use the
  number of cores detected on the system. Larger numbers will be rounded down
  to this value. To avoid high disk space usage during processing, or a large
  number of temporary files left around after an error, only one file will be
  rewritten at a time regardless of the number of threads.

* `-c, --clear`: Remove all `R128` tags from the specified files. The output
  gain of each file is unchanged, regardless of the specified preset.

If the internal gain and tag values are already correct for the specified files,
`opusgain` will avoid rewriting them.

`opusgain` supports Unix shell style wildcards under Windows, where wildcards
must be handled by the application rather than expanded by the shell.

## `opuscomment`

`opuscomment` can be used to delete, append, replace and list the comments
located in an Ogg Opus file.

The following options are available (run `opuscomment --help` for usage):

* `-l, --list`: List all tags in the file in `NAME=VALUE` format. This will be to
  standard output unless `-c` is specified.

* `-m, --modify`: Tags specified using `-t` or `-I` will be appended to the
  specified file. Tags matching patterns specified using `-d` will be
  removed from the existing tags on the file.

* `-r, --replace`: All existing tags in the file will be removed and will be
  replaced with those specified using `-t` or `-I`.

* `-t NAME=VALUE, --tag NAME=VALUE`. The specified tag is will be added to the
  file in modify or replace mode.

* `-d NAME[=VALUE], --delete NAME[=VALUE]`. Specifies either a tag name, or a
  name-value mapping to be deleted. All tags that match the pattern will be
  removed, not just the first. This option is only valid in modify mode.

* `-e, --escapes`: In all tag input/output either on the command-line or
  to/from a file escapes will be used for line-feeds (`\n`), carriage returns
  (`\r`), backslashes (`\\`) and the null character (`\0`). All other escapes
  are invalid. This option makes it possible to specify tags which contain
  newlines which would otherwise fail to be parsed correctly from a comment
  file.

* `-I COMMENT_FILE, --tags-in COMMENT_FILE`: In the modify and replace modes,
  the tags to added will be read from this file in addition to those specified
  on the command line. Tags are read in `NAME=VALUE` format, with one tag per
  line. If `-` is specified for the file name, tags will be read from
  standard input.

* `-O COMMENT_FILE, --tags-out COMMENT_FILE`: In list mode, tags will be
  written to this file. Tags are read in `NAME=VALUE` format, with one tag per
  line. If `-` is specified for the file name, tags will be written to standard
  output.

`opuscomment` only has knowledge of UTF-8. Usage on systems where UTF-8 is not
the character encoding scheme in use may encounter issues.

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

## Installation via `cargo`

At the command line, simply run
```
$ cargo install zoog
```

`opusgain` and `opuscomment` should now be available in the path.

## Releases

Zoog binaries for Windows, MacOS and Linux can be found on the [releases
page](https://github.com/FrancisRussell/zoog/releases/). Only the Linux
binaries have undergone any testing at present.

## About Ogg Opus Volume Normalization

### Background

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

### The problem

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
internal gain corresponds to an album normalization at -23 LUFS.

## Q & A

### How is loudness calculated?

Loudness is calculated using [ITU-R
BS.1770](https://en.wikipedia.org/wiki/LKFS). This is the standard used by [EBU
R 128](https://en.wikipedia.org/wiki/EBU_R_128) for measuring loudness and the
one intended for use when calculating Opus `R128` tags.

### What happened to the `zoog` program?

It was deprecated and removed from the repository.

### What did `zoog` do?

`zoog` modified the internal gain values of Opus files and applied the inverse
gain delta to the any `R128` tags present in the file.  Like `opusgain`, this
enabled targeting Opus-encoded tracks to a particular loudness level on players
that did not support `R128` tags whilst maintaining the same loudness value for
players that used them.

### Why was `zoog` deprecated?

`zoog` did not decode audio in order to determine loudness. Instead it relied
upon existing `R128` tags. This was problematic because lack of an
`R128_ALBUM_GAIN` tag does not indicate a track is not album normalized - it
might still have been album normalized via the internal gain header (as done by
`opusenc` when encoding from FLAC files containing ReplayGain tags). Such files
are problematic for players in general if they wish to play tracks at an
album-normalized volume because it's not obvious how to tell if tracks have
been album normalized.

`zoog` had a similar issue. Modifying an album-normalized track's internal gain
requires creation of an `R128_ALBUM_GAIN` tag if there is not one present. If
the track is not album-normalized, then adding such a tag is nonsensical.

`zoog` did not introduce new `R128_ALBUM_GAIN` tags and It was suggested that a
tool like [loudgain](https://github.com/Moonbase59/loudgain) be used create
`R128_ALBUM_GAIN` tags before applying `zoog` to album-normalized files.
However, failure to do this would likely result in different internal gains being
applied to different tracks in an album, losing album-normalization in a way that
would likely go unnoticed.

Due to the potential for error, `zoog` was removed and `opusgain` was created.
Like `vorbisgain` and similar tools, `opusgain` decodes the audio to determine loudness
and has an option to specify whether the tracks being normalized are part of an album.

### When should I use `opusgain` versus `loudgain`

If you only play Opus files in players which support `R128` tags, then use
[loudgain](https://github.com/Moonbase59/loudgain).

You should use `opusgain` if you play Ogg Opus files in players that do not
support `R128` tags and would like them to play at either their original
volume, or at the volumes suggested by ReplayGain or EBU R 128.

Once you have set the internal gains of a set of Opus files to the desired
values, then `loudgain` is likely preferable for any future tag updates related to
normalization.

### How can I check if `opusgain` is working correctly?

Applying `opusgain` to various test files then reviewing the diagnostic output
and `R128` tags generated by [loudgain](https://github.com/Moonbase59/loudgain)
when applied to the rewritten files is helpful in this regard.

## Disclaimer

Please see LICENSE. Unless you have a source you can easily reconstruct your Opus files
from, the author recommends making a backup of any files you intend to modify first, and
running `opusinfo` afterwards on any processed files.
