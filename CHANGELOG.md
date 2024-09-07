# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!--
# Guiding Principles

* Changelogs are for _humans_, not machines.
* There should be an entry for every single version.
* The same types of changes should be grouped.
* Versions and sections should be linkable.
* The latest version comes first.
* The release date of each version is displayed.
* Mention whether you follow Semantic Versioning.

# Types of changes

* `Added` for new features.
* `Changed` for changes in existing functionality.
* `Deprecated` for soon-to-be removed features.
* `Removed` for now removed features.
* `Fixed` for any bug fixes.
* `Security` in case of vulnerabilities.
-->
## [v1.2.0] - 2024-09-08
### Change MSRV from 1.56 to 1.57
Minor refactorings to abuse assertions in constants that Rust 1.57.0 allows.

## [v1.1.0] - 2024-04-30
### Change MSRV from 1.51 to 1.56

windows-sys bump to 1.56 when they switch to 2021 edition.
As a dependent of that crate, we have no choice but to follow.

## [v1.0.0] - 2023-02-26
### First major version
The public API of this crate has been unchanged around 3 years without complains.
It signals that the API is mature enough to be stable for a long time.

## [v0.2.1] - 2023-02-25
### Fixed
* Fix weird build failure when cross-compiling from non-Windows hosts
  657c176a440a64437236ba9d88a2ebd98a8babb1

## [v0.2.0] - 2020-09-05
### Changed
* Some internal refactorings that requires Rust v1.46.0

## [v0.1.5] - 2020-03-18
### Fixed
* Prevent a panic happen when open a reparse point (Commit fd9bbec6061fb100f79795ac9b64db59fbb6a3c0)

## [v0.1.4] - 2020-01-30
### Changed
* Ask for forgiveness in case we have no necessary permission
  instead of always asking for permission.

## [v0.1.3] - 2019-10-28
### Changed
* Obtain appropriate privilege before opening directories.

## [v0.1.0] - 2019-05-15

First release

[v1.2.0]: https://github.com/lzutao/junction/compare/v1.1.0...v1.2.0
[v1.1.0]: https://github.com/lzutao/junction/compare/v1.0.0...v1.1.0
[v1.0.0]: https://github.com/lzutao/junction/compare/v0.2.1...v1.0.0
[v0.2.1]: https://github.com/lzutao/junction/compare/v0.2.0...v0.2.1
[v0.2.0]: https://github.com/lzutao/junction/compare/v0.1.0...v0.2.0
[v0.1.5]: https://github.com/lzutao/junction/compare/v0.1.4...v0.1.5
[v0.1.4]: https://github.com/lzutao/junction/compare/v0.1.3...v0.1.4
[v0.1.3]: https://github.com/lzutao/junction/compare/v0.1.0...v0.1.3
[v0.1.0]: https://github.com/lzutao/junction/releases/tag/v0.1.0
