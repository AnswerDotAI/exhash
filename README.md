# exhash — Verified Line-Addressed File Editor

This repository contains:

- **`crates/exhash-core`**: the Rust library
- **`src/bin/`**: two native CLIs (`exhash`, `lnhashview`)
- **`python/exhash`**: PyO3 bindings exposing the string-based engine

Install via pip to get both the Python API and native CLI binaries:

```bash
pip install exhash
```

Or install just the CLI binaries via cargo:

```bash
cargo install exhash
```

## lnhash format

`lineno|hash|` where `hash` is the lower 16 bits of Rust's `DefaultHasher` over the line content.

## CLI

The native Rust binaries are installed into your PATH via pip.

### View

```bash
lnhashview path/to/file.txt
lnhashview path/to/file.txt 10 20
```

### Edit

```bash
# Substitute on one line
exhash file.txt '12|abcd|s/foo/bar/g'

# Append multiline text (terminated by a single dot)
exhash file.txt '12|abcd|a' <<'EOF'
new line 1
new line 2
.
EOF

# Dry-run
exhash --dry-run file.txt '12|abcd|d'
```

For `a/i/c` commands, provide the text block on stdin:

```bash
printf "new line 1\nnew line 2\n.\n" | exhash file.txt "2|beef|a"
```

### Stdin filter mode

```bash
cat file.txt | exhash --stdin - '1|abcd|s/foo/bar/'
```

In `--stdin` mode, multiline `a/i/c` text blocks are not available.

## Python API

```py
from exhash import exhash, lnhash, lnhashview, line_hash
```

### Viewing

```py
text = "foo\nbar\n"
view = lnhashview(text)  # ["1|a1b2|  foo", "2|c3d4|  bar"]
```

### Editing

`exhash(text, *cmds)` takes the text and one or more command strings. For `a`/`i`/`c` commands, lines after the command are the text block (no `.` terminator needed):

```py
addr = lnhash(1, "foo")  # "1|a1b2|"
res = exhash(text, f"{addr}s/foo/baz/")
print(res.text())      # "baz\nbar"
print(res.modified)    # [1]

# Multiple commands
a1, a2 = lnhash(1, "foo"), lnhash(2, "bar")
res = exhash(text, f"{a1}s/foo/FOO/", f"{a2}s/bar/BAR/")

# Append multiline text (no dot terminator)
res = exhash(text, f"{addr}a\nnew line 1\nnew line 2")
```

### EditResult

- `.lines` — list of output lines
- `.hashes` — lnhash for each output line
- `.modified` — 1-based line numbers of modified/added lines
- `.deleted` — 1-based line numbers of removed lines (in original)
- `.text()` — joined output
- `.view()` — output in lnhash format
- `repr()` — shows only modified lines in lnhash format

## Tests

```bash
cargo test && pytest -q
```
