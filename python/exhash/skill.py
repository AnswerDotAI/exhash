r"""Universal hash-verified text editing for local files. Use this when an LLM needs one safe editing interface for reading, previewing, and modifying text files.

Exhash's purpose is to make edits precise and auditable. First view a file as `lineno|hash|text` (line numbers may be space-padded for alignment); then issue ex-style commands against those exact addresses. Every addressed line's hash is checked immediately before the command runs, so stale context or wrong targets fail instead of editing nearby text. Within one call, a single-line address may match the line's current content or its content at call start, so commands can stack on one line; across calls, re-view. Structural edits still shift lines as they apply, so work *backwards* (bottom-to-top).

Prefer exhash over ad hoc patching for text file modifications, and prefer reading with `lnhashview_file` over plain file reads whenever an edit may follow: the view doubles as the address book, so the edit needs no second read.

Core APIs:
- `lnhashview_file` lists hashed lines.
- `exhash` is the in-memory command engine; this docstring is the complete command reference, and `doc(exhash)` adds engine details (strict `s` matching, EditResult fields).
- `file_exhash` is the file-aware engine; unqualified addresses use `path` and file-qualified addresses can edit or transfer across files.
- `lnhashview_cell` views one notebook cell's source in an `.ipynb` file; `lnhashview_cells` views several explicit cells with `# cell <id>` headers. `cell_exhash` edits one cell.

Workflow:
1. `lnhashview_file(...)`, ending the cell with the bare call: the result displays verbatim, one `lineno|hash|content` line each, so never join, print, or reformat it.
2. Copy exact displayed `lineno|hash|` addresses.
3. Use tuple command specs; pass each command as its own positional argument, e.g. `file_exhash(path, (addr1, "d"), (addr2, "s", pat, repl))`. Use raw triple-quoted Python strings for address, pattern, replacement, and payload text when composing commands.
4. Use `file_exhash(path, *cmds)` (or `cell_exhash(path, cell_id, *cmds)` for one notebook cell) to apply the edit: both write to disk and return a diff by default. Pass `inplace=False` to preview the result object without touching the file.

Once you hold lnhash addresses, `p` commands are the scattered view: one `(addr, "p")` per address, straight from an `rg` hit or a summary, gives back exactly those lines as verified rows. A `p`-only call writes nothing and returns a bare `lnhashview` of the printed lines (no diff headers, no tag column, no truncation), so its output is an address book that feeds the next edit directly, e.g. `file_exhash(path, ("9|e08c|", "p"), ("18|2f61|", "p"), ("28|3cc8|", "p"))`. `("%", "p")` views a whole file this way, and `(addr, "g", pat, ("p",))` is scoped grep: a verified row per matching line in the range. Where a call also edits, its printed lines appear in the diff as context rows, always shown even when no hunk is near them.

Addressing:
  Address strings use lnhash addresses: lineno|hash| where hash is a 4-char
  hex content hash. Use lnhashview to get addresses:
    lnhashview file.txt          show all lines with addresses
    lnhashview file.txt 10 20    show lines 10-20
  With multiple commands, hashes are checked immediately before each command runs; a single-line address may also match that line's call-start hash.

  Single:   12|a3f2|
  Range:    12|a3f2|,15|b1c3|
  Last:     $ (last line)
  Whole:    % (whole file or cell, same as 1,$; no hashes needed)
  Special:  0|0000| targets before line 1 (only with a or i)

Tuple commands:
  (addr, "s", pat, repl[, flags]) Substitute (Rust regex syntax: backrefs in repl are $1/$0/${name}; a two-char \1 stays literal; a literal $ is written $$, and $name followed by more text needs ${name}text -- an unknown group reference is an error). Flags: g=all, i=case-insensitive. pat/repl are verbatim: literal newlines, slashes, and backslashes all work. For $-heavy replacement text, prefer a c command: its payload has no template layer at all.
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
  (addr, "p")       View line(s): emit them as verified `lineno|hash|` rows, changing nothing
  (addr, "g", pat, sub), (addr, "g!", pat, sub), (addr, "v", pat, sub) Global commands: apply `sub` to each addressed line matching `pat` (`g!`/`v`: not matching). `sub` is a subcommand tuple without an address, e.g. ("d",) or ("s", "foo", "bar", "g"); globals cannot nest.
  (addr, "y", source, dest) Transliterate `source` chars to `dest` (equal counts required)


Cut/copy/paste between files and notebook cells:
Any `m` (cut+paste) or `t` (copy+paste) address can carry a target prefix: `path:` for another file, or `path.ipynb:cellid:` for one cell's source (`cellid` exact or unique prefix). This is THE way to transfer existing lines between locations: the lines never pass through your output, so opaque content (base64 blobs, hashes, long literals) cannot be mistyped. Take source addresses from `lnhashview_file`/`lnhashview_cell` of each target as usual:

  file_exhash(path, ("src/a.py:10|aaaa|,20|bbbb|", "m", "src/b.py:$"))          # cut a.py lines 10-20, paste at end of b.py
  file_exhash(path, ("nb.ipynb:ab12cd34:6|830e|", "t", "other.ipynb:9f8e:$"))   # copy one cell line into another notebook's cell
  file_exhash(path, ("nb.ipynb:ab12cd34:%", "t", "snippets.py:0|0000|"))        # copy a whole cell's source into a new file

A range must stay within one file or cell, and cells are never created by a transfer (files can be, via a `0|0000|` destination). For whole-cell structural operations (add/delete/move cells) use `pyskills.ipynb`'s `copy_cells`/`paste_cells` instead; this idiom is for line-level transfer.

Reformatting a section like `gq`, plus optional indents: `j` the range onto one line, re-view, then split with ONE g-flagged `s` alternating the tokens (picked by eye) that should start each new line, captured and restored with the break and indent in the replacement: `(addr, "s", r", ('foo'|'bar'|'baz')", ",\n    $1", "g")`
Important:
Do not pass raw commands to Python APIs. Do not create addresses by text search or remembered line numbers, and never construct them by computing hashes (e.g. via `line_hash`): addresses come only from a fresh view immediately before the edit. On stale hash, re-view and rebuild. Where rgapi is installed, hits from `rg(pattern, lnhashs=True)` count as fresh views too: their addresses drop straight into commands (and `nbrg` finds the cell ids that `lnhashview_cell` takes). If reaching an address seems to need arithmetic, scraping, or a guessed hash, a step on that route was skipped. Tuple text fields can contain newlines wherever the command accepts text. For example, `(addr, "s", "foo", "bar\nbaz")` replaces one line with two. Text fields are taken verbatim: a two-character `\n` sequence stays literal; use an actual newline when you want a line break. For `a`/`i`/`c`, put all text in one tuple payload: `"first\nsecond"` starts with `first`, while `"\nfirst"` inserts a leading blank line before `first`. For moving/copying between files or cells, use the qualified `m`/`t` addresses shown above. Missing files can only be created through `(r"0|0000|", "a", text)` or `(r"0|0000|", "i", text)` creation semantics.

The `%%exhash` cell magic:
In IPython sessions, importing this module registers the `%%exhash` cell magic: `%%exhash <path> [<cell_id>] <address> <a|i|c>` applies one command whose payload is everything below the magic line, taken verbatim (one trailing newline stripped). Passing `<cell_id>` targets that cell in an .ipynb file instead of a plain file (`cell_exhash`); the magic dispatches on token count, so no separate cell magic exists. Because the payload is never parsed as Python, no quoting or escaping applies. Use it for EVERY `a`/`i`/`c` command, however innocent the payload looks: create a file with `%%exhash path 0|0000| a`; replace a whole cell or file with `%%exhash <path> [<cell_id>] % c` (`%` needs no hashes: a full replace has no neighboring lines to mis-hit); replace a region within one with a range address and `c` (`%%exhash <path> 12|a3f2|,15|b1c3| c`), both addresses straight from the one pre-edit view. Tuple `a`/`i`/`c` payloads are only for contexts without magics (scripts, tests): interactively they add a Python quoting layer whose failure modes are not reliably foreseeable, so do not use them.
IPython expands `{expr}` and `$var` in the magic line from the user namespace (its standard `var_expand` for all magics), so a path or cell id held in a variable needs no retyping: `%%exhash {path} {cid} % c`. Only the line expands; the payload stays verbatim.

Document outlines (`open_doc`):
The outline layer makes exhash a hierarchical editor: `open_doc(src)` opens a URL (fetched), a file path (recorded for `refresh()` and edits), or text in hand, and returns a `Section` tree. Markdown sections come from headings; code files (py, js, ts, tsx, rs, zig, swift) from tree-sitter definitions; `.ipynb` files from md-heading cells over cells. Display the root bare: fixed-width rows `token title (count) [size] preview`, previews ¶-joined with links rendered `[text][n]` so no URL spends tokens. The token is the verified address: `1.2.|12|a3f2|,45|b1c3|` fuses the dotted addr (trailing dot; the root's is `.`) with the section's boundary lnhash pair, so one row serves navigation, editing, and orientation: `at()` verifies the first hash, and the pair drops straight into a `file_exhash` range command. `at()` accepts ONLY tokens copied from a listing (`'1.6.|12|a3f2|'` or the full range form); bare dotted addrs raise, teaching the idiom. Live traversal is free: `d[1][6]`, `find(title)`, `search(pat)` (counts and matching-line previews), `paths(depth)`, `links(pat)`, and `open(n)` opens link `n` as a new tree with `base` recorded (a non-md target comes back as a leaf whose `.src` holds the payload). Notebook section tokens carry the heading cell id (`1.2.|ab12cd34|8f3a|`), and `view(lnhashs=True)` rows are `cellid:lineno|hash|`, ready for `cell_exhash`. The whole llms.txt docs workflow: `toc = open_doc(url)`; `toc.links(topic)`; `page = toc.open(n)`; display `page` bare; `page.text` whole when small, else `page.search(topic)` and `at(token)`.
"""

from . import exhash, cell_exhash, file_exhash, line_hash, lnhash, lnhashview, lnhashview_cell, lnhashview_cells, lnhashview_file, magic
from . import open_doc, Section, Sections, Link, Links

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "lnhashview_cell", "lnhashview_cells", "exhash", "file_exhash", "cell_exhash", "open_doc", "Section", "Sections", "Link", "Links"]

import sys
if 'IPython' in sys.modules:
    from IPython import get_ipython
    if (_ip := get_ipython()): magic.load_ipython_extension(_ip)
