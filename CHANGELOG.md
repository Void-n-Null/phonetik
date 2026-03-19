# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-03-19

### Added

- `StressMode` enum (`Spoken` / `Dictionary`) controlling function word stress.
- `scan_with_mode()` method for explicit stress mode selection.
- Monosyllabic function words (I, the, to, shall, etc.) demoted to unstressed in default `Spoken` mode, fixing iambic pentameter detection for Shakespeare and other formal verse.

### Changed

- `scan()` now defaults to `StressMode::Spoken`. Previous behavior available via `scan_with_mode(line, StressMode::Dictionary)`.

## [0.2.0] - 2026-03-19

### Added

- `Phonetik` implements `Clone`. Cloning is cheap (reference-counted internals, no allocation).
- `server` module with `server::router()` for in-process testing of HTTP handlers.
- `distance` module consolidating the Levenshtein implementation.
- 227 tests: 196 unit tests, 27 integration tests, 4 doc tests.

### Changed

- Levenshtein uses single-row algorithm with stack-allocated buffer. No heap allocation for typical phoneme inputs.
- Server handlers extracted from `main.rs` into `src/server.rs`.
- Startup no longer constructs a second `Phonetik` instance for the log message.

### Removed

- `load_dotenv` function and its `unsafe { set_var }` call. `PORT` env var still works via the standard environment.

## [0.1.0] - 2026-03-18

Initial release.

[0.3.0]: https://github.com/Void-n-Null/phonetik/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Void-n-Null/phonetik/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Void-n-Null/phonetik/releases/tag/v0.1.0
