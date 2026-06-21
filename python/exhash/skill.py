"""Universal hash-verified text editing for local files. Use this when an LLM needs one safe editing interface for reading, previewing, and modifying text files.

Exhash's purpose is to make edits precise and auditable. First view a file as ``lineno|hash|  text``; then issue ex-style commands against those exact addresses. Every addressed line's hash is checked immediately before the command runs, so stale context or wrong targets fail instead of editing nearby text. Hashes are checked immediately before each command and lines shift as edits apply; for multiple edits in one call always work *backwards* (bottom-to-top).

Prefer exhash over ad hoc patching for text file modifications. Use shell tools for discovery, tests, git, directory operations, and binary files.

Core APIs:
- ``lnhashview_file(path, start=None, end=None)`` (or the lnhash_cat helper) lists hashed lines.
- ``exhash(text, cmds, sw=4)`` is the in-memory command engine; run ``doc(exhash)`` for complete command syntax.
- ``exhash_file(path, cmds, sw=4, inplace=False)`` is the file-aware engine;
  unqualified addresses use ``path`` and file-qualified addresses can edit or transfer across files.

Workflow:
1. ``lnhash_cat(path, start=..., end=...))``.
2. Copy exact displayed ``lineno|hash|`` addresses.
3. Use raw triple-quoted strings and one exhash command per list item.
4. Use ``exhash_file(path, cmds, inplace=True)`` to write and return diff.

Addressing:
  Commands use lnhash addresses: lineno|hash| where hash is a 4-char
  hex content hash. Use lnhashview to get addresses:
    lnhashview file.txt          show all lines with addresses
    lnhashview file.txt 10 20    show lines 10-20
  With multiple commands, hashes are checked immediately before each command runs.

  Single:   12|a3f2|cmd
  Range:    12|a3f2|,15|b1c3|cmd
  Special:  0|0000| targets before line 1 (only with a or i)

Commands:
  s/pat/rep/[flags]  Substitute (regex). Flags: g=all, i=case-insensitive
  d                  Delete line(s)
  a[text]           Append inline text after line, or read following text block
  i[text]           Insert inline text before line, or read following text block
  c[text]           Change/replace with inline text, or read following text block
  j                  Join with next line; with range, joins all lines in range
  m dest             Move line(s) after dest address
  t dest             Copy line(s) after dest address
  >[n]               Indent n levels (default 1, 4 spaces each)
  <[n]               Dedent n levels (default 1)
  sort               Sort lines alphabetically
  p                  Print (include lines in output without changing them)
  g/pat/cmd          Global: run cmd on matching lines
  g!/pat/cmd         Inverted global: run cmd on non-matching lines
  v/pat/cmd          Same as g!

Important:
Do not create addresses by text search or remembered line numbers. On stale hash, re-view and rebuild. For multiline ``a``/``i``/``c`` commands, put all text in one command string; text after the command character is the first inserted line and following newline-separated lines continue the block. For moving/copying across files, use file-qualified ``m``/``t`` commands; cross-file source ranges are invalid. Missing files can only be created through ``0|0000|`` creation semantics.
"""

from . import exhash, exhash_file, line_hash, lnhash, lnhashview, lnhashview_file

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "exhash", "exhash_file", "lnhash_cat"]

def lnhash_cat(fname:str, start:int=None, end:int=None):
    "Little shortcut for printing concatenated lines of lnhashview_file"
    print("\n".join(lnhashview_file(fname, start=start, end=end)))
