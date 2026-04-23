# Development

## Prerequisites

- Rust toolchain (stable)
- Python 3.10+
- [maturin](https://github.com/PyO3/maturin): `pip install maturin`

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
  __init__.py     Python wrapper functions with typed/docstring API (+ exhash_result helper)
python/exhash.data/scripts/
  exhash          native binary (built, not checked in)
  lnhashview      native binary (built, not checked in)
tests/
  cli.rs          Rust integration tests for CLIs
  test_exhash.py  Python API tests
```

## Building

```bash
tools/build.sh
```

This builds binaries (debug by default) and copies them to `python/exhash.data/scripts/`. Pass `release` for optimized builds:

```bash
tools/build.sh release
```

## Testing

```bash
cargo test && pytest -q
```

## Hash verification timing

`edit_text` verifies lnhashes command-by-command against the current in-memory buffer, immediately before each command executes (not all upfront). If an earlier command shifts or rewrites a later target line, that later command will fail with a stale-hash error unless you recompute addresses.
The `$` (last line) and `%` (whole file) address forms are resolved against the current buffer and do not require hashes.
`edit_text_with_sw` exposes configurable shift width for `<` and `>`; `edit_text` defaults to `sw=4`.
In CLI/file-helper flows, a missing file is treated as empty input only when the parsed command set is valid against an empty buffer (for example `0|0000|a`); otherwise the original file-not-found error is preserved.
`lnhashview` range requests clamp `end` past EOF to the last available line, while invalid `start` values still error.

## Release

Publishing is handled by GitHub Actions in `.github/workflows/ci.yml` and is triggered by pushing a tag matching `v*`.

Release flow is: release first, then bump.

1. Confirm tests pass:

```bash
cargo test && pytest -q
```

2. Confirm the release version matches in both:
   - `pyproject.toml` (`[project].version`)
   - `Cargo.toml` (`[package].version`)

3. Tag that commit and push the tag:

```bash
git tag v0.1.3
git push origin v0.1.3
```

4. After pushing the release tag, bump both files to the next dev version (for example `0.1.4`) and commit/push to `main` (no tag). No need to wait for publish to finish first.

No local build is required for release; CI runs the release build, creates a GitHub Release, and publishes to PyPI.

## How the binary distribution works

Maturin's `data` option in `pyproject.toml` points to `python/exhash.data/`. Files in the `scripts/` subdirectory are installed as standalone executables when the wheel is installed via pip. The build script compiles the Rust `[[bin]]` targets and copies them there before building the wheel.

## Command parsing modes

The Rust core has three parsing functions:

- `parse_commands_from_strs(&[&str])` — for the Python API; each string is one command, text blocks are the remaining lines (no `.` terminator; a trailing `.` line is literal text and the Python binding warns about this common mistake)
- `parse_commands_from_script(&str)` — for script strings; commands separated by newlines, text blocks terminated by `.`
- `parse_commands_from_args(&[String], &mut BufRead)` — for the CLI; each arg is a command, text blocks read from stdin terminated by `.`

Substitute parsing keeps Rust regex escapes intact (`\d`, `\w`, etc.) while still allowing escaped command delimiters (`\/`) in pattern and replacement.
Transliteration uses `y/src/dst/` and validates equal character counts at parse time.
