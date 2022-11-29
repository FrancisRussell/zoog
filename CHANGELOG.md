# Changelog

## Unreleased

* Enable compilation with stable Rust.
* Specify minimum Rust version in `Cargo.toml`.

## 0.7.0

* Add `--dry-run` option to `opuscomment`.
* Move `DiscreteCommentList` iterator into submodule.
* Bump Rust edition to 2021.
* Use clippy in pedantic mode for library and executables.
* Some code refactoring to make clippy happier.
* Preserve additional binary data in Opus comment header as suggested by the spec.
* Significant refactoring for multiple codec support.
* Add `opuscomment` support for Ogg Vorbis and rename to `zoogcomment`.
* Handle pre-skip when analyzing volume of Ogg Opus streams.

## 0.6.0

* Add support for interrupting a stream rewrite.
* Allow interrupts during stream rewrite in `opusgain`.
* Add interrupt support to `opuscomment`.

## 0.5.1

* Add Ctrl-C support for stopping `opusgain`.

## 0.5.0

* Add some missing docs.
* Remove an unnecessary type alias (hence the version bump).

## 0.4.0

* Minor code cleanup.
* Document `VolumeAnalyzer::mean_lufs_across_multiple()`.
* Deliberately panic on some exceptional Opus comment cases.
* Treat Opus comment keys case-insensitively and add tests (bugfix).
* Improve Opus comment field name validation.
* Define a trait for comment lists.
* Define a type for holding comment lists.
* Refactor `VolumeRewriter` to be more generic.
* Add `opuscomment` binary for manipulating comments in Ogg Opus files.
* Add library functionality for escaping and unescaping comments.

## 0.3.0

* Fix incorrect help text which still called the "original" option "none".
* Remove `commit` function on `CommentHeader`.
* Document API.
* Remove code that printed to standard output and `verbose` flag from `Rewriter`.
* Rename `RewriteResult` to `SubmitResult`.
* Reduce likelihood of unnecessarily rewriting files due to tag reordering.
* Add tests for `CommentHeader::replace()`.
* Make `opusgain` print existing gains when leaving files unchanged.
* Make `opusgain` print previous and new gains when altering files.
* Make it clearer to Cargo what the licence is.
* Upgrade to `clap` version 4.
* Use `wild` for wildcard support on Windows.
* Enable parallel volume analysis of multiple files.
* Ensure input file is closed before renaming (bugfix on Windows).
* Add option to generate tags without changing output gain.
* Add option to clear `R128` tags from specified files.

## 0.2.0

* `zoog` binary is deprecated and removed from the repository.
* `opusgain` binary is added which can compute the loudness of Opus files
  directly in order to adjust the output gain and generate tags.

## 0.1.4

* Strip debug info from release binaries (requires Rust nightly).

## 0.1.3

* Enable link-time optimization for release builds.

## 0.1.2

* Add 32-bit Windows builds.
* Add unit tests for gain handling and comment header manipulation.

## 0.1.1

* Enable Darwin CI and release builds.

## 0.1.0

* Support for adjusting Opus header and R128 output gain tags.
* Presets defined for ReplayGain, EBU R 128 and initial encoding volume.
