from .exhash import line_hash, lnhash, lnhashview, exhash as _exhash

class EditResult:
    def __init__(self, r):
        self.lines, self.hashes, self.modified, self.deleted = r.lines, r.hashes, r.modified, r.deleted
    def text(self): return '\n'.join(self.lines)
    def view(self): return '\n'.join(f"{h}  {l}" for h, l in zip(self.hashes, self.lines))
    def __repr__(self):
        return '\n'.join(f"{self.hashes[i-1]}  {self.lines[i-1]}" for i in self.modified if i-1 < len(self.hashes))

def exhash(text, *cmds):
    """Verified line-addressed editor. Apply commands to `text`, return `EditResult`.

    Commands use lnhash addresses: ``lineno|hash|cmd`` where hash is a 4-char
    hex content hash. Use ``lnhashview(text)`` or ``lnhash(lineno, line)`` to
    get addresses.

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

    Returns EditResult with:
      .lines     list of output lines
      .hashes    lnhash for each output line
      .modified  1-based line numbers of modified/added lines
      .deleted   1-based line numbers of removed lines (in original)
      .text()    joined output
      .view()    output in lnhash format
      repr()     shows only modified lines in lnhash format

    Examples::

      from exhash import exhash, lnhash, lnhashview
      text = "foo\\nbar\\n"
      addr = lnhash(1, "foo")           # "1|a1b2|"
      res = exhash(text, f"{addr}s/foo/baz/")
      print(res)                         # "1|c2da|  baz" (modified lines only)
      res.text()                         # "baz\\nbar"
      res = exhash(text, f"{addr}a\\nnew line 1\\nnew line 2")
    """
    return EditResult(_exhash(text, *cmds))
