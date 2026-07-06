r"""Universal hash-verified text editing for local files. Use this when an LLM needs one safe editing interface for reading, previewing, and modifying text files.

Exhash's purpose is to make edits precise and auditable. First view a file as `lineno|hash|text` (line numbers may be space-padded for alignment); then issue ex-style commands against those exact addresses. Every addressed line's hash is checked immediately before the command runs, so stale context or wrong targets fail instead of editing nearby text. Hashes are checked immediately before each command and lines shift as edits apply; for multiple edits in one call always work *backwards* (bottom-to-top).

Prefer exhash over ad hoc patching for text file modifications, and prefer reading with `lnhashview_file`/`lnhash_cat` over plain file reads whenever an edit may follow: the view doubles as the address book, so the edit needs no second read.

Core APIs:
- `lnhashview_file` (or the lnhash_cat helper) lists hashed lines.
- `exhash` is the in-memory command engine; run `doc(exhash)` for complete command syntax.
- `exhash_file` is the file-aware engine; unqualified addresses use `path` and file-qualified addresses can edit or transfer across files.
- `lnhashview_cell` views one notebook cell's source in an `.ipynb` file; `lnhashview_cells` views several explicit cells with `# cell <id>` headers. `exhash_cell` edits one cell.

Workflow:
1. `lnhash_cat(...)`.
2. Copy exact displayed `lineno|hash|` addresses.
3. Use tuple command specs; use raw triple-quoted Python strings for address, pattern, replacement, and payload text when composing commands.
4. Use `exhash_file(...)` (or `exhash_cell(...)` for one notebook cell) to apply the edit: both write to disk and return a diff by default. Pass `inplace=False` to preview the result object without touching the file.

Addressing:
  Address strings use lnhash addresses: lineno|hash| where hash is a 4-char
  hex content hash. Use lnhashview to get addresses:
    lnhashview file.txt          show all lines with addresses
    lnhashview file.txt 10 20    show lines 10-20
  With multiple commands, hashes are checked immediately before each command runs.

  Single:   12|a3f2|
  Range:    12|a3f2|,15|b1c3|
  Special:  0|0000| targets before line 1 (only with a or i)

Tuple commands:
  (addr, "s", pat, repl[, flags]) Substitute (regex). Flags: g=all, i=case-insensitive. Literal newlines work in pat/repl.
  (addr, "d")       Delete line(s)
  (addr, "a", text) Append payload after line
  (addr, "i", text) Insert payload before line
  (addr, "c", text) Change/replace with payload
  (addr, "j")       Join with next line; with range, joins all lines in range
  (addr, "m", dest) Move line(s) after dest address
  (addr, "t", dest) Copy line(s) after dest address
  (addr, ">", n)    Indent n levels (default 1, 4 spaces each)
  (addr, "<", n)    Dedent n levels (default 1)
  (addr, "sort")    Sort lines alphabetically
  (addr, "p")       Print (include lines in output without changing them)
  (addr, "g", payload), (addr, "g!", payload), (addr, "v", payload) Global commands; payload is compact ex syntax such as /pat/d.

Important:
Do not pass raw commands to Python APIs. Do not create addresses by text search or remembered line numbers, and never construct them by computing hashes (e.g. via `line_hash`): addresses come only from a fresh view immediately before the edit. On stale hash, re-view and rebuild. Tuple text fields can contain newlines wherever the command accepts text. For example, `(addr, "s", "foo", "bar\nbaz")` replaces one line with two. Text fields are taken verbatim: a two-character `\n` sequence stays literal; use an actual newline when you want a line break. For `a`/`i`/`c`, put all text in one tuple payload: `"first\nsecond"` starts with `first`, while `"\nfirst"` inserts a leading blank line before `first`. For moving/copying across files, use file-qualified `m`/`t` address or destination strings; cross-file source ranges are invalid. Missing files can only be created through `(r"0|0000|", "a", text)` or `(r"0|0000|", "i", text)` creation semantics.
"""

from . import exhash, exhash_cell, exhash_file, line_hash, lnhash, lnhashview, lnhashview_cell, lnhashview_cells, lnhashview_file

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "lnhashview_cell", "lnhashview_cells", "exhash", "exhash_file", "exhash_cell", "lnhash_cat"]

def lnhash_cat(fname:str, start:int=None, end:int=None):
    "Little shortcut for printing concatenated lines of lnhashview_file"
    print("\n".join(lnhashview_file(fname, start=start, end=end)))
