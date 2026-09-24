r"""Read, navigate, and edit files and notebook cells using hash-verified addresses. Use for precise text edits, transfers between files or cells, and hierarchical navigation of Markdown, code, notebooks, and linked documentation.

Prefer exhash to ad hoc patching. Before using `open_doc` outlines, read `doc(open_doc)` (inputs, verified navigation, views, links, llms.txt workflow).

## Edit loop

1. View with `lnhashview_file`/`lnhashview_cell`/`lnhashview_cells` (`# cell <id>` headers) whenever an edit may follow; `rg(..., lnhashs=True)` and verified outline views also give addresses. Rows are `lineno|hash|text`: space-padded line numbers, 2-char Base64url hash. Display views bare, never joined, printed, or reformatted.
2. Copy addresses from that view. Never guess line numbers, derive addresses by text search, or compute hashes.
3. Apply with `file_exhash(path, *cmds)` or `cell_exhash(path, cell_id, *cmds)`: tuples, never compact CLI strings. Unqualified addresses use the call's file/cell. Each command's hashes are checked just before it runs; any failure writes nothing.
4. Read the returned diff: it is the verification. It caps rows and lines; add `p` for lines needed in full.
5. Re-view before building more commands, including after a stale-hash error.

Commands run in order. A single-line address may match current or call-start content, so commands can stack on one line; structural edits shift later lines, so apply them bottom-to-top. `doc(file_exhash)`: `inplace=False` previews, missing files, unqualified `m`/`t` destinations. `exhash(text, cmds)` is the in-memory engine; its docstring lists result fields.

## Addresses

  Single:   12|Py|
  Range:    12|Py|,15|HD|
  Last:     $
  Whole:    % (whole file or cell, same as 1,$)
  Special:  0|AA| (before line 1; only with a or i)
  Print:    12 or 12,15 (plain line numbers; only p, including as a g/g!/v subcommand)

`$`, `%`, and plain line numbers need no hashes. Prefix `path:` or `path.ipynb:cellid:` (exact or unique cell prefix) to target another file or cell.

## Text payloads

In IPython, use the `%%exhash` magic (registered on import; syntax and examples: `doc(exhash.magic.exhash_magic)`) for every interactive `a`/`i`/`c`: the cell body is the unquoted text. Scripts/tests: one tuple field, a raw triple-quoted string with literal line breaks; an initial newline inserts an initial blank line.

## Commands

  (addr, "s", pat, repl[, flags])  Rust regex; groups $1/$0/${name}, ${1}x before a name char; $$ = literal $; \1 stays literal. Flags g=all, i=case-insensitive. Fails if nothing matches (except inside g) or on an unknown group. Literal newlines, slashes, and backslashes work. Prefer c for $-heavy text.
  (addr, "a"|"i"|"c", text)        append after, insert before, change
  (addr, "d"|"j"|"sort"|"p")       delete; join (a range joins all its lines); sort; print rows, changing nothing
  (addr, "m"|"t", dest)            move/copy after dest
  (addr, ">"|"<"[, n])             indent/dedent n levels (default 1, 4 spaces each)
  (addr, "y", source, dest)        transliterate chars (equal counts)
  (addr, "g"|"g!"|"v", pat, sub)   run sub, an address-free tuple such as ("d",) or ("s", "foo", "bar", "g"), on each addressed line matching pat (g!/v: not matching); globals cannot nest

## Viewing with `p`

`p`-only calls write nothing and return untruncated verified rows, without diff headers or tags: `("%", "p")` all lines; `("12,20", "p")` a range; several `(addr, "p")` scattered lines, across files with qualified addresses; `(addr, "g", pat, ("p",))` lines matching `pat`. In a call that also edits, the diff comes first, then `# printed` and the printed rows in full, as they stand after the whole call.

## Transfers

Transfer existing lines with `m`/`t`, not by retyping them:

  file_exhash(path, ("src/a.py:10|qq|,20|u7|", "m", "src/b.py:$"))          # cut a.py lines 10-20, paste at end of b.py
  file_exhash(path, ("nb.ipynb:ab12cd34:6|MO|", "t", "other.ipynb:9f8e:$"))   # copy one cell line into another notebook's cell
  file_exhash(path, ("nb.ipynb:ab12cd34:%", "t", "snippets.py:0|AA|"))        # copy a whole cell's source into a new file

A range stays within one file/cell. Transfers cannot create cells; use the notebook/dialog structural APIs for whole-cell operations. A `0|AA|` destination can create a file; otherwise only `0|AA|` with `a`/`i` creates one.

For reflow: `j` a range, re-view, then one g-flagged substitution inserts breaks before chosen tokens: `(addr, "s", r", ('foo'|'bar'|'baz')", ",\n    $1", "g")`.
"""

from . import exhash, cell_exhash, file_exhash, line_hash, lnhash, lnhashview, lnhashview_cell, lnhashview_cells, lnhashview_file, magic
from . import open_doc, Section, Sections, SearchHit, SearchHits, Link, Links

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "lnhashview_cell", "lnhashview_cells", "exhash", "file_exhash", "cell_exhash", "open_doc", "Section", "Sections", "SearchHit", "SearchHits", "Link", "Links"]

import sys
if 'IPython' in sys.modules:
    from IPython import get_ipython
    if (_ip := get_ipython()): magic.load_ipython_extension(_ip)
