# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.2] - 2022-09-08
### Changed
* Change memory management strategy
* Extend the API so other storages can be built on top of the storage
* Create a different fork based on Eugene Babichenko's work
* Rename the library to fast-reindexable-append-only-storage (fraos)

## [0.6.1] - 2021-03-09
### Changed
* Migrate to `memmap2`. This is a maintained fork of `memmap`.

## [0.6.0] - 2020-09-09
### Added
* In-memory databases.

## [0.5.0] - 2020-08-27
### Added
* `SharedMmap` is now `Sync`.
### Removed
* The possibility to mutate `SharedMmap`
### Fixed
* Instability of parallel reads and writes on macOS.

## [0.4.0] - 2020-08-19
### Changed
- Changed the internal data format to remove lengths from the flatfile.
- Zero-length entries are not legal now and will cause panics.
### Removed
- Snapshot capability due to breaking changes in the data format. Now users
  should just copy the whole directory.

## [0.3.1] - 2020-08-11
### Added
- Implement `Debug` for `SharedMmap`.

## [0.3.0] - 2020-08-11
### Changed
- Public methods now use `SharedMmap` instead of `&[u8]`.
- `SeqNoIter` now also uses `SharedMmap` which allows it to use the `Iterator`
  trait.

## [0.2.0] - 2020-08-04
### Removed
- Indexing by key - now records can only be indexed by their sequential number.
  This also allows to remove serializers and `Record` type.

## [0.1.1] - 2020-07-22
### Fixed
- Non-existent database location is actually created

## [0.1.0] - 2020-07-22
### Added
- Basic cross-platform flat storage.
- Persistent indexing by record number.
- In-memory B-tree for indexing by keys.
- Possibility to have different record serialization approaches.

[Unreleased]: https://github.com/eugene-babichenko/data-pile/compare/v0.6.1...HEAD
[0.6.1]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.6.1
[0.6.0]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.6.0
[0.5.0]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.5.0
[0.4.0]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.4.0
[0.3.1]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.3.1
[0.3.0]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.3.0
[0.2.0]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.2.0
[0.1.1]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.1.1
[0.1.0]: https://github.com/eugene-babichenko/data-pile/releases/tag/v0.1.0
