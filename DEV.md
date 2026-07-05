# Development

## Prerequisites

- Rust toolchain (stable)
- Python 3.10+
- [maturin](https://github.com/PyO3/maturin): `pip install maturin`
- [fastship](https://github.com/AnswerDotAI/fastship): installed by `pip install -e '.[dev]'`

## Project layout

```
src/
  lib.rs          public API, error type, module declarations
  engine.rs       edit engine producing EditResult
  lnhash.rs       lnhash hashing/formatting/parsing
  parse.rs        command parsing (script, strs, and args modes)
  python.rs       PyO3 bindings (incl. exhash_argv used by the CLI)
python/exhash/
  __init__.py     Python wrapper functions plus file-aware exhash_file orchestration
  _cli.py         exhash/lnhashview console-script entry points
  skill.py        pyskills entry point exposing exhash APIs for LLM tools
tests/
  test_exhash.py    Python API tests
  test_commands.py  engine command coverage
  test_cli.py       CLI console-script tests
```

## Building

For local development, build and install the extension:

```bash
maturin develop
```

`ship-rs-build` builds the distributable wheel. The `exhash` and `lnhashview` commands are Python console scripts (`python/exhash/_cli.py`) over the extension; there are no separate Rust binaries.

## Testing

```bash
pytest -q
```

All tests are Python (`tests/`); there are no `cargo test` unit tests.

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
pytest -q
```

2. Confirm the release version in `Cargo.toml` (`[package].version`). `pyproject.toml` gets the Python package version from Cargo via `dynamic = ["version"]`.

3. Tag that commit and push the tag:

```bash
ship-rs-release
```

4. After pushing the release tag, run `ship-rs-bump`, commit the `Cargo.toml` version bump, and push to `main` (no tag). No need to wait for publish to finish first.

No local build is required for release; CI runs the release build, creates a GitHub Release, and publishes to PyPI.

## How the CLIs work

The `exhash` and `lnhashview` commands are Python console scripts declared in `[project.scripts]` (`python/exhash/_cli.py`). They handle argument parsing, file I/O (atomic writes, binary/UTF-8 rejection, `--stdin`/`--dry-run`), and delegate command parsing and editing to the extension: `_cli` calls the `exhash_argv` binding, which runs `parse_commands_from_args` (ex-style `a/i/c` text blocks terminated by `.`) plus `edit_text_with_sw`.

## Command parsing modes

The Rust core has three parsing functions:

- `parse_commands_from_strs(&[&str])` — for the Python API; each string is one command. For `a/i/c`, text starts immediately after the command character and may contain newlines. Use `cfirst\nsecond` when `first` is the first inserted line; `c\nfirst` inserts a leading blank line before `first`. Do not use `.` terminators or split text into separate command entries; a trailing `.` line is literal text and the Python binding warns about this common mistake.
- `parse_commands_from_script(&str)` — for script strings; commands are separated by newlines. Single-line `a/i/c` text may be inline; if omitted, following lines up to `.` are used as the text block.
- `parse_commands_from_args(&[String], &mut BufRead)` — used by the `exhash` CLI via the `exhash_argv` binding; each arg is a command. Single-line `a/i/c` text may be inline; if omitted, text blocks are read from the stdin stream terminated by `.`.

File-qualified addresses are parsed by the Python `exhash_file` wrapper; the Rust parser and CLI remain single-buffer.

Substitute parsing keeps Rust regex escapes intact (`\d`, `\w`, etc.) while still allowing escaped command delimiters (`\/`) in pattern and replacement.
Transliteration uses `y/src/dst/` and validates equal character counts at parse time.
