r"""Read, navigate and precisely edit files and notebook cells using hash-verified addresses; transfer text between files/cells, read large files, and navigate Markdown, code, notebook and linked-document hierarchies.

Prefer exhash to ad hoc patching. You MUST open a file with `open_doc` before reading or searching it, and always first display the result bare. `doc(open_doc)` gives the workflow.

## Verified editing

1. Whenever an edit may follow, view with `lnhashview_file`/`lnhashview_cell`/`lnhashview_cells` (`# cell <id>` headers), `rg(..., lnhashs=True)`, or verified outline views. Rows are `lineno|hash|text`: space-padded line numbers, 2-char Base64url hash. Display views bare, never joined, printed, or reformatted.
2. Copy addresses from that view. Never guess line numbers, derive addresses by text search, or compute hashes.
3. Apply tuple commands with `file_exhash(path, *cmds)` or `cell_exhash(path, cell_id, *cmds)`, never compact CLI strings. Unqualified addresses use the call's file/cell. Each command's hashes are checked just before it runs; any failure writes nothing.
4. Read the returned diff as verification. Rows and lines are capped; add `p` for lines needed in full.
5. Re-view before building more commands, including after a stale-hash error.

Commands run in order. A single-line address may match current or call-start content, so commands can stack on one line; structural edits shift later lines, so apply them bottom-to-top. `doc(file_exhash)` covers `inplace=False` previews, missing files and unqualified `m`/`t` destinations. `exhash(text, cmds)` is the in-memory engine; its docstring lists result fields.

## Addresses and payloads

Single `12|Py|`; range `12|Py|,15|HD|`; last `$`; whole file/cell `%` (`1,$`); before line 1 `0|AA|` (only with `a`/`i`); plain `12` or `12,15` only for `p`, including as a `g`/`g!`/`v` subcommand. `$`, `%` and plain line numbers need no hashes. Prefix `path:` or `path.ipynb:cellid:` (exact/unique cell prefix) for another file/cell. A range stays within one file/cell.

In IPython, use `%%exhash` for every interactive `a`/`i`/`c` (registered on import; syntax/examples: `doc(exhash.magic.exhash_magic)`). Its body is unquoted text, one payload line per cell line. Scripts/tests: one tuple field containing a raw triple-quoted string with literal line breaks. Tuple payloads split at each newline: `"x\n"` gives `x` then a blank line; `""` gives one blank line.

## Commands

  (addr, "s", pat, repl[, flags])  fancy-regex; replacement groups $1/$0/${name}, ${1}x before a name char; $$ = literal $; \1 in replacements stays literal. Flags g=all, i=case-insensitive. Fails if nothing matches (except inside g) or on an unknown group. Literal newlines, slashes, and backslashes work. Prefer c for $-heavy text.
  (addr, "a"|"i"|"c", text)        append after, insert before, change
  (addr, "d"|"j"|"sort"|"p")       delete; join (a range joins all its lines); sort; print rows, changing nothing
  (addr, "m"|"t", dest)            move/copy after dest
  (addr, ">"|"<"[, n])             indent/dedent n levels (default 1, 4 spaces each); blank lines stay blank
  (addr, "y", source, dest)        transliterate chars (equal counts)
  (addr, "g"|"g!"|"v", pat, sub)   run sub, an address-free tuple such as ("d",) or ("s", "foo", "bar", "g"), on each addressed line matching pat (g!/v: not matching); globals cannot nest

Patterns in `s` and `g`/`g!`/`v` support lookbehind (`(?<=...)`, `(?<!...)`), lookahead (`(?=...)`, `(?!...)`), and backreferences (`\1`). Matching errors, including exceeding the default backtracking limit, abort the entire call without writing files.

`p`-only calls write nothing and return untruncated verified rows without diff headers/tags: `("%", "p")` all lines; `("12,20", "p")` a range; several `(addr, "p")` scattered lines, across files with qualified addresses; `(addr, "g", pat, ("p",))` matching lines. With edits, the diff precedes `# printed` and the full printed rows as they stand after the whole call.

Transfer existing lines with `m`/`t`, not by retyping them:

  file_exhash(path, ("src/a.py:10|qq|,20|u7|", "m", "src/b.py:$"))
  file_exhash(path, ("nb.ipynb:ab12cd34:6|MO|", "t", "other.ipynb:9f8e:$"))
  file_exhash(path, ("nb.ipynb:ab12cd34:%", "t", "snippets.py:0|AA|"))

Transfers cannot create cells; use notebook/dialog structural APIs for whole-cell operations. A `0|AA|` destination can create a file; otherwise only `0|AA|` with `a`/`i` creates one. Creating a file also creates missing parent directories.

For reflow: `j` a range, re-view, then one g-flagged substitution inserts breaks before chosen tokens: `(addr, "s", r", ('foo'|'bar'|'baz')", ",\n    $1", "g")`.

## Edit journal

`journal(path, author=None, since=None)` reads recorded edits oldest first; path is the journal directory or one author's file.
Filter by author or epoch-seconds `since`. `j.stat()` shows per-file added/removed/net rows; `j.diff()` returns full recorded hunks.
Only exhash file/cell writes are recorded when `EXHASH_JOURNAL` is set; `EXHASH_AUTHOR` identifies the writer.
"""

from . import exhash, cell_exhash, file_exhash, line_hash, lnhash, lnhashview, lnhashview_cell, lnhashview_cells, lnhashview_file, magic
from . import open_doc, Section, Sections, SearchHit, SearchHits, Link, Links, journal

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "lnhashview_cell", "lnhashview_cells", "exhash", "file_exhash", "cell_exhash", "open_doc", "Section", "Sections", "SearchHit", "SearchHits", "Link", "Links", "journal"]

import sys
if 'IPython' in sys.modules:
    from IPython import get_ipython
    if (_ip := get_ipython()): magic.load_ipython_extension(_ip)
