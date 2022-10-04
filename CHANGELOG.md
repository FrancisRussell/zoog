# Changelog

## Unreleased

* Fix incorrect help text which still called the "original" option "none".
* Remove `commit` function on `CommentHeader`.
* Document API.

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
