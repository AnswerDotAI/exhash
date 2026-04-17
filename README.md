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

We refer to an *lnhash* as a tag of the form `lineno|hash|`, where `hash` is the lower 16 bits of Rust's `DefaultHasher` over the line content.

Address forms:

- `lineno|hash|` — hash-verified address
- `$` — last line (no hash)
- `%` — whole file (`1,$`, no hashes)

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

# Transliterate characters on one line
exhash file.txt '12|abcd|y/abc/ABC/'

# Append multiline text (terminated by a single dot)
exhash file.txt '12|abcd|a' <<'EOF'
new line 1
new line 2
.
EOF

# Dry-run
exhash --dry-run file.txt '12|abcd|d'

# Set shift width for < and >
exhash --sw 2 file.txt '12|abcd|>1'

# Last line and whole file shorthands (no hash)
exhash file.txt '$d'
exhash file.txt '%j'

# Move a line to EOF using $ as the destination
exhash file.txt '12|abcd|m$'

# Create a missing file by treating it as empty input
exhash new.txt '0|0000|a' <<'EOF'
first line
.
EOF
```

Substitute uses Rust regex syntax:

- Pattern syntax is from [`regex`](https://docs.rs/regex/latest/regex/)
- Replacement syntax is from [`regex::Replacer`](https://docs.rs/regex/latest/regex/struct.Regex.html#method.replace), e.g. `$1`, `$0`, `${name}`
- `\/` escapes the command delimiter in pattern/replacement
- Custom delimiters: `s`, `y`, `g`, `g!`, and `v` all accept any non-alphanumeric char as delimiter instead of `/`, e.g. `s@pat@rep@`, `g@pat@cmd`. Each command in a combo picks its own delimiter independently: `g@a/b@s/old/new/`
- Literal newlines in pattern/replacement are supported (joins/splits lines as needed)
- Transliteration uses `y/src/dst/` and requires source/destination to have equal character counts

When passing multiple commands, each command's lnhashes are verified immediately before that command runs.

For `a/i/c` commands, provide the text block on stdin:

```bash
printf "new line 1\nnew line 2\n.\n" | exhash file.txt "2|beef|a"
```

If the file does not exist and the command set is valid on empty input, exhash treats it as an empty file and writes the result. For example, `0|0000|a` can create a new file.

### Stdin filter mode

```bash
cat file.txt | exhash --stdin - '1|abcd|s/foo/bar/'
```

In `--stdin` mode, multiline `a/i/c` text blocks are not available.

## Python API

```py
from exhash import exhash, exhash_file, lnhash, lnhashview, lnhashview_file, line_hash
```

### Viewing

```py
text = "foo\nbar\n"
view = lnhashview(text)       # ["1|a1b2|  foo", "2|c3d4|  bar"]
view = lnhashview_file("f.py") # same but reads from file
```

### Editing

`exhash(text, cmds, sw=4)` takes the text and a required iterable of command strings (use `[]` for no-op). `sw` controls how far `<` and `>` shift. For `a`/`i`/`c` commands, lines after the command are the text block. Do not include an ex-style trailing `.` line here: unlike CLI/script mode, `exhash(text, cmds)` does not use one, and a final `.` line is inserted literally.

```py
addr = lnhash(1, "foo")  # "1|a1b2|"
res = exhash(text, [f"{addr}s/foo/baz/"])
print(res["lines"])    # ["baz", "bar"]
print(res["modified"]) # [1]

# Multiple commands
a1, a2 = lnhash(1, "foo"), lnhash(2, "bar")
res = exhash(text, [f"{a1}s/foo/FOO/", f"{a2}s/bar/BAR/"])

# Hashes are checked just-in-time per command.
# If earlier commands change/shift a later target line, recompute lnhash first.

# Append multiline text (no dot terminator)
res = exhash(text, [f"{addr}a\nnew line 1\nnew line 2"])

# Wrong for the Python API: the trailing "." would be inserted literally
# res = exhash(text, [f"{addr}a\nnew line 1\nnew line 2\n."])

# Change shift width for < and >
res = exhash(text, [f"{addr}>1"], sw=2)

# Custom delimiters (useful when pattern/replacement contains /)
res = exhash(text, [f"{addr}s|foo|bar|"])

# Literal newlines in pattern/replacement (joins/splits lines)
a1, a2 = lnhash(1, "foo"), lnhash(2, "bar")
res = exhash("foo\nbar\n", [f"{a1},{a2}s/foo\nbar/replaced/"])
```

### File helpers

`exhash_file` and `lnhashview_file` read directly from a file path:

```py
view = lnhashview_file("file.py")

# Returns EditResult, file unchanged
res = exhash_file("file.py", [f"{addr}s/foo/bar/"])

# With inplace=True, writes back on success and returns diff string
diff = exhash_file("file.py", [f"{addr}s/foo/bar/"], inplace=True)

# Missing files are treated as empty when the commands are valid on empty input
diff = exhash_file("new.py", ["0|0000|a\nprint('hi')"], inplace=True)
```

### EditResult

`exhash()` returns an `EditResult` with attributes (also accessible via `res["key"]`):

- `lines` — list of output lines
- `hashes` — lnhash for each output line
- `modified` — 1-based line numbers of modified/added lines
- `deleted` — 1-based line numbers of removed lines (in original)
- `origins` — for each output line, the 1-based original line number (None if inserted)

`res.format_diff(context=1)` returns a unified-diff-style summary showing only changed lines with context:

```py
res = exhash(text, [f"{addr}s/foo/baz/"])
print(res.format_diff())
# -1|a1b2|  foo
# +1|c3d4|  baz
#  2|e5f6|  bar
```

## Tests

```bash
cargo test && pytest -q
```
