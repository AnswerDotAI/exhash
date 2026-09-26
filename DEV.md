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
  engine.rs       single- and multi-buffer edit engines producing EditResult, and report formatting
  lnhash.rs       lnhash hashing/formatting/parsing
  parse.rs        compact command parsing (script and args modes)
  commands.rs     shared structured command fields/parser
  files.rs        file/cell paths, notebook JSON, views, edit/write orchestration, per-target reports
  python.rs       PyO3 bindings (incl. exhash_argv used by the CLI)
python/exhash/
  __init__.py     Python validation and result wrappers
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

The existing Python API/CLI regression suite exercises the shared Rust file/cell
implementation. `cargo test` additionally tests the Rust API with no Python feature.
`cargo check --no-default-features` verifies embedding without PyO3.

## Hash verification timing

`edit_text` verifies lnhashes command-by-command immediately before each command executes. A single-line address can match the line's current hash or a recorded call-start hash from an earlier in-place edit. Records are inserted only once per line. A structural edit drops records at and below its topmost affected line; range addresses do not use the fallback.
The `$` (last line) and `%` (whole file) address forms are resolved against the current buffer and do not require hashes.
A bare line number (`Address::Line`) also needs no hash. `build_command` accepts it only for `p`, or for `g`/`g!`/`v` with a `p` subcommand. `m` and `t` destinations reject it. In `files.rs`, a bare number counts as an address only when it ends the field or precedes `,`. This stops a cell ID that starts with a digit, such as `9f8e` in `nb.ipynb:9f8e:12`, from reading as line 9.
`edit_text_with_sw` exposes configurable shift width for `<` and `>`; `edit_text` defaults to `sw=4`.
In CLI and Python file-helper flows, a missing file is treated as empty input only when the parsed command set is valid against an empty buffer (for example `0|AA|a`); otherwise the original file-not-found error is preserved.
Rust `edit_files` resolves optional `path:` and notebook-cell prefixes, loads every
referenced buffer, and calls `edit_buffers_with_sw` once. Rust owns validation,
transfers, diffs, and file/notebook writes. Every command succeeds before writes
begin, but this is not an atomic multi-file transaction: an OS write failure can
leave earlier writes completed. Notebook source form, metadata, outputs, and
trailing newlines are preserved; each notebook is read/written once. JSON uses
arbitrary-precision numbers so unrelated notebook metadata cannot be rounded.
Python `file_exhash` and `cell_exhash` are thin adapters over this core.
The Rust `notebook_text` export supplies the same sorted, one-space-indented JSON serialization to embedding callers.
File views and `file_exhash` normalize CR, CRLF, and LF line endings on read. Unicode separators remain line content, and Python result wrappers use the Rust engine's original lines so no-op detection and diffs agree. No-op file edits leave the original bytes untouched.
`lnhashview` range requests clamp `end` past EOF to the last available line, while invalid `start` values still error.
`EditResult::format_diff_with_maxlen` caps diff rows and chooses where a capped row of a changed pair starts. The Python reprs and the `file_exhash`/`cell_exhash` return values call it with `MAXLEN`. Python `truncate_diff` wraps the Rust line-count cap that `EditResult.__repr__` also uses.
`EditResult::format_diff` never includes the lines addressed by `p`. `EditResult::format_printed` renders them as a bare view with uncapped rows. The Python wrappers and the CLI show the diff, then a `# printed` header, then that view.

## Release

Publishing is handled by GitHub Actions in `.github/workflows/ci.yml` and is triggered by pushing a tag matching `v*`.

Release flow is: release first, then bump.

1. Confirm tests pass:

```bash
pytest -q
```

2. Confirm the release version in `Cargo.toml` (`[package].version`). `pyproject.toml` gets the Python package version from Cargo via `dynamic = ["version"]`.

3. Release:

```bash
ship-release
```

It tags `v<version>`, pushes branch and tag, then bumps `Cargo.toml`, refreshes the editable install, and pushes the bump to `main` (no tag). No need to wait for publish to finish first.

No local build is required for release; CI runs the release build, creates a GitHub Release, and publishes to PyPI.

## How the CLIs work

The commands are Python console scripts declared in `[project.scripts]` (`python/exhash/_cli.py`). `exhash` and `exhash-cell` handle argument parsing and delegate compact parsing,
editing, notebook serialization, and atomic replacement to the extension. `lnhashview` and `lnhashview-cell` provide the corresponding address views. `exhash-open` is a fastcore `call_parse` wrapper over the document outline API.

## Command parsing modes

The Rust core takes commands two ways:

- Structural (PyO3 `exhash` binding): the Python wrapper validates tuple command specs and passes them through as tuples; `commands.rs` builds `Command`/`Subcommand` values directly (`command_from_fields`), with no string round-trip. Address strings are parsed by `parse::command_from_parts`; field validation (substitute flags, transliterate counts) is shared with the compact parser via `subst_from_parts`/`translit_from_parts`. Global commands carry their subcommand as a nested tuple; text fields are verbatim, so there is no delimiter choice or escaping anywhere on this path. A trailing `.` line in an `a/i/c` payload is literal text and the binding warns about this common mistake.
- Compact ex-style strings, where strings are the input medium:
  - `parse_commands_from_script(&str)`: for script strings; commands are separated by newlines. Single-line `a/i/c` text may be inline; if omitted, following lines up to `.` are used as the text block.
  - `parse_commands_from_args(&[String], &mut BufRead)`: used by the `exhash` CLI via the `exhash_argv` binding; each arg is a command. Single-line `a/i/c` text may be inline. One command may instead read a multiline text block from stdin through EOF.

File-qualified addresses and notebook cell prefixes are resolved by `files.rs`.
The public Rust `CommandField` representation supports strings and nested arrays,
so Python and Luau use the same parser without a delimiter/string round trip.
The engine itself still treats target identifiers as opaque strings.

Commands preserve newlines in text fields. This is used by `a/i/c` payloads and by `s` pattern/replacement; replacement newlines split lines during editing. `split_text_payload` splits `a/i/c` payloads with a trailing newline ending the last line, matching stdin text blocks, so the `%%exhash` magic passes its cell body through unchanged. Commands without text fields do not take text. In compact strings, substitute parsing keeps Rust regex escapes intact (`\d`, `\w`, etc.) while allowing escaped command delimiters (`\/`); compact transliteration uses `y/src/dst/`. Tuple fields need no escaping at all.
