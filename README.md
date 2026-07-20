# exhash: Verified Line-Addressed File Editor

exhash combines Can Bölük's very clever [line number + hash editing system](https://blog.can.ac/2026/02/12/the-harness-problem/) with the powerful and expressive syntax of the classic [ex editor](https://en.wikipedia.org/wiki/Ex_(text_editor)).

Install via pip to get a convenient Python API, an IPython cell magic, and the `exhash`/`lnhashview` CLI commands:

```bash
pip install exhash
```

## lnhash format

We refer to an *lnhash* as a tag of the form `lineno|hash|`, where `hash` is the lower 16 bits of CRC-32 (IEEE) over the line's UTF-8 content, i.e. Python's `zlib.crc32(line) & 0xffff` formatted as 4 hex chars.

Address forms:

- `lineno|hash|`: hash-verified address
- `$`: last line (no hash)
- `%`: whole file (`1,$`, no hashes)

## CLI

The `exhash` and `lnhashview` commands are Python console scripts over the native Rust extension, installed into your PATH by pip.

### View

```bash
# Shows every line prefixed with its lnhash
lnhashview path/to/file.txt
# Optional line number range to show
lnhashview path/to/file.txt 10 20
```

If `end` is past EOF, `lnhashview` returns through the last available line instead of failing.

### Edit

```bash
# Substitute on one line
exhash file.txt '12|abcd|s/foo/bar/g'

# Transliterate characters on one line
exhash file.txt '12|abcd|y/abc/ABC/'

# Change one line with inline text (spaces after c are literal text)
exhash file.txt '12|abcd|c    replacement line'

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
- For example, `s///` accepts newlines in pattern/replacement; replacement newlines split one line into multiple lines.
- Transliteration uses `y/src/dst/` and requires source/destination to have equal character counts
- A substitute whose pattern matches nothing in its addressed range fails (nothing is written), so a typo cannot silently no-op; substitutes running inside `g`/`g!`/`v` payloads stay lenient, since not every selected line need match

When passing multiple commands, each command's lnhashes are verified immediately before that command runs.

For CLI multiline `a/i/c` commands, omit inline text and provide the text block on stdin:

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
from exhash import exhash, file_exhash, lnhash, lnhashview, lnhashview_file, line_hash
```

### Viewing

```py
text = "foo\nbar\n"
view = lnhashview(text)                        # ["1|a1b2|foo", "2|c3d4|bar"]
view = lnhashview_file("f.py", start=1, end=260) # end past EOF is clamped
```

`lnhashview`/`lnhashview_file` return a `list` subclass whose repr shows the rows verbatim, one per line, so a bare call in IPython displays a ready-to-copy view.

### Editing

`exhash(text, cmds, sw=4)` takes the text and a required iterable of tuple command specs (use `[]` for no-op). Raw command strings are rejected by the Python API. `sw` controls how far `<` and `>` shift.

A command is usually `(addr, op)` or `(addr, op, payload)`. `addr` is an lnhash address string from `lnhash(...)`/`lnhashview(...)`; put ranges in that same string, e.g. `f"{a1},{a2}"`. Substitute uses `(addr, "s", pattern, replacement[, flags])`, so patterns and replacements can contain `/` without delimiter escaping.

Text fields can contain newlines. That covers multiline `a`/`i`/`c` payloads and substitute pattern/replacement. Commands such as `d`, `m`, and `sort` do not take text.

```py
addr = lnhash(1, "foo")  # "1|a1b2|"
res = exhash(text, [(addr, "s", "foo", "baz")])
print(res["lines"])    # ["baz", "bar"]
print(res["modified"]) # [1]

# Multiple commands
a1, a2 = lnhash(1, "foo"), lnhash(2, "bar")
res = exhash(text, [(a1, "s", "foo", "FOO"), (a2, "s", "bar", "BAR")])

# Hashes are checked just-in-time per command.
# If earlier commands change/shift a later target line, recompute lnhash first.

# Change one line; leading spaces are part of the replacement
res = exhash(text, [(addr, "c", "    replacement line")])

# Append multiline text in one tuple payload (no dot terminator)
res = exhash(text, [(addr, "a", "new line 1\nnew line 2")])

# Wrong for the Python API: the trailing "." would be inserted literally
# res = exhash(text, [(addr, "a", "new line 1\nnew line 2\n.")])

# Also wrong: do not split the inserted text into separate cmds entries
# res = exhash(text, [(addr, "a"), "new line 1", "new line 2"])

# Change shift width for < and >
res = exhash(text, [(addr, ">", "1")], sw=2)

# Literal / needs no delimiter escaping in tuple substitute fields
res = exhash("a/b\n", [(lnhash(1, "a/b"), "s", "a/b", "c/d")])

# Literal newlines in replacement split one line into multiple lines
res = exhash("foo\n", [(lnhash(1, "foo"), "s", "foo", "bar\nbaz")])
print(res["lines"])  # ["bar", "baz"]

# Literal newlines in pattern can match across lines
a1, a2 = lnhash(1, "foo"), lnhash(2, "bar")
res = exhash("foo\nbar\n", [(f"{a1},{a2}", "s", "foo\nbar", "replaced")])

# Global commands take a pattern plus a nested subcommand tuple (no address)
res = exhash("keep\nTODO x\n", [("%", "g", "TODO", ("d",))])

# Transliterate takes source/dest fields (equal character counts)
res = exhash("abc\n", [(lnhash(1, "abc"), "y", "abc", "ABC")])
```

### File helpers

`lnhashview_file` reads directly from one file path. All file paths, including file-qualified addresses, expand a leading `~` to your home directory. `file_exhash(path, *cmds, sw=4, inplace=True)` uses `path` as the default file context for unqualified addresses. Pass each command as its own tuple argument. Put file-qualified source and `m`/`t` destination addresses in the address/destination tuple fields:

```py
view = lnhashview_file("file.py")

# By default, writes changed files after every command succeeds
# and returns the combined diff string.
diff = file_exhash("file.py", (addr, "s", "foo", "bar"))

# With inplace=False, files are unchanged and a FileSetEditResult is returned.
res = file_exhash("file.py", (addr, "s", "foo", "bar"), inplace=False)
print(res.changed)          # ["file.py"]
print(res["file.py"].lines)
print(res.format_diff())    # includes --- file.py / +++ file.py headers

# Missing files are treated as empty only when the command is valid on empty input.
diff = file_exhash("new.py", ("0|0000|", "a", "print('hi')"))

# File-qualified addresses can edit or transfer lines across files.
diff = file_exhash("src/a.py",
    ("src/a.py:24|8f12|,38|c0de|", "m", "src/b.py:$"),
    (r"src/a.py:5|91aa|", "s", r"from \.b import old", r"from \.b import helper"))
```

A file prefix is separated from the address with `:`. Escape literal colons in filenames as `\:` and literal backslashes as `\\`.

`file_exhash(..., inplace=False)` returns a `FileSetEditResult`:

- `res.files`: dict of path to `FileEditResult`
- `res.changed`: changed paths, in first-touch order
- `res.default_path`: the default path passed to `file_exhash`
- `res[path]`: shorthand for `res.files[path]`
- `res.format_diff(context=1)`: combined diff with `--- path` / `+++ path` headers

### Notebook cells

`lnhashview_cell(path, cell_id, ...)` returns a normal lnhash view for one cell. `lnhashview_cells(path, *cell_ids, ...)` returns the requested cells in order, using `# cell <id>` headers before each cell's normal `lineno|hash|content` lines. `cell_exhash(path, cell_id, *cmds, sw=4, inplace=True)` edits one cell; pass each command as its own tuple argument. Like `file_exhash` it writes and returns a diff by default, and `inplace=False` previews the `EditResult` without touching the file.

### The `%%exhash` cell magic

Importing `exhash.skill` under IPython or Jupyter registers the `%%exhash` cell magic - the standard way to apply `a`/`i`/`c` payload commands interactively. The magic line is `%%exhash <path> [<cell_id>] <address> <a|i|c>`; the payload is everything below it, taken verbatim. Nothing in the payload is parsed as Python, so there is no quoting or escaping at all:

```
%%exhash notes.txt 2|beef|a
new line 1
new line 2
```

- `%%exhash new.py 0|0000| a` creates a missing file.
- `%%exhash f.py % c` replaces the whole file (`%` needs no hashes). With a cell id, `%%exhash nb.ipynb ab12 % c` replaces that notebook cell's source.
- `%%exhash f.py 12|a3f2|,15|b1c3| c` replaces just that range, both addresses from one `lnhashview_file` view.
- One trailing newline (the cell terminator) is stripped; to end the payload with a blank line, leave one extra blank line at the bottom.
- Each magic cell applies one command and returns the diff.

Tuple `a`/`i`/`c` payloads (as in the examples above) remain for scripts and tests, where magics don't exist. Interactively, prefer the magic: a Python string layer invites quoting mistakes.

### Pyskill

The package registers `exhash.skill` as a pyskill exposing the primary Python APIs with LLM-oriented workflow docs. Use `doc(exhash.skill)` after importing it through a pyskills host.

### EditResult

`exhash()` returns an `EditResult` with attributes (also accessible via `res["key"]`):

- `lines`: list of output lines
- `hashes`: lnhash for each output line
- `modified`: 1-based line numbers of modified/added lines
- `deleted`: 1-based line numbers of removed lines (in original)
- `origins`: for each output line, the 1-based original line number (None if inserted)

`res.format_diff(context=1)` returns a unified-diff-style summary showing only changed lines with context:

```py
res = exhash(text, [(addr, "s", "foo", "baz")])
print(res.format_diff())
# --- original
# +++ modified
# -1|a1b2|foo
# +1|c3d4|baz
#  2|e5f6|bar
```

All diff strings returned by `format_diff`, `file_exhash`, and `cell_exhash` are fastcore `PrettyString`s, and the result objects' reprs show the diff too - so in IPython, ending a cell with the bare call displays the diff verbatim, no `print` needed.

## Tests

```bash
pytest -q
```
