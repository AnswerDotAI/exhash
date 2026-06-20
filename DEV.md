# Development

## Prerequisites

- Rust toolchain (stable)
- Python 3.10+
- [maturin](https://github.com/PyO3/maturin): `pip install maturin`
- [fastship](https://github.com/AnswerDotAI/fastship): `pip install fastship`

## Project layout

```
src/
  lib.rs          public API, error type, module declarations
  engine.rs       edit engine producing EditResult
  lnhash.rs       lnhash hashing/formatting/parsing
  parse.rs        command parsing (script, strs, and args modes)
  python.rs       PyO3 bindings
  bin/exhash.rs   CLI editor (atomic in-place edit, dry-run, stdin mode)
  bin/lnhashview.rs  CLI viewer
python/exhash/
  __init__.py     Python wrapper functions plus file-aware exhash_file orchestration
  skill.py        pyskills entry point exposing exhash APIs for LLM tools
python/exhash.data/scripts/
  exhash          native binary (built, not checked in)
  lnhashview      native binary (built, not checked in)
tests/
  cli.rs          Rust integration tests for CLIs
  test_exhash.py  Python API tests
```

## Building

```bash
ship-rs-prep
```

This builds binaries (debug by default) and copies them to `python/exhash.data/scripts/`. Pass `release` for optimized builds:

```bash
ship-rs-prep --release
```

## Testing

```bash
ship-rs-test
```

## Hash verification timing

`edit_text` verifies lnhashes command-by-command against the current in-memory buffer, immediately before each command executes (not all upfront). If an earlier command shifts or rewrites a later target line, that later command will fail with a stale-hash error unless you recompute addresses.
The `$` (last line) and `%` (whole file) address forms are resolved against the current buffer and do not require hashes.
`edit_text_with_sw` exposes configurable shift width for `<` and `>`; `edit_text` defaults to `sw=4`.
In CLI and Python file-helper flows, a missing file is treated as empty input only when the parsed command set is valid against an empty buffer (for example `0|0000|a`); otherwise the original file-not-found error is preserved.
Python `exhash_file` adds the file-qualified orchestration layer. It parses optional `path:` prefixes, applies each command to the current in-memory buffer for that file, rejects cross-file source ranges, and writes changed files only after every command succeeds.
`lnhashview` range requests clamp `end` past EOF to the last available line, while invalid `start` values still error.

## Release

Publishing is handled by GitHub Actions in `.github/workflows/ci.yml` and is triggered by pushing a tag matching `v*`.

Release flow is: release first, then bump.

1. Confirm tests pass:

```bash
ship-rs-test
```

2. Confirm the release version in `Cargo.toml` (`[package].version`). `pyproject.toml` gets the Python package version from Cargo via `dynamic = ["version"]`.

3. Tag that commit and push the tag:

```bash
ship-rs-release
```

4. After pushing the release tag, run `ship-rs-bump`, commit the `Cargo.toml` version bump, and push to `main` (no tag). No need to wait for publish to finish first.

No local build is required for release; CI runs the release build, creates a GitHub Release, and publishes to PyPI.

## How the binary distribution works

Maturin's `data` option in `pyproject.toml` points to `python/exhash.data/`. Files in the `scripts/` subdirectory are installed as standalone executables when the wheel is installed via pip. `ship-rs-prep` compiles the Rust `[[bin]]` targets configured in `[tool.fastship.rs]` and copies them there before building the wheel.

## Command parsing modes

The Rust core has three parsing functions:

- `parse_commands_from_strs(&[&str])` — for the Python API; each string is one command. Single-line `a/i/c` text may follow the command character directly, e.g. `12|abcd|c    value`. Multiline `a/i/c` text blocks must be in that same string using newlines. Text after the command character is the first inserted line, so `cfirst\nsecond` and `c\nfirst\nsecond` are both valid. Do not use `.` terminators or split the inserted text into separate command entries; a trailing `.` line is literal text and the Python binding warns about this common mistake.
- `parse_commands_from_script(&str)` — for script strings; commands are separated by newlines. Single-line `a/i/c` text may be inline; if omitted, following lines up to `.` are used as the text block.
- `parse_commands_from_args(&[String], &mut BufRead)` — for the CLI; each arg is a command. Single-line `a/i/c` text may be inline; if omitted, text blocks are read from stdin terminated by `.`.

File-qualified addresses are parsed by the Python `exhash_file` wrapper; the Rust parser and CLI remain single-buffer.

Substitute parsing keeps Rust regex escapes intact (`\d`, `\w`, etc.) while still allowing escaped command delimiters (`\/`) in pattern and replacement.
Transliteration uses `y/src/dst/` and validates equal character counts at parse time.
