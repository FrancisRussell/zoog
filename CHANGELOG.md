# Changelog

## Unreleased

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
* Ensure input file is closed before renaming.

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
