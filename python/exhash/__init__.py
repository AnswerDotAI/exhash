from .exhash import line_hash as _line_hash, lnhash as _lnhash, lnhashview as _lnhashview, exhash as _exhash

def line_hash(line:str) -> str:
    'Return a 4-char lowercase hex hash for a single line of text.'
    return _line_hash(line)


def lnhash(lineno:int, line:str) -> str:
    'Return an lnhash address ``lineno|hash|`` for ``line`` at 1-based ``lineno``.'
    return _lnhash(lineno, line)


def lnhashview(text:str) -> list[str]:
    'Return lines formatted as ``lineno|hash|  content`` for each line in ``text``.'
    return _lnhashview(text)


def exhash_result(results:list[dict]) -> str:
    'Format modified lines from exhash result dicts in lnhash view format.'
    if not isinstance(results, list): raise TypeError("results must be a list[dict]")
    out = []
    for r in results:
        if not isinstance(r, dict): raise TypeError("results must be a list[dict]")
        lines, hashes, modified = r.get("lines"), r.get("hashes"), r.get("modified")
        if not isinstance(lines, list) or not isinstance(hashes, list) or not isinstance(modified, list):
            raise TypeError("each result must include list fields: lines, hashes, modified")
        out += [f"{hashes[i-1]}  {lines[i-1]}" for i in modified if isinstance(i, int) and 0 < i <= len(hashes)]
    return '\n'.join(out)


def exhash(text:str, cmds:list[str]) -> dict:
    """Verified line-addressed editor. Apply commands to `text`, return a result dict.

    Commands use lnhash addresses: ``lineno|hash|cmd`` where hash is a 4-char
    hex content hash. Use ``lnhashview(text)`` or ``lnhash(lineno, line)`` to
    get addresses.
    Each command's hashes are verified against current text immediately before
    that command executes.

    Addressing:
      Single:   ``12|a3f2|cmd``
      Range:    ``12|a3f2|,15|b1c3|cmd``
      Special:  ``0|0000|`` targets before line 1 (only with a or i)

    Commands:
      s/pat/rep/[flags]  Substitute (regex). Flags: g=all, i=case-insensitive
      d                  Delete line(s)
      a                  Append text after line
      i                  Insert text before line
      c                  Change/replace line(s)
      j                  Join with next line; with range, joins all
      m dest             Move line(s) after dest address
      t dest             Copy line(s) after dest address
      >[n]               Indent n levels (default 1, 4 spaces each)
      <[n]               Dedent n levels (default 1)
      sort               Sort lines alphabetically
      p                  Print (include in output without changing)
      g/pat/cmd          Global: run cmd on matching lines
      g!/pat/cmd         Inverted global (also v/pat/cmd)

    For a/i/c, remaining lines in the command string are the text block
    (no '.' terminator needed, unlike the CLI).

    Returns a dict with:
      lines     list of output lines
      hashes    lnhash for each output line
      modified  1-based line numbers of modified/added lines
      deleted   1-based line numbers of removed lines (in original)

    `cmds` is a required iterable of command strings. For `a`/`i`/`c`, include
    the text block in the same command string after a newline.

    Examples::

      from exhash import exhash, lnhash, lnhashview
      text = "foo\\nbar\\n"
      addr = lnhash(1, "foo")           # "1|a1b2|"
      res = exhash(text, [f"{addr}s/foo/baz/"])
      print(res["lines"])                # ["baz", "bar"]
      "\\n".join(res["lines"])           # "baz\\nbar"
      res = exhash(text, [f"{addr}a\\nnew line 1\\nnew line 2"])
    """
    r = _exhash(text, *cmds)
    return dict(lines=r.lines, hashes=r.hashes, modified=r.modified, deleted=r.deleted)
