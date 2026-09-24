"Hash-verified line-addressed text editing. See `exhash.skill` for the workflow guide: view with `lnhashview_*` first, then edit with addresses taken from that view."

from pathlib import Path
from .exhash import line_hash as _line_hash, lnhash as _lnhash, lnhashview as _lnhashview, exhash as _exhash
from .exhash import view_file as _view_file, view_cell as _view_cell, view_cells as _view_cells
from .exhash import edit_files as _edit_files, edit_cell as _edit_cell, truncate_diff as _truncate_diff, render as _render_edits, MAXLEN
from fastcore.basics import fail_clean, PrettyString

stdexcs = (ValueError, OSError, KeyError)

def line_hash(line:str) -> str:
    'Return a 2-char Base64url hash for a single line of text.'
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
    ``maxlen=n`` caps each diff row at ``n`` chars plus a closing ``…``.
    Where a run of changed rows holds as many ``-`` rows as ``+`` rows, the nth ``-`` row pairs with the nth ``+`` row.
    A capped row of a pair starts 20 chars before the pair's first difference, with ``…`` after its address.
    Every other capped row keeps its start.
    Non-empty diffs start with ``--- original`` and ``+++ modified`` headers.
    A result that changed nothing has an empty diff.
    Lines addressed by ``p`` are not part of the diff.
    ``res.format_printed()`` returns them as a bare ``lnhashview``, never capped or truncated.
    ``str(res)`` and the repr show the diff, then a ``# printed`` header, then the printed lines.
    With no diff, they show the printed lines alone.
    NB: ``file_exhash``/``cell_exhash`` with ``inplace=True`` (their default) do not
    return an EditResult: they return that output as a string, with the diff display-truncated via ``format_diff(maxlen=MAXLEN)`` and ``truncate_diff``.

    Examples::

      from exhash import exhash, lnhash, lnhashview
      text = "foo\\nbar\\n"
      addr = lnhash(1, "foo")           # "1|Gy|"
      res = exhash(text, [(addr, "s", "foo", "baz")])
      print(res["lines"])                # ["baz", "bar"]
      print(res.format_diff())           # unified-diff-style summary
    """
    return _exhash(text, *_normalize_cmds(cmds), sw=sw)


class FileEditResult:
    'Edited state for one file.'
    def __init__(self, result):
        self.path, self.cell = result.path, result.cell
        self.original_lines = list(result.original_lines)
        self.lines = list(result.lines)
        self.hashes = list(result.hashes)
        self.printed = list(result.printed)
        self._result = result

    @property
    def changed(self): return self.original_lines != self.lines

    def __getitem__(self, key):
        if key in {"lines", "hashes", "original_lines", "printed"}: return getattr(self, key)
        raise KeyError(key)

    def format_diff(self, context=1, maxlen=None): return self._result.format_diff(context, maxlen)

    def format_printed(self): return self._result.format_printed()

    def __str__(self): return str(self._result)

    def __repr__(self):
        note = (f', {len(self.printed)} printed' if self.printed else '') + ('' if self.changed else ', no changes')
        report = str(self._result.report(trunc=True))
        return f'FileEditResult({self.path}: {len(self.lines)} lines{note})' + (f'\n{report}' if report else '')


class FileSetEditResult:
    'Edited state for an file_exhash command set.'
    def __init__(self, files, default_path):
        self.files = files
        self.default_path = default_path
        self.changed = [path for path, result in files.items() if result.changed]
        self.printed = [path for path, result in files.items() if result.printed]

    def __getitem__(self, path): return self.files[_norm_path(path)]

    def _render(self, context=1, trunc=False): return _render_edits([r._result for r in self.files.values()], context, trunc)

    def format_diff(self, context=1): return PrettyString(''.join(str(self.files[p].format_diff(context)) for p in self.changed))

    def _trunc_report(self): return PrettyString(self._render(1, trunc=True))

    def __str__(self): return self._render()

    def __repr__(self):
        diff = self._trunc_report()
        counts = f'{len(self.changed)} changed' + (f', {len(self.printed)} printed' if self.printed else '')
        return f'FileSetEditResult({len(self.files)} files, {counts})' + (f'\n{diff}' if diff else '')


def _norm_path(path): return str(Path(path).expanduser())


def truncate_diff(
    s:str, # Formatted diff text
    max_lines:int=15, # Max lines to keep before eliding the rest
)->str:
    "Truncate diff text for display: cap the line count, appending an elided-lines marker."
    return _truncate_diff(s, max_lines)


def _diff_out(res):
    'Display output for an EditResult: its capped, truncated diff, then its printed lines in full.'
    return res.report(trunc=True) or PrettyString('none: No changes.')



@fail_clean(*stdexcs)
def file_exhash(path:str, *cmds:tuple, sw:int=4, inplace:bool=True):
    r'''Read files and notebook cells, apply file-aware exhash commands, and return per-target results or a combined diff.

    Command tuples are the ``exhash.skill`` module docstring's; ``path`` (expands ``~``, as do qualified addresses) is the
    default file context for unqualified addresses. Prefix source address
    strings, and ``m``/``t`` destination strings, with ``path:`` to target
    another file, or ``path.ipynb:cellid:`` to target one notebook cell's
    source (``cellid`` may be an exact id or unique prefix)::

      ("src/a.py:10|qq|,20|u7|", "m", "src/b.py:$")

    A range must stay within one file or cell. An ``m``/``t`` destination that
    omits the prefix inherits it from the *first address*, never from ``path``:
    a bare destination like ``$`` targets the source's own file, even when
    ``path`` names another. So whenever the source is qualified, qualify the
    destination too. Escape literal colons in filenames as ``\:`` and literal
    backslashes as ``\\``. Missing files are treated as empty only for commands
    valid against an empty buffer (``0|AA|`` with ``a``/``i``, or as an
    ``m``/``t`` destination); cells are never created: a cell target must
    already exist, or the command raises ``KeyError``.

    By default (``inplace=True``) write changed files only after every command succeeds.
    If any command fails, write nothing.
    Return each target's diff (rows capped at 180 characters and at most 15 lines, via ``format_diff(maxlen=MAXLEN)`` and ``truncate_diff``), then its lines addressed by ``p`` under a ``# printed`` header.
    Printed lines are never capped or truncated.
    A ``p``-only call writes nothing and returns the printed lines as a bare ``lnhashview``.
    A call that changes and prints nothing returns ``none: No changes.``
    With more than one reported target, each target with only printed lines is headed by ``# file <path>`` or ``# cell <id>``.
    Pass ``inplace=False`` to preview instead: a ``FileSetEditResult`` is returned with ``files``, ``changed``, ``printed``, ``default_path``,
    ``res[path]`` (cell targets under ``'path:cellid'``), and ``res.format_diff(context=1)``.
    '''
    native = _edit_files(str(path), _normalize_cmds(cmds), sw=sw, inplace=inplace)
    files = {r.path: FileEditResult(r) for r in native}
    result = FileSetEditResult(files, _norm_path(path))
    return PrettyString(result._trunc_report() or 'none: No changes.') if inplace else result


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
    re-serializes in Jupyter's JSON layout). If any command fails, write nothing.
    Return the diff (rows capped at 180 characters and at most 15 lines, via ``format_diff(maxlen=MAXLEN)`` and ``truncate_diff``), then the lines addressed by ``p`` under a ``# printed`` header.
    A ``p``-only call writes nothing and returns the printed lines as a bare, untruncated ``lnhashview``.
    A call that changes and prints nothing returns ``none: No changes.``
    Pass ``inplace=False`` to preview instead: the EditResult is returned without touching the file.
    """
    res = _edit_cell(str(path), cell_id, _normalize_cmds(cmds), sw=sw, inplace=inplace)
    return _diff_out(res) if inplace else res

from .outline import *
