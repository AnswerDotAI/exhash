# NB: this file is designed for an LLM reader, not a human. Add it to your LLM's context and ask it to explain.
r"""Read, navigate and precisely edit files and notebook cells using hash-verified addresses; transfer text between files/cells, read large files, and navigate Markdown, code, notebook and linked-document hierarchies.

- prefer exhash over ad hoc patching; no ad hoc edit tokens, no session per file    /exhash guards against conflicts itself
- MUST `open_doc` a file before reading or searching it; workflow: `doc(open_doc)`

## Edit loop

- view before any possible edit: `lnhashview_file`, `lnhashview_cell`, `lnhashview_cells`, `rg(lnhashs=True)` or verified outline views; display bare, never joined, printed or reformatted
- addresses: copy from that view; never guess line numbers, find them by text search, or compute hashes
- non-trivial edit: build `cmds = [...]` first; failed: fix `cmds`, retry    /no retyping
- apply: `file_exhash(path, *cmds)` or `cell_exhash(path, cell_id, *cmds)`; read returned diff; its rows, lines are capped: add `p` for full lines
- re-view before building more commands, after a stale-hash error too; structural edits shift later lines: apply bottom-to-top

## Addresses

- `12|Py|`: one line; `12|Py|,15|HD|`: range, within one file or cell
- `$`: last line; `%`: whole file or cell (`1,$`); neither takes a hash
- `0|AA|`: before line 1, only with `a`, `i`
- `12`, `12,15`: plain line numbers, only with `p`: alone, or as the address of a `g`, `g!` or `v` whose sub is `("p",)`
- prefix `path:` or `path.ipynb:cellid:` (unique cellid prefix fine): another file or cell

## Payloads: text of `a`, `i`, `c`

- interactive IPython: always `%%exhash` (registered on import); body unquoted, one payload line per line; `doc(exhash.magic.exhash_magic)`
- scripts, tests: one tuple field: raw triple-quoted string with literal line breaks
- tuple payload splits at each newline: `"x\n"` → `x` then a blank line; `""` → one blank line

## Commands

- `(addr, "s", pat, repl[, flags])`: substitute; flags `g` all, `i` case-insensitive; repl `$1` `$0` `${name}`, `${1}x` before a name char; `\1` stays literal; newlines, slashes, backslashes literal; `$`-heavy text: prefer `c`
- `(addr, "a"|"i"|"c", text)`: append after, insert before, change
- `(addr, "d"|"j"|"sort"|"p")`: delete; join (a range joins all its lines); sort; print, changing nothing
- `(addr, "m"|"t", dest)`: move, copy after dest
- `(addr, ">"|"<"[, n])`: indent, dedent n levels (default 1, 4 spaces each); blank lines stay blank
- `(addr, "y", source, dest)`: transliterate chars (equal counts)
- `(addr, "g"|"g!"|"v", pat, sub)`: address-free sub (`("d",)`, `("s", "foo", "bar", "g")`) on each addressed line matching pat (`g!`, `v`: not matching); no nesting
- patterns: fancy-regex: lookbehind, lookahead, backreferences `\1`; matching error, backtracking limit included: whole call aborted, nothing written
- read exact lines: `("%", "p")`, `("12,20", "p")`, several `(addr, "p")` across files, `(addr, "g", pat, ("p",))`    /verified rows, never truncated

## Transfer

- existing lines: `m`, `t`, never retype:
```python
file_exhash(path, ("src/a.py:10|qq|,20|u7|", "m", "src/b.py:$"))
file_exhash(path, ("nb.ipynb:ab12cd34:%", "t", "snippets.py:0|AA|"))
```
- reflow: `j` the range; re-view; one `g`-flagged `s` inserts breaks before chosen tokens: `(addr, "s", r", ('foo'|'bar'|'baz')", ",\n    $1", "g")`

## Edit journal

- `journal(path)`: exhash file and cell writes made while `EXHASH_JOURNAL` is set, oldest first; `EXHASH_AUTHOR` names the writer
- `j.stat()`: added, removed, net rows per file; `j.stat(count=f)`: each hunk's added and removed lines counted through `f` (`fastws.integrate.code_count`: Rust code lines); `j.diff()`: full hunks
"""

from . import exhash, cell_exhash, file_exhash, line_hash, lnhash, lnhashview, lnhashview_cell, lnhashview_cells, lnhashview_file, magic
from . import open_doc, Section, Sections, SearchHit, SearchHits, Link, Links, journal

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "lnhashview_cell", "lnhashview_cells", "exhash", "file_exhash", "cell_exhash", "open_doc", "Section", "Sections", "SearchHit", "SearchHits", "Link", "Links", "journal"]

import sys
if 'IPython' in sys.modules:
    from IPython import get_ipython
    if (_ip := get_ipython()): magic.load_ipython_extension(_ip)
