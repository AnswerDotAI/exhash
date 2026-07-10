r"""Universal hash-verified text editing for local files. Use this when an LLM needs one safe editing interface for reading, previewing, and modifying text files.

Exhash's purpose is to make edits precise and auditable. First view a file as `lineno|hash|text` (line numbers may be space-padded for alignment); then issue ex-style commands against those exact addresses. Every addressed line's hash is checked immediately before the command runs, so stale context or wrong targets fail instead of editing nearby text. Hashes are checked immediately before each command and lines shift as edits apply; for multiple edits in one call always work *backwards* (bottom-to-top).

Prefer exhash over ad hoc patching for text file modifications, and prefer reading with `lnhashview_file` over plain file reads whenever an edit may follow: the view doubles as the address book, so the edit needs no second read.

Core APIs:
- `lnhashview_file` lists hashed lines.
- `exhash` is the in-memory command engine; run `doc(exhash)` for complete command syntax.
- `exhash_file` is the file-aware engine; unqualified addresses use `path` and file-qualified addresses can edit or transfer across files.
- `lnhashview_cell` views one notebook cell's source in an `.ipynb` file; `lnhashview_cells` views several explicit cells with `# cell <id>` headers. `exhash_cell` edits one cell.

Workflow:
1. `lnhashview_file(...)`, ending the cell with the bare call: the result displays verbatim, one `lineno|hash|content` line each, so never join, print, or reformat it.
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
  Last:     $ (last line)
  Whole:    % (whole file or cell, same as 1,$; no hashes needed)
  Special:  0|0000| targets before line 1 (only with a or i)

Tuple commands:
  (addr, "s", pat, repl[, flags]) Substitute (Rust regex syntax: backrefs in repl are $1/$0/${name}; a two-char \1 stays literal). Flags: g=all, i=case-insensitive. Literal newlines work in pat/repl.
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

The `%%exhash` cell magic:
In IPython sessions, importing this module registers the `%%exhash` cell magic: `%%exhash <path> [<cell_id>] <address> <a|i|c>` applies one command whose payload is everything below the magic line, taken verbatim (one trailing newline stripped). Passing `<cell_id>` targets that cell in an .ipynb file instead of a plain file (`exhash_cell`); the magic dispatches on token count, so no separate cell magic exists. Because the payload is never parsed as Python, no quoting or escaping applies, so this is the idiomatic way to create a file (`%%exhash path 0|0000| a`) and to make any large insert. Replacing a whole cell or file is one command: `%%exhash <path> [<cell_id>] % c` with the new content as the payload (`%` needs no hashes: a full replace has no neighboring lines to mis-hit). For a region *within* a file or cell, use a range address with `c`: `%%exhash <path> 12|a3f2|,15|b1c3| c` replaces the whole range with the payload once, both addresses straight from the one pre-edit view. Reserve tuple `a`/`i`/`c` payloads for short, quote-free text.
"""

from . import exhash, exhash_cell, exhash_file, line_hash, lnhash, lnhashview, lnhashview_cell, lnhashview_cells, lnhashview_file, magic

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "lnhashview_cell", "lnhashview_cells", "exhash", "exhash_file", "exhash_cell"]

import builtins
_ip = getattr(builtins, 'get_ipython', lambda: None)()
if _ip is not None: magic.load_ipython_extension(_ip)

