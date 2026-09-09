"Hash-verified line-addressed text editing. See `exhash.skill` for the workflow guide: view with `lnhashview_*` first, then edit with addresses taken from that view."

import re
from pathlib import Path
from .exhash import line_hash as _line_hash, lnhash as _lnhash, lnhashview as _lnhashview, exhash as _exhash, edit_buffers as _edit_buffers
from .exhash import view_file as _view_file, view_cell as _view_cell, view_cells as _view_cells, edit_files as _edit_files, edit_cell as _edit_cell
from fastcore.basics import fail_clean, PrettyString

MAXLEN = 180 # Most characters shown per displayed line

stdexcs = (ValueError, OSError, KeyError)

def line_hash(line:str) -> str:
    'Return a 4-char lowercase hex hash for a single line of text.'
    return _line_hash(line)


def lnhash(lineno:int, line:str) -> str:
    'Return an lnhash address ``lineno|hash|`` for ``line`` at 1-based ``lineno``.'
    return _lnhash(lineno, line)

class LnhashView(list):
    'List of ``lineno|hash|content`` lines, displayed verbatim one per line.'
    def __str__(self): return '\n'.join(self)
    def _repr_pretty_(self, p, cycle): p.text('...' if cycle else str(self))



def lnhashview(text:str, start:int=None, end:int=None) -> "LnhashView":
    'Return lines formatted as space-padded ``lineno|hash|content``. Optional 1-based ``start``/``end`` filter the range; ``end`` past EOF is clamped.'
    return LnhashView(_lnhashview(text, start, end))


@fail_clean(*stdexcs)
def lnhashview_file(path:str, start:int=None, end:int=None) -> "LnhashView":
    'Return lines formatted as space-padded ``lineno|hash|content`` for file at ``path`` (expands ``~``). Optional 1-based ``start``/``end`` filter the range; ``end`` past EOF is clamped.'
    return LnhashView(_view_file(str(path), start, end))


_NOFIELD = {'d', 'p', 'j', 'sort'}


def _normalize_subcmd(op, parts):
    "Validate and canonicalize the post-address fields of a tuple command"
    if op == 's':
        if len(parts) not in {2, 3}: raise ValueError("s tuple must be (addr, 's', pattern, replacement[, flags])")
        if not all(isinstance(o, str) for o in parts): raise TypeError("s tuple fields must be strings")
        return (op, *parts)
    if op == 'y':
        if len(parts) != 2 or not all(isinstance(o, str) for o in parts): raise ValueError("y tuple must be (addr, 'y', source, dest)")
        return (op, *parts)
    if op in ('g', 'g!', 'v'):
        if len(parts) != 2 or not isinstance(parts[0], str) or not isinstance(parts[1], tuple) or not parts[1]:
            raise ValueError(f"{op} tuple must be (addr, {op!r}, pattern, (op, ...))")
        return (op, parts[0], _normalize_subcmd(parts[1][0], list(parts[1][1:])))
    if op in _NOFIELD:
        if parts: raise ValueError(f"{op!r} tuple takes no payload")
        return (op,)
    if len(parts) > 1: raise ValueError(f"{op!r} tuple accepts at most one payload field")
    payload = parts[0] if parts else ''
    if op in '><' and isinstance(payload, int): payload = str(payload)
    if not isinstance(payload, str): raise TypeError("tuple command payload must be a string")
    return (op, payload)


def _normalize_cmd(cmd):
    if not isinstance(cmd, tuple): raise TypeError("commands must be tuples")
    if len(cmd) < 2: raise ValueError("tuple commands must start with (address, command)")
    addr, op, *parts = cmd
    if not isinstance(addr, str) or not isinstance(op, str): raise TypeError("tuple command address and command must be strings")
    return (addr, *_normalize_subcmd(op, parts))


def _normalize_cmds(cmds): return [_normalize_cmd(cmd) for cmd in cmds]


@fail_clean(ValueError)
def exhash(text:str, cmds:list[tuple], sw:int=4):
    """Verified line-addressed editor. Apply commands to `text`, return an EditResult.
    Python commands are tuple specs; raw command strings are rejected. Use
    ``lnhashview(text)`` or ``lnhash(lineno, line)`` to get hash-verified
    address strings. Each command's hashes are checked immediately before it runs.
    A single-line address can match the line's current hash or its call-start hash,
    allowing commands to stack on one line.

    Addresses, command tuples, and payload rules: the ``exhash.skill`` module
    docstring is the full reference. Engine details beyond it:

    - ``s`` fails if the pattern matches nothing in the addressed range
      (substitutes inside ``g`` subcommands stay lenient).
    - ``s`` also fails if the replacement references a capture group the pattern
      does not define (unknown references would otherwise silently substitute the
      empty string); a literal ``$`` is written ``$$``.
    - ``sw`` controls shift width for ``<`` and ``>`` and defaults to 4.
    - In-place edits record each changed line's call-start hash. Structural edits
      drop records at and below their topmost affected line; range addresses never
      use the recorded-hash fallback.
    - Do not use ``.`` terminators: a final ``.`` line is inserted literally,
      and exhash emits a warning.

    Returns an EditResult with attributes (also accessible as dict keys):
      lines     list of output lines
      hashes    lnhash for each output line
      modified  1-based line numbers of modified/added lines
      deleted   1-based line numbers of removed lines (in original)
      origins   for each output line, the 1-based original line number (None if inserted)
      printed   1-based line numbers explicitly addressed by ``p``

    Call ``res.format_diff(context=1)`` for a unified-diff-style summary.
    Non-empty diffs start with ``--- original`` and ``+++ modified`` headers, except a
    ``p``-only result: that renders as a bare ``lnhashview`` of the printed lines, headerless
    and untruncated. Printed lines inside a real diff always show, as context rows.
    NB: ``file_exhash``/``cell_exhash`` with ``inplace=True`` (their default) do not
    return an EditResult: they return the formatted diff string directly (display-truncated via ``truncate_diff``).

    Examples::

      from exhash import exhash, lnhash, lnhashview
      text = "foo\\nbar\\n"
      addr = lnhash(1, "foo")           # "1|a1b2|"
      res = exhash(text, [(addr, "s", "foo", "baz")])
      print(res["lines"])                # ["baz", "bar"]
      print(res.format_diff())           # unified-diff-style summary
    """
    return _exhash(text, *_normalize_cmds(cmds), sw=sw)


class FileEditResult:
    'Edited state for one file.'
    def __init__(self, path, original_lines, result, cell=None):
        self.path = _norm_path(path)
        self.original_lines = list(original_lines)
        self.lines = list(result.lines)
        self.hashes = list(result.hashes)
        self.printed = list(result.printed)
        self.cell = cell
        self._result = result

    @property
    def changed(self): return self.original_lines != self.lines

    @property
    def header(self): return f'# cell {self.cell}' if self.cell else f'# file {self.path}'

    def __getitem__(self, key):
        if key in {"lines", "hashes", "original_lines", "printed"}: return getattr(self, key)
        raise KeyError(key)

    def format_diff(self, context=1):
        diff = str(self._result.format_diff(context))
        if self.changed: diff = diff.replace('--- original\n+++ modified\n', f'--- {self.path}\n+++ {self.path}\n', 1)
        return PrettyString(diff)

    def __str__(self): return str(self.format_diff())

    def __repr__(self):
        view_only = self.printed and not self.changed
        diff = self.format_diff() if view_only else truncate_diff(self.format_diff())
        if self.changed: note = ''
        elif self.printed: note = f', {len(self.printed)} printed, no changes'
        else: note = ', no changes'
        return f'FileEditResult({self.path}: {len(self.lines)} lines{note})' + (f'\n{diff}' if diff else '')


class FileSetEditResult:
    'Edited state for an file_exhash command set.'
    def __init__(self, files, default_path):
        self.files = files
        self.default_path = default_path
        self.changed = [path for path, result in files.items() if result.changed]
        self.printed = [path for path, result in files.items() if result.printed]

    def __getitem__(self, path): return self.files[_norm_path(path)]

    @property
    def _shown(self): return [p for p, r in self.files.items() if r.changed or r.printed]

    def _render(self, context=1, trunc=False):
        'Diffs for changed targets, then bare views for printed-only ones, headed when several targets show.'
        shown = self._shown
        out = []
        for p in shown:
            r = self.files[p]
            d = str(r.format_diff(context))
            if r.changed: out.append(truncate_diff(d) if trunc else d)
            else: out.append((f'{r.header}\n' if len(shown) > 1 else '') + d)
        return ''.join(out)

    def format_diff(self, context=1): return PrettyString(self._render(context))

    def _trunc_diff(self): return PrettyString(self._render(1, trunc=True))

    def __str__(self): return str(self.format_diff())

    def __repr__(self):
        diff = self._trunc_diff()
        counts = f'{len(self.changed)} changed' + (f', {len(self.printed)} printed' if self.printed else '')
        return f'FileSetEditResult({len(self.files)} files, {counts})' + (f'\n{diff}' if diff else '')


_ADDR_RE = re.compile(r'(?:\$|%|\d+\|[0-9a-fA-F]{4}\|)')


def _norm_path(path): return str(Path(path).expanduser())


def _text_from_lines(lines): return '\n'.join(lines) + ('\n' if lines else '')


def _write_lines(path, lines): Path(path).write_text(_text_from_lines(lines))


def truncate_diff(
    s:str, # Formatted diff text
    max_lines:int=15, # Max lines to keep before eliding the rest
    maxlen:int=MAXLEN, # Max chars per line; longer lines are cut and end with an ellipsis (``---``/``+++`` file headers exempt)
)->str:
    "Truncate diff text for display: cap line length and count, appending an elided-lines marker."
    lines = s.splitlines()
    out = [l if len(l)<=maxlen or l.startswith(('--- ','+++ ')) else l[:maxlen]+'…' for l in lines[:max_lines]]
    if len(lines)>max_lines: out.append(f'…{len(lines)-max_lines} lines elided…')
    return '\n'.join(out)+'\n' if out else ''


def _diff_out(res):
    'Formatted output for an EditResult: a print-only result is a view, so it is never truncated.'
    diff = res.format_diff()
    if res['printed'] and not res['modified'] and not res['deleted']: return PrettyString(diff)
    return PrettyString(truncate_diff(diff))



@fail_clean(*stdexcs)
def file_exhash(path:str, *cmds:tuple, sw:int=4, inplace:bool=True):
    r'''Read files and notebook cells, apply file-aware exhash commands, and return per-target results or a combined diff.

    Command tuples are the ``exhash.skill`` module docstring's; ``path`` (expands ``~``, as do qualified addresses) is the
    default file context for unqualified addresses. Prefix source address
    strings, and ``m``/``t`` destination strings, with ``path:`` to target
    another file, or ``path.ipynb:cellid:`` to target one notebook cell's
    source (``cellid`` may be an exact id or unique prefix)::

      ("src/a.py:10|aaaa|,20|bbbb|", "m", "src/b.py:$")

    A range must stay within one file or cell. An ``m``/``t`` destination that
    omits the prefix inherits it from the *first address*, never from ``path``:
    a bare destination like ``$`` targets the source's own file, even when
    ``path`` names another. So whenever the source is qualified, qualify the
    destination too. Escape literal colons in filenames as ``\:`` and literal
    backslashes as ``\\``. Missing files are treated as empty only for commands
    valid against an empty buffer (``0|0000|`` with ``a``/``i``, or as an
    ``m``/``t`` destination); cells are never created: a cell target must
    already exist, or the command raises ``KeyError``.

    By default (``inplace=True``) write changed files only after every command
    succeeds and return the combined diff string (display-truncated via
    ``truncate_diff``); if any command fails, write nothing. Lines addressed by ``p``
    are reported too: a ``p``-only call writes nothing and returns those lines as a bare,
    untruncated ``lnhashview``, and printed rows in a target that also changed ride in its
    diff as context. With more than one reported target, each printed-only group is headed
    by ``# file <path>`` or ``# cell <id>``. Pass ``inplace=False`` to preview instead: a
    ``FileSetEditResult`` is returned with ``files``, ``changed``, ``default_path``,
    ``res[path]`` (cell targets under ``'path:cellid'``), and ``res.format_diff(context=1)``.
    '''
    native = _edit_files(str(path), _normalize_cmds(cmds), sw=sw, inplace=inplace)
    files = {key: FileEditResult(key, result.original_lines, result, cell=cell) for key, cell, result in native}
    result = FileSetEditResult(files, _norm_path(path))
    return result._trunc_diff() if inplace else result


@fail_clean(*stdexcs)
def lnhashview_cell(path:str, cell_id:str, start:int=None, end:int=None) -> "LnhashView":
    'Return lines formatted as ``lineno|hash|content`` for the source of notebook cell ``cell_id`` in ipynb file at ``path`` (expands ``~``). ``cell_id`` may be an exact id or unique prefix; optional 1-based ``start``/``end`` filter the range.'
    return LnhashView(_view_cell(str(path), cell_id, start, end))


@fail_clean(*stdexcs)
def lnhashview_cells(path:str, *cell_ids:str, start:int=None, end:int=None) -> "LnhashView":
    'Return grouped lnhash views for explicit notebook cell ids in the ipynb file at ``path`` (expands ``~``). Each group starts with ``# cell <id>``; following lines keep normal ``lineno|hash|content`` format.'
    return LnhashView(_view_cells(str(path), cell_ids, start, end))


@fail_clean(*stdexcs)
def cell_exhash(path:str, cell_id:str, *cmds:tuple, sw:int=4, inplace:bool=True):
    """Apply exhash commands to the source of notebook cell ``cell_id`` in ipynb file at ``path`` (expands ``~``).

    Command tuples are the ``exhash.skill`` module docstring's; use
    ``lnhashview_cell(path, cell_id)`` for addresses.
    ``cell_id`` may be an exact id or unique prefix.

    By default (``inplace=True``) write the edited source back when the source actually
    changed (preserving the cell's original str-or-list-of-lines form; the notebook
    re-serializes in Jupyter's JSON layout) and return the diff string (display-truncated via
    ``truncate_diff``); if any command fails, write nothing. A ``p``-only call writes nothing and
    returns the printed lines as a bare, untruncated ``lnhashview``. Pass
    ``inplace=False`` to preview instead: the EditResult is returned without touching the file.
    """
    res = _edit_cell(str(path), cell_id, _normalize_cmds(cmds), sw=sw, inplace=inplace)
    return _diff_out(res) if inplace else res

from .outline import *
