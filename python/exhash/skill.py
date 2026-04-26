"""Universal hash-verified text editing for local files. Use this when an LLM needs one safe editing interface for reading, previewing, and modifying text files.

Exhash's purpose is to make edits precise and auditable. First view a file as
``lineno|hash|  text``; then issue ex-style commands against those exact
addresses. Every addressed line's hash is checked immediately before the command
runs, so stale context or wrong targets fail instead of editing nearby text.

Prefer exhash over ad hoc patching for text file modifications. Use shell tools
for discovery, tests, git, directory operations, and binary files.

Core APIs:
- ``lnhashview_file(path, start=None, end=None)`` lists hashed lines.
- ``exhash(text, cmds, sw=4)`` is the in-memory command engine; run
  ``doc(exhash)`` for complete command syntax.
- ``exhash_file(path, cmds, sw=4, inplace=False)`` is the file-aware engine;
  unqualified addresses use ``path`` and file-qualified addresses can edit or
  transfer across files.
- Common LLM patterns: print ``"\\n".join(lnhashview_file(...))`` to view,
  call ``exhash_file(..., inplace=False).format_diff()`` to preview, and call
  ``exhash_file(..., inplace=True)`` to write after every command succeeds.

Workflow:
1. Print ``"\\n".join(lnhashview_file(path, start=..., end=...))``.
2. Copy exact displayed ``lineno|hash|`` addresses.
3. Use raw triple-quoted strings and one exhash command per list item.
4. Use ``exhash_file(path, cmds, inplace=False).format_diff()`` to inspect.
5. Use ``exhash_file(path, cmds, inplace=True)`` to write after every command
   succeeds.

Do not create addresses by text search or remembered line numbers. On stale
hash, re-view and rebuild. For moving/copying across files, use file-qualified
``m``/``t`` commands; cross-file source ranges are invalid. Missing files can
only be created through ``0|0000|`` creation semantics documented by
``doc(exhash_file)``.
"""

from pyskills.core import PosAllowPolicy, allow
from . import exhash, exhash_file, line_hash, lnhash, lnhashview, lnhashview_file

__all__ = ["line_hash", "lnhash", "lnhashview", "lnhashview_file", "exhash", "exhash_file"]

allow(line_hash, lnhash, lnhashview, lnhashview_file, exhash)
allow(exhash_file, allow_policy=PosAllowPolicy(0))

