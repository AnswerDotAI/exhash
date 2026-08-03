"Hash-verified line-addressed text editing. See `exhash.skill` for the workflow guide: view with `lnhashview_*` first, then edit with addresses taken from that view."

import json, re
from pathlib import Path
from .exhash import line_hash as _line_hash, lnhash as _lnhash, lnhashview as _lnhashview, exhash as _exhash, edit_buffers as _edit_buffers
from fastcore.basics import fail_clean, PrettyString

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
def lnhashview_file(*paths:str, start:int=None, end:int=None) -> "LnhashView":
    'Return lines formatted as space-padded ``lineno|hash|content`` for one or more files, each after a ``# file <path>`` header when several. Optional 1-based ``start``/``end`` filter the range per file; ``end`` past EOF is clamped.'
    if not paths: raise ValueError("lnhashview_file() requires at least one path")
    views = [_lnhashview(Path(p).expanduser().read_text(), start, end) for p in paths]
    if len(views)==1: return LnhashView(views[0])
    return LnhashView(x for p,v in zip(paths,views) for x in (f"# file {p}", *v))

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
    maxlen:int=120, # Max chars per line; longer lines are cut and end with an ellipsis (``---``/``+++`` file headers exempt)
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



def _unescape_path(path):
    out, escaped = [], False
    for ch in path:
        if escaped:
            if ch not in ':\\': out.append('\\')
            out.append(ch)
            escaped = False
        elif ch == '\\': escaped = True
        else: out.append(ch)
    if escaped: out.append('\\')
    return ''.join(out)


def _split_file_prefix(s):
    if _ADDR_RE.match(s): return None, s
    escaped = False
    for i, ch in enumerate(s):
        if escaped:
            escaped = False
            continue
        if ch == '\\':
            escaped = True
            continue
        if ch == ':' and _ADDR_RE.match(s[i + 1:]):
            path = _unescape_path(s[:i])
            if not path: raise ValueError('empty filename prefix')
            return _norm_path(path), s[i + 1:]
    return None, s


_CELLPATH_RE = re.compile(r'(.*\.ipynb):([A-Za-z0-9_-]+)')

def _target_key(target):
    path, cell = target
    return path if cell is None else f'{path}:{cell}'


def _parse_fileaddr(s, default):
    s = s.lstrip()
    path, rest = _split_file_prefix(s)
    target = (path, None) if path else default
    if path and (m2 := _CELLPATH_RE.fullmatch(path)): target = (m2.group(1), m2.group(2))
    m = _ADDR_RE.match(rest)
    if not m: raise ValueError(f'expected exhash address near {s[:40]!r}')
    return target, m.group(0), rest[m.end():]


def _parse_file_command(cmd, default):
    addr, op, *fields = cmd
    src, addr1, rest = _parse_fileaddr(addr, default)
    has_comma, addr2 = False, None
    if rest.startswith(','):
        has_comma = True
        src2, addr2, rest = _parse_fileaddr(rest[1:], src)
        if src2 != src: raise ValueError('a range must stay within one file or cell')
    if rest.strip(): raise ValueError(f'unexpected trailing characters in address: {rest!r}')
    parsed = dict(src=src, addr1=addr1, addr2=addr2, has_comma=has_comma, op=op, dest=None, dest_addr=None, local=None)
    local_addr = addr1 if addr2 is None else f'{addr1},{addr2}'
    if op in ('m', 't'):
        dest, dest_addr, tail = _parse_fileaddr(fields[0], src)
        if tail.strip(): raise ValueError(f'unexpected trailing characters after destination: {tail!r}')
        parsed.update(dest=dest, dest_addr=dest_addr, local=(local_addr, op, dest_addr))
    else: parsed['local'] = (local_addr, op, *fields)
    return parsed


_UNEXPANDED_RE = re.compile(r'\{[A-Za-z_]\w*\}|\$\{?[A-Za-z_]\w*\}?')


def _unexpanded(path):
    "A note naming the IPython interpolation left literal in `path`: in a `%%exhash` line, an undefined `{name}`/`$name` is passed through as text, so the path silently becomes nonsense."
    m = _UNEXPANDED_RE.search(str(path))
    return f" -- note: {m.group(0)!r} looks like an unexpanded IPython variable (undefined names in a magic line are passed through literally)" if m else ""


def _load_buffer(st, target, missing_ok=False):
    path, cell = target
    if cell is not None:
        if path not in st['nbs']:
            nbp = Path(path).expanduser()
            if not nbp.exists(): raise FileNotFoundError(f'notebook not found: {path}{_unexpanded(path)}')
            st['nbs'][path] = json.loads(nbp.read_text())
        c = _find_cell(st['nbs'][path], cell, path)
        target = (path, c['id'])
        if target not in st['bufs']:
            text = _cell_text(c)
            st['bufs'][target] = dict(target=target, path=path, cellref=c, trail_nl=text.endswith('\n'), original=text.splitlines(), lines=text.splitlines())
        return st['bufs'][target]
    if target in st['bufs']: return st['bufs'][target]
    p = Path(path)
    try: lines = p.read_text().splitlines()
    except FileNotFoundError:
        if not missing_ok: raise FileNotFoundError(f'file not found: {path}{_unexpanded(path)} (a new file can only be created with a 0|0000| a/i command)') from None
        if not p.parent.exists(): raise FileNotFoundError(f'cannot create {path}: parent directory {p.parent} does not exist{_unexpanded(path)}') from None
        lines = []
    st['bufs'][target] = dict(target=target, path=path, cellref=None, original=list(lines), lines=list(lines))
    return st['bufs'][target]


def _can_create_missing(parsed): return parsed['addr1'] == '0|0000|' and parsed['op'] in ('a', 'i')


def _prepare_file_command(st, parsed):
    src = _load_buffer(st, parsed['src'], missing_ok=_can_create_missing(parsed) and parsed['src'][1] is None)
    dest = None
    if parsed['dest'] is not None:
        dest = _load_buffer(st, parsed['dest'], missing_ok=parsed['dest_addr'] == '0|0000|' and parsed['dest'][1] is None)
    return (_target_key(src['target']), parsed['local'], _target_key(dest['target']) if dest else None)


@fail_clean(*stdexcs)
def file_exhash(path:str, *cmds:tuple, sw:int=4, inplace:bool=True):
    r'''Read files and notebook cells, apply file-aware exhash commands, and return per-target results or a combined diff.

    Command tuples are the ``exhash.skill`` module docstring's; ``path`` is the
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
    default, st = (_norm_path(path), None), dict(bufs={}, nbs={})
    commands = [_prepare_file_command(st, _parse_file_command(cmd, default)) for cmd in _normalize_cmds(cmds)]
    if not st['bufs']: _load_buffer(st, default)
    by_key = {_target_key(target): buf for target, buf in st['bufs'].items()}
    buffers = [(key, _text_from_lines(buf['lines'])) for key, buf in by_key.items()]
    native = _edit_buffers(buffers, commands, sw=sw)
    files = {key: FileEditResult(key, by_key[key]['original'], result, cell=by_key[key]['target'][1]) for key, result in native}
    for key, result in files.items(): by_key[key]['lines'] = result.lines
    result = FileSetEditResult(files, _norm_path(path))
    if inplace:
        nbs_out = {}
        for t, buf in st['bufs'].items():
            if buf['original'] == buf['lines']: continue
            if buf['cellref'] is None: _write_lines(buf['path'], buf['lines'])
            else:
                new = '\n'.join(buf['lines'])
                if buf['trail_nl'] and new: new += '\n'
                c = buf['cellref']
                c['source'] = new.splitlines(keepends=True) if isinstance(c['source'], list) else new
                nbs_out[buf['path']] = st['nbs'][buf['path']]
        for pth, nb in nbs_out.items(): Path(pth).expanduser().write_text(json.dumps(nb, sort_keys=True, indent=1, ensure_ascii=False) + '\n')
        return result._trunc_diff()
    return result




def _find_cell(nb, cell_id, path):
    'The cell in `nb` whose id is ``cell_id`` (exact match or unique prefix).'
    cells = [c for c in nb['cells'] if c.get('id','').startswith(cell_id)]
    exact = [c for c in cells if c.get('id')==cell_id]
    if exact: cells = exact
    if not cells: raise KeyError(f'no cell with id {cell_id!r} in {path}')
    if len(cells)>1: raise KeyError(f'cell id prefix {cell_id!r} is ambiguous in {path}')
    return cells[0]


def _load_cell(path, cell_id):
    'Return ``(nb, cell)`` for the cell whose id is ``cell_id`` (exact match or unique prefix).'
    nbp = Path(path).expanduser()
    if not nbp.exists(): raise FileNotFoundError(f'notebook not found: {path}{_unexpanded(path)}')
    nb = json.loads(nbp.read_text())
    return nb, _find_cell(nb, cell_id, path)


def _cell_text(cell):
    src = cell['source']
    return src if isinstance(src, str) else ''.join(src)


@fail_clean(*stdexcs)
def lnhashview_cell(path:str, *cell_ids:str, start:int=None, end:int=None) -> "LnhashView":
    'Return lines formatted as ``lineno|hash|content`` for the source of one or more notebook cells in ipynb file at ``path``, each after a ``# cell <id>`` header when several. Each cell id may be exact or a unique prefix; optional 1-based ``start``/``end`` filter the range per cell.'
    if not cell_ids: raise ValueError("lnhashview_cell() requires at least one cell id")
    cells = [_load_cell(path, c)[1] for c in cell_ids]
    views = [_lnhashview(_cell_text(c), start, end) for c in cells]
    if len(views)==1: return LnhashView(views[0])
    return LnhashView(x for i,c,v in zip(cell_ids,cells,views) for x in (f"# cell {c.get('id', i)}", *v))


@fail_clean(*stdexcs)
def cell_exhash(path:str, cell_id:str, *cmds:tuple, sw:int=4, inplace:bool=True):
    """Apply exhash commands to the source of notebook cell ``cell_id`` in ipynb file at ``path``.

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
    nb, cell = _load_cell(path, cell_id)
    text = _cell_text(cell)
    res = exhash(text, cmds, sw=sw)
    if not inplace: return res
    new = '\n'.join(res['lines'])
    if text.endswith('\n') and new: new += '\n'
    if new != text:
        cell['source'] = new.splitlines(keepends=True) if isinstance(cell['source'], list) else new
        Path(path).expanduser().write_text(json.dumps(nb, sort_keys=True, indent=1, ensure_ascii=False) + '\n')
    return _diff_out(res)
