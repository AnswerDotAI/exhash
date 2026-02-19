# exhash — Verified Line-Addressed File Editor

exhash combines Can Bölük's very clever [line number + hash editing system](https://blog.can.ac/2026/02/12/the-harness-problem/) with the powerful and expressive syntax of the classic [ex editor](https://en.wikipedia.org/wiki/Ex_(text_editor)).

Install via pip to get both a convenient Python API, and native CLI binaries:

```bash
pip install exhash
```

Or install just the CLI binaries via cargo:

```bash
cargo install exhash
```

## lnhash format

We refer to an *lnhash* as a tag of the form `lineno|hash|`, where `hash` is the lower 16 bits of Rust's `DefaultHasher` over the line content. exhash is just like ex, except that addresses *must* be in lnhash format. Addresses like `%`, `.`, etc are not permitted.

## CLI

The native Rust binaries are installed into your PATH via pip.

### View

```bash
# Shows every line prefixed with its lnhash
lnhashview path/to/file.txt
# Optional line number range to show
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
