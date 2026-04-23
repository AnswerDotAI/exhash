from pathlib import Path
from .exhash import line_hash as _line_hash, lnhash as _lnhash, lnhashview as _lnhashview, exhash as _exhash

def line_hash(line:str) -> str:
    'Return a 4-char lowercase hex hash for a single line of text.'
    return _line_hash(line)


def lnhash(lineno:int, line:str) -> str:
    'Return an lnhash address ``lineno|hash|`` for ``line`` at 1-based ``lineno``.'
    return _lnhash(lineno, line)


def lnhashview(text:str, start:int=None, end:int=None) -> list[str]:
    'Return lines formatted as ``lineno|hash|  content``. Optional 1-based ``start``/``end`` filter the range; ``end`` past EOF is clamped.'
    return _lnhashview(text, start, end)


def lnhashview_file(path:str, start:int=None, end:int=None) -> list[str]:
    'Return lines formatted as ``lineno|hash|  content`` for file at ``path``. Optional 1-based ``start``/``end`` filter the range; ``end`` past EOF is clamped.'
    return _lnhashview(Path(path).read_text(), start, end)


def exhash(text:str, cmds:list[str], sw:int=4):
    """Verified line-addressed editor. Apply commands to `text`, return an EditResult.

    Commands primarily use lnhash addresses: ``lineno|hash|cmd`` where hash is
    a 4-char hex content hash. Use ``lnhashview(text)`` or
    ``lnhash(lineno, line)`` to get hashed addresses.
    Each command's hashes are verified against current text immediately before
    that command executes.

    Addressing:
      Single:   ``12|a3f2|cmd``
      Range:    ``12|a3f2|,15|b1c3|cmd``
      Last:     ``$cmd`` (last line)
      Whole:    ``%cmd`` (whole file, same as ``1,$``)
      Special:  ``0|0000|`` targets before line 1 (only with a or i)

    Commands:
      s/pat/rep/[flags]  Substitute using Rust regex syntax.
                         Replacement supports $1, $0, ${name}. Flags: g=all, i=case-insensitive
                         Any non-alphanumeric delimiter works: s@pat@rep@, s|pat|rep|g
                         Literal newlines in pat/rep are supported (joins/splits lines)
      y/src/dst/         Transliterate chars in-place (also supports custom delimiters;
                         source and destination lengths must match)
      d                  Delete line(s)
      a                  Append text after line
      i                  Insert text before line
      c                  Change/replace line(s)
      j                  Join with next line; with range, joins all
      m dest             Move line(s) after dest address
      t dest             Copy line(s) after dest address
      >[n]               Indent n levels (default 1, `sw` spaces each)
      <[n]               Dedent n levels (default 1, `sw` spaces each)
      sort               Sort lines alphabetically
      p                  Print (include in output without changing)
      g/pat/cmd          Global: run cmd on matching lines (custom delimiters ok: g@pat@cmd)
      g!/pat/cmd         Inverted global (also v/pat/cmd; custom delimiters ok)

    `sw` controls shift width for `<` and `>` and defaults to 4.

    For multiline a/i/c commands, include the inserted text in the same command
    string using newline characters, e.g. ``["12|abcd|c\nnew line 1\nnew line 2"]``.
    Do not use ``.`` terminators, and do not split the text block into separate
    ``cmds`` entries. If you include a final ``.`` line, it is inserted literally
    and exhash emits a warning.

    Returns an EditResult with attributes (also accessible as dict keys):
      lines     list of output lines
      hashes    lnhash for each output line
      modified  1-based line numbers of modified/added lines
      deleted   1-based line numbers of removed lines (in original)
      origins   for each output line, the 1-based original line number (None if inserted)

    Call ``res.format_diff(context=1)`` for a unified-diff-style summary.

    Examples::

      from exhash import exhash, lnhash, lnhashview
      text = "foo\\nbar\\n"
      addr = lnhash(1, "foo")           # "1|a1b2|"
      res = exhash(text, [f"{addr}s/foo/baz/"])
      print(res["lines"])                # ["baz", "bar"]
      print(res.format_diff())           # unified-diff-style summary
    """
    return _exhash(text, *cmds, sw=sw)


def exhash_file(path:str, cmds:list[str], sw:int=4, inplace:bool=False):
    'Like ``exhash`` but reads from file at ``path``. For multiline a/i/c commands, include the inserted text in the same command string using newline characters, e.g. ``["12|abcd|c\\nnew line 1\\nnew line 2"]``. Do not use ``.`` terminators, and do not split the text block into separate ``cmds`` entries. If the file is missing and the commands are valid on empty input (for example ``0|0000|a``), it is treated as empty and created on inplace write. If ``inplace``, writes back and returns diff string.'
    p = Path(path)
    try: text = p.read_text()
    except FileNotFoundError as e:
        try: r = exhash("", cmds, sw=sw)
        except Exception: raise e
    else: r = _exhash(text, *cmds, sw=sw)
    if inplace:
        p.write_text('\n'.join(r['lines']) + '\n' if r['lines'] else '')
        return r.format_diff()
    return r
