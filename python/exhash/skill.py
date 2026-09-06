r"""Read, navigate, and edit files and notebook cells using hash-verified addresses. Use for precise text edits, transfers between files or cells, and hierarchical navigation of Markdown, code, notebooks, and linked documentation.

Prefer exhash to ad hoc patching, with hashed views when edits may follow. Display views bare, without joining, printing, or reformatting their verified rows.

## Addresses and calls

Copy addresses only from fresh `lnhashview*` output, `rg(..., lnhashs=True)`, or verified outline views. Never guess line numbers, derive addresses by text search, or compute hashes. Rows are `lineno|hash|text`, optionally space-padded; hashes are four hex characters. Re-view after each edit call or stale-hash error before constructing more commands. Within one call, a single-line address may match current or call-start content; structural edits still shift lines, so apply bottom-to-top.

  Single:   12|a3f2|
  Range:    12|a3f2|,15|b1c3|
  Last:     $ (last line)
  Whole:    % (whole file or cell, same as 1,$; no hashes needed)
  Special:  0|0000| targets before line 1 (only with a or i)

Python APIs take tuples, never compact CLI strings: `file_exhash(path, (addr, "d"), (addr2, "s", pat, repl))`, or `cell_exhash(path, cell_id, *cmds)`. Unqualified addresses use the supplied path/cell. These write and return a diff; `inplace=False` previews an `EditResult`. `exhash` is the in-memory engine; read its function docs for strict substitution matching and result fields. `lnhashview_cells` groups multiple cell views under `# cell <id>` headers.

Use raw triple-quoted strings when composing tuple fields. Text is verbatim: actual newlines split lines, while two-character `\n` stays literal. An `a`/`i`/`c` block is one field; an initial newline inserts an initial blank line. In IPython, use the magic below for these blocks instead.

## Tuple command reference
  (addr, "s", pat, repl[, flags]) Substitute using Rust regex. Replacement groups: $1/$0/${name}; unknown groups fail, \1 stays literal, $$ means literal $, ${name}text disambiguates adjacent text. Flags: g=all, i=case-insensitive. Literal newlines, slashes, and backslashes work. Prefer c for $-heavy text to avoid template parsing.
  (addr, "d")       Delete line(s)
  (addr, "a", text) Append text after line
  (addr, "i", text) Insert text before line
  (addr, "c", text) Change/replace with text
  (addr, "j")       Join with next line; with range, joins all lines in range
  (addr, "m", dest) Move line(s) after dest address
  (addr, "t", dest) Copy line(s) after dest address
  (addr, ">", n)    Indent n levels (default 1, 4 spaces each)
  (addr, "<", n)    Dedent n levels (default 1)
  (addr, "sort")    Sort lines alphabetically
  (addr, "p")       View line(s): emit them as verified `lineno|hash|` rows, changing nothing
  (addr, "g", pat, sub), (addr, "g!", pat, sub), (addr, "v", pat, sub) Global commands: apply `sub` to each addressed line matching `pat` (`g!`/`v`: not matching). `sub` is a subcommand tuple without an address, e.g. ("d",) or ("s", "foo", "bar", "g"); globals cannot nest.
  (addr, "y", source, dest) Transliterate `source` chars to `dest` (equal counts required)


`p`-only calls write nothing and return untruncated verified rows without diff headers or tags: `("%", "p")` reads all, several `(addr, "p")` commands read scattered lines, and `(addr, "g", pat, ("p",))` is scoped grep. In mixed edit/view calls, printed lines become diff context even far from edited hunks.

## Transfers

Transfer existing lines with `m`/`t`, not by retyping their content. Qualify addresses with `path:` or `path.ipynb:cellid:` (exact or unique cell prefix):

  file_exhash(path, ("src/a.py:10|aaaa|,20|bbbb|", "m", "src/b.py:$"))          # cut a.py lines 10-20, paste at end of b.py
  file_exhash(path, ("nb.ipynb:ab12cd34:6|830e|", "t", "other.ipynb:9f8e:$"))   # copy one cell line into another notebook's cell
  file_exhash(path, ("nb.ipynb:ab12cd34:%", "t", "snippets.py:0|0000|"))        # copy a whole cell's source into a new file

A range stays within one file/cell. Transfers cannot create cells; use the notebook/dialog structural APIs for whole-cell operations. A `0|0000|` destination can create a file; ordinary creation requires `(r"0|0000|", "a", text)` or `i`.

For reflow: `j` a range, re-view, then one g-flagged substitution inserts breaks before chosen tokens: `(addr, "s", r", ('foo'|'bar'|'baz')", ",\n    $1", "g")`.

## IPython magic

Importing this module registers `%%exhash <path> [<cell_id>] <address> <a|i|c>`. Use it for every interactive `a`/`i`/`c`; tuple text blocks are for scripts/tests without magics. Its body is literal (one trailing newline stripped), with no Python quoting. Token count distinguishes file from cell; the shlex-split line requires quoted or escaped spaces in paths. IPython expands `{expr}`/`$var` only on that line, e.g. `%%exhash {path} {cid} % c`. Use `0|0000| a` to create a file, `% c` to replace all, or a verified range plus `c` to replace a region.

## Document outlines

`open_doc` accepts files (`fname=` or a `Path`, retained for refresh/edit), URLs (`https?://` strings), or held text (other strings). `Section` trees use Markdown headings, tree-sitter definitions (py/js/ts/tsx/rs/zig/swift), or notebook heading cells. Listing rows are `token title (count) [size] preview`; code previews start with the definition/signature instead of a title. Newlines display as ¶ and links as `[text][n]`.

Tokens combine dotted section addresses (root `.`, trailing dot otherwise) and boundary hashes: `1.2.|12|a3f2|,45|b1c3|`. `at()` accepts copied listing tokens, not bare dotted addresses, and verifies the first hash; the boundary pair is an edit-ready range. Live navigation uses `d[1][6]`, `find(title)`, `search(pat)` (match counts/previews), `paths(depth)`, `links(pat)`, and `open(n)`. Opened links record `base`; non-Markdown targets become leaves with text in `.src`.

`view()` renders links as `[text][n]`; `.src` is raw text. `view(*tokens)` reads multiple sections under `# token` headers; `nums`/`lnhashs` show stored lines. Notebook tokens contain heading cell IDs (`1.2.|ab12cd34|8f3a|`); hashed views use `cellid:lineno|hash|` for cell edits.

For llms.txt: `toc = open_doc(url)` → `toc.links(topic)` → `page = toc.open(n)` → display `page`; read `page.view()` when small, otherwise `page.search(topic)` and `page.view(*tokens)`.
"""

from . import exhash, cell_exhash, file_exhash, line_hash, lnhash, lnhashview, lnhashview_cell, lnhashview_cells, lnhashview_file, magic
from . import open_doc, Section, Sections, Link, Links

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "lnhashview_cell", "lnhashview_cells", "exhash", "file_exhash", "cell_exhash", "open_doc", "Section", "Sections", "Link", "Links"]

import sys
if 'IPython' in sys.modules:
    from IPython import get_ipython
    if (_ip := get_ipython()): magic.load_ipython_extension(_ip)
