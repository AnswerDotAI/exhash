"Hash-verified line-addressed text editing. See `exhash.skill` for the workflow guide: view with `lnhashview_*` first, then edit with addresses taken from that view."

import json, re
from difflib import SequenceMatcher
from pathlib import Path
from .exhash import line_hash as _line_hash, lnhash as _lnhash, lnhashview as _lnhashview, exhash as _exhash
from fastcore.basics import fail_clean

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


def lnhashview_file(path:str, start:int=None, end:int=None) -> "LnhashView":
    'Return lines formatted as space-padded ``lineno|hash|content`` for file at ``path``. Optional 1-based ``start``/``end`` filter the range; ``end`` past EOF is clamped.'
    return LnhashView(_lnhashview(Path(path).expanduser().read_text(), start, end))


_SUBST_DELIMS = "/@#~%=:;,+^!|"


def _normalize_subst_tuple(addr, parts):
    if len(parts) not in {2, 3}: raise ValueError("s tuple must be (addr, 's', pattern, replacement[, flags])")
    pat, repl = parts[:2]
    flags = parts[2] if len(parts) == 3 else ''
    if not all(isinstance(o, str) for o in (addr, pat, repl, flags)): raise TypeError("s tuple fields must be strings")
    delim = next((d for d in _SUBST_DELIMS if d not in pat and d not in repl), None)
    if delim is None: raise ValueError("pattern/replacement contain every supported substitute delimiter")
    return f'{addr}s{delim}{pat}{delim}{repl}{delim}{flags}'


def _normalize_cmd(cmd):
    if not isinstance(cmd, tuple): raise TypeError("commands must be tuples")
    if len(cmd) < 2: raise ValueError("tuple commands must start with (address, command)")
    addr, op, *parts = cmd
    if not isinstance(addr, str) or not isinstance(op, str): raise TypeError("tuple command address and command must be strings")
    if op == 's': return _normalize_subst_tuple(addr, parts)
    if len(parts) > 1: raise ValueError(f"{op!r} tuple accepts at most one payload field")
    payload = parts[0] if parts else ''
    if not isinstance(payload, str): raise TypeError("tuple command payload must be a string")
    return f'{addr}{op}{payload}'


def _normalize_cmds(cmds): return [_normalize_cmd(cmd) for cmd in cmds]


@fail_clean(ValueError)
def exhash(text:str, cmds:list[tuple], sw:int=4):
    """Verified line-addressed editor. Apply commands to `text`, return an EditResult.
    Python commands are tuple specs; raw command strings are rejected. Use
    ``lnhashview(text)`` or ``lnhash(lineno, line)`` to get hash-verified
    address strings. Each command's hashes are checked against the current text
    immediately before that command executes.

    Address strings:
      Single:   ``12|a3f2|``
      Range:    ``12|a3f2|,15|b1c3|``
      Last:     ``$`` (last line)
      Whole:    ``%`` (whole file, same as ``1,$``)
      Special:  ``0|0000|`` targets before line 1 (only with a or i)

    Command tuples:
      (addr, "s", pattern, replacement[, flags])
                         Substitute using Rust regex syntax. Replacement supports
                         $1, $0, ${name}. Flags: g=all, i=case-insensitive.
                         Pattern and replacement strings may contain literal
                         slashes and newlines; replacement newlines split lines.
                         Fails if the pattern matches nothing in the addressed
                         range (substitutes inside g// payloads stay lenient).
      (addr, "d")       Delete line(s)
      (addr, "a", text) Append payload after line
      (addr, "i", text) Insert payload before line
      (addr, "c", text) Change/replace with payload
      (addr, "j")       Join with next line; with range, joins all
      (addr, "m", dest) Move line(s) after dest address
      (addr, "t", dest) Copy line(s) after dest address
      (addr, ">", n)    Indent n levels (default 1, `sw` spaces each)
      (addr, "<", n)    Dedent n levels (default 1, `sw` spaces each)
      (addr, "sort")    Sort lines alphabetically
      (addr, "p")       Print (include in output without changing)
      (addr, "g", payload), (addr, "g!", payload), (addr, "v", payload)
                         Global commands; payload uses compact ex syntax such as
                         ``/pattern/d``.
      (addr, "y", payload)
                         Transliterate; payload uses compact ex syntax such as
                         ``/abc/ABC/``.

    `sw` controls shift width for `<` and `>` and defaults to 4.

    Text fields can contain newlines. This includes
    multiline a/i/c payloads and s pattern/replacement fields. Commands without
    text fields, such as d, m, and sort, do not take text.

    For a/i/c, the payload string is inserted literally, including leading spaces
    and newlines, e.g. ``(addr, "c", "    return x")``. ``"first\nsecond"``
    starts with ``first``; ``"\nfirst"`` inserts a leading blank line before
    ``first``. Do not use ``.`` terminators, and do not split the text block
    into separate ``cmds`` entries. If you include a final ``.`` line, it is
    inserted literally and exhash emits a warning.

    Returns an EditResult with attributes (also accessible as dict keys):
      lines     list of output lines
      hashes    lnhash for each output line
      modified  1-based line numbers of modified/added lines
      deleted   1-based line numbers of removed lines (in original)
      origins   for each output line, the 1-based original line number (None if inserted)

    Call ``res.format_diff(context=1)`` for a unified-diff-style summary.
    Non-empty diffs start with ``--- original`` and ``+++ modified`` headers.
    NB: ``exhash_file``/``exhash_cell`` with ``inplace=True`` (their default) do not
    return an EditResult: they return the formatted diff string directly.

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
    def __init__(self, path, original_lines, lines):
        self.path = _norm_path(path)
        self.original_lines = list(original_lines)
        self.lines = list(lines)
        self.hashes = [lnhash(i + 1, line) for i, line in enumerate(self.lines)]

    @property
    def changed(self): return self.original_lines != self.lines

    def __getitem__(self, key):
        if key in {"lines", "hashes", "original_lines"}: return getattr(self, key)
        raise KeyError(key)

    def format_diff(self, context=1): return _format_file_diff(self.path, self.original_lines, self.lines, context)


class FileSetEditResult:
    'Edited state for an exhash_file command set.'
    def __init__(self, files, default_path):
        self.files = files
        self.default_path = default_path
        self.changed = [path for path, result in files.items() if result.changed]

    def __getitem__(self, path): return self.files[_norm_path(path)]

    def format_diff(self, context=1): return ''.join(self.files[path].format_diff(context) for path in self.changed)


_ADDR_RE = re.compile(r'(?:\$|%|\d+\|[0-9a-fA-F]{4}\|)')
_LNHASH_RE = re.compile(r'(\d+)\|([0-9a-fA-F]{4})\|')


def _norm_path(path): return str(Path(path).expanduser())


def _text_from_lines(lines): return '\n'.join(lines) + ('\n' if lines else '')


def _write_lines(path, lines): Path(path).write_text(_text_from_lines(lines))


def _format_file_diff(path, old_lines, new_lines, context=1):
    if old_lines == new_lines: return ''
    events = []
    for tag, i1, i2, j1, j2 in SequenceMatcher(a=old_lines, b=new_lines, autojunk=False).get_opcodes():
        if tag == 'equal': events += [(' ', j + 1, new_lines[j]) for j in range(j1, j2)]
        elif tag == 'delete': events += [('-', i + 1, old_lines[i]) for i in range(i1, i2)]
        elif tag == 'insert': events += [('+', j + 1, new_lines[j]) for j in range(j1, j2)]
        elif tag == 'replace':
            events += [('-', i + 1, old_lines[i]) for i in range(i1, i2)]
            events += [('+', j + 1, new_lines[j]) for j in range(j1, j2)]
    interesting = set()
    for i, (tag, _, _) in enumerate(events):
        if tag != ' ': interesting.update(range(max(0, i - context), min(len(events), i + context + 1)))
    out, last = [f'--- {path}', f'+++ {path}'], None
    for i in sorted(interesting):
        if last is not None and i > last + 1: out.append('---')
        tag, lineno, line = events[i]
        out.append(f'{tag}{lnhash(lineno, line)}{line}')
        last = i
    return '\n'.join(out) + '\n'


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


def _parse_fileaddr(s, default_path):
    s = s.lstrip()
    path, rest = _split_file_prefix(s)
    path = path or default_path
    m = _ADDR_RE.match(rest)
    if not m: raise ValueError(f'expected exhash address near {s[:40]!r}')
    return path, m.group(0), rest[m.end():]


def _parse_file_command(raw, default_path):
    if not raw.strip(): return None
    src, addr1, rest = _parse_fileaddr(raw.lstrip(), default_path)
    has_comma, addr2, local = False, None, addr1
    if rest.startswith(','):
        has_comma = True
        src2, addr2, rest = _parse_fileaddr(rest[1:], src)
        if src2 != src: raise ValueError('cross-file ranges are invalid')
        local += ',' + addr2
    local += rest
    body = rest.lstrip()
    op = body[:1] if body[:1] in {'m', 't'} else None
    dest = dest_addr = None
    if op:
        dest, dest_addr, tail = _parse_fileaddr(body[1:].strip(), src)
        if tail.strip(): raise ValueError(f'unexpected trailing characters after destination: {tail!r}')
    return dict(src=src, addr1=addr1, addr2=addr2, has_comma=has_comma, rest=rest, local=local, op=op, dest=dest, dest_addr=dest_addr)


def _load_buffer(buffers, path, missing_ok=False):
    if path in buffers: return buffers[path]
    p = Path(path)
    try: lines = p.read_text().splitlines()
    except FileNotFoundError:
        if not missing_ok: raise
        if not p.parent.exists(): raise
        lines = []
    buffers[path] = dict(path=path, original=list(lines), lines=list(lines))
    return buffers[path]


def _can_create_missing(parsed): return parsed['addr1'] == '0|0000|' and parsed['rest'].lstrip()[:1] in {'a', 'i'}


def _split_lnhash_addr(addr):
    m = _LNHASH_RE.fullmatch(addr)
    if not m: raise ValueError(f'expected lnhash address, got {addr!r}')
    return int(m.group(1)), m.group(2).lower()


def _line_no(lines, addr, allow_zero=False):
    if addr == '$':
        if not lines: raise ValueError("address '$' out of range on empty file")
        return len(lines)
    if addr == '%': raise ValueError('% is only allowed as a source range')
    lineno, expected = _split_lnhash_addr(addr)
    if lineno == 0:
        if expected != '0000': raise ValueError('0|0000| must have hash 0000')
        if allow_zero: return 0
        raise ValueError('address 0 is not allowed here')
    if lineno > len(lines): raise ValueError(f'address out of range: {lineno} > {len(lines)}')
    actual = line_hash(lines[lineno - 1])
    if actual != expected: raise ValueError(f'stale lnhash at line {lineno}: expected {expected}, got {actual}')
    return lineno


def _source_indexes(lines, parsed):
    if parsed['addr1'] == '%':
        if parsed['has_comma'] or parsed['addr2'] is not None: raise ValueError('% is already a whole-file range')
        return (0, len(lines) - 1) if lines else (0, -1)
    start = _line_no(lines, parsed['addr1'])
    end = _line_no(lines, parsed['addr2']) if parsed['addr2'] is not None else start
    if start > end: raise ValueError(f'invalid range: {start}..{end}')
    return start - 1, end - 1


def _dest_index(lines, addr):
    if addr == '%': raise ValueError('destination % is not allowed')
    return _line_no(lines, addr, allow_zero=True)


def _apply_transfer(buffers, parsed):
    src = _load_buffer(buffers, parsed['src'])
    dst = _load_buffer(buffers, parsed['dest'], missing_ok=parsed['dest_addr'] == '0|0000|')
    s, e = _source_indexes(src['lines'], parsed)
    dest = _dest_index(dst['lines'], parsed['dest_addr'])
    segment = src['lines'][s:e + 1] if s <= e else []
    if parsed['op'] == 't':
        dst['lines'][dest:dest] = list(segment)
        return
    if src is dst:
        if s <= e and s < dest <= e + 1: raise ValueError('destination is within moved range')
        del src['lines'][s:e + 1]
        insert_at = dest if dest <= s else dest - len(segment)
        src['lines'][insert_at:insert_at] = segment
    else:
        del src['lines'][s:e + 1]
        dst['lines'][dest:dest] = segment


def _apply_file_command(buffers, parsed, sw):
    if parsed['op']:
        _apply_transfer(buffers, parsed)
        return
    buf = _load_buffer(buffers, parsed['src'], missing_ok=_can_create_missing(parsed))
    res = _exhash(_text_from_lines(buf['lines']), parsed['local'], sw=sw)
    buf['lines'] = list(res['lines'])


@fail_clean(ValueError)
def exhash_file(path:str, cmds:list[tuple], sw:int=4, inplace:bool=True):
    r'''Read files, apply file-aware exhash commands, and return per-file results or a combined diff.

    Core tuple syntax is the same as ``exhash(text, cmds, sw=sw)``; run
    ``doc(exhash)`` for the full command reference. Use ``path`` as the default
    file context for unqualified addresses. Prefix source address strings, and
    ``m``/``t`` destination strings, with ``path:`` to target another file::

      ("src/a.py:12|a3f2|", "s", "foo", "bar")
      ("src/a.py:10|aaaa|,20|bbbb|", "m", "src/b.py:$")
      ("src/a.py:10|aaaa|", "t", "new.py:0|0000|")

    A range must stay within one file. The second address may omit the filename
    and inherit it from the first address. Cross-file ranges are invalid. Escape
    literal colons in filenames as ``\:`` and literal backslashes as ``\\``.

    For multiline ``a``/``i``/``c`` commands, put all inserted text in the tuple
    payload string. A leading newline in that payload inserts a leading blank
    line. Do not use ``.`` terminators, and do not split the text block into
    separate ``cmds`` entries.

    Missing files are treated as empty only when the command is valid against an
    empty buffer, such as ``("0|0000|", "a", text)``/``("0|0000|", "i", text)``
    or an ``m``/``t`` destination of ``0|0000|``.

    By default (``inplace=True``) write changed files only after every command
    succeeds and return the combined diff string; if any command fails, write
    nothing. Pass ``inplace=False`` to preview instead: nothing is written and a
    ``FileSetEditResult`` is returned with ``files``, ``changed``, ``default_path``,
    ``res[path]``, and ``res.format_diff(context=1)``.
    '''
    default_path, buffers = _norm_path(path), {}
    for raw in _normalize_cmds(cmds):
        parsed = _parse_file_command(raw, default_path)
        if parsed is not None: _apply_file_command(buffers, parsed, sw)
    if not buffers: _load_buffer(buffers, default_path)
    files = {path: FileEditResult(path, buf['original'], buf['lines']) for path, buf in buffers.items()}
    result = FileSetEditResult(files, default_path)
    if inplace:
        for path in result.changed: _write_lines(path, result[path].lines)
        return result.format_diff()
    return result




def _load_cell(path, cell_id):
    'Return ``(nb, cell)`` for the cell whose id is ``cell_id`` (exact match or unique prefix).'
    nb = json.loads(Path(path).expanduser().read_text())
    cells = [c for c in nb['cells'] if c.get('id','').startswith(cell_id)]
    exact = [c for c in cells if c.get('id')==cell_id]
    if exact: cells = exact
    if not cells: raise KeyError(f'no cell with id {cell_id!r} in {path}')
    if len(cells)>1: raise KeyError(f'cell id prefix {cell_id!r} is ambiguous in {path}')
    return nb, cells[0]


def _cell_text(cell):
    src = cell['source']
    return src if isinstance(src, str) else ''.join(src)


def lnhashview_cell(path:str, cell_id:str, start:int=None, end:int=None) -> "LnhashView":
    'Return lines formatted as ``lineno|hash|content`` for the source of notebook cell ``cell_id`` in ipynb file at ``path``. ``cell_id`` may be an exact id or unique prefix; optional 1-based ``start``/``end`` filter the range.'
    return LnhashView(_lnhashview(_cell_text(_load_cell(path, cell_id)[1]), start, end))


def lnhashview_cells(path:str, *cell_ids:str, start:int=None, end:int=None) -> "LnhashView":
    'Return grouped lnhash views for explicit notebook cell ids. Each group starts with ``# cell <id>``; following lines keep normal ``lineno|hash|content`` format.'
    out = []
    for cell_id in cell_ids:
        _, cell = _load_cell(path, cell_id)
        out.append(f"# cell {cell.get('id', cell_id)}")
        out += _lnhashview(_cell_text(cell), start, end)
    return LnhashView(out)


@fail_clean(ValueError)
def exhash_cell(path:str, cell_id:str, cmds:list[tuple], sw:int=4, inplace:bool=True):
    """Apply exhash commands to the source of notebook cell ``cell_id`` in ipynb file at ``path``.

    Command syntax is the same as ``exhash(text, cmds, sw=sw)``; run ``doc(exhash)``
    for the full reference, and ``lnhashview_cell(path, cell_id)`` for addresses.
    ``cell_id`` may be an exact id or unique prefix.

    By default (``inplace=True``) write the edited source back (preserving the cell's
    original str-or-list-of-lines form; the notebook re-serializes in Jupyter's JSON
    layout) and return the diff string; if any command fails, write nothing. Pass
    ``inplace=False`` to preview instead: the EditResult is returned without touching the file.
    """
    nb, cell = _load_cell(path, cell_id)
    text = _cell_text(cell)
    res = exhash(text, cmds, sw=sw)
    if not inplace: return res
    new = '\n'.join(res['lines'])
    if text.endswith('\n') and new: new += '\n'
    cell['source'] = new.splitlines(keepends=True) if isinstance(cell['source'], list) else new
    Path(path).expanduser().write_text(json.dumps(nb, sort_keys=True, indent=1, ensure_ascii=False) + '\n')
    return res.format_diff()
