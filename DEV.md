# Development

## Prerequisites

- Rust toolchain (stable)
- Python 3.8+
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
  __init__.py     Python wrapper (EditResult class, exhash function)
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

## How the binary distribution works

Maturin's `data` option in `pyproject.toml` points to `python/exhash.data/`. Files in the `scripts/` subdirectory are installed as standalone executables when the wheel is installed via pip. The build script compiles the Rust `[[bin]]` targets and copies them there before building the wheel.

## Command parsing modes

The Rust core has three parsing functions:

- `parse_commands_from_strs(&[&str])` — for the Python API; each string is one command, text blocks are the remaining lines (no `.` terminator)
- `parse_commands_from_script(&str)` — for script strings; commands separated by newlines, text blocks terminated by `.`
- `parse_commands_from_args(&[String], &mut BufRead)` — for the CLI; each arg is a command, text blocks read from stdin terminated by `.`
