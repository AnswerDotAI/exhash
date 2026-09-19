"Hierarchical document outlines: `Section` trees with verified `addr.|lnhash` addresses, links by number, and `open_doc`."

import json, re
from pathlib import Path
from fastcore.basics import store_attr, humanize, PrettyString
from .exhash import md_scan as _md_scan, code_scan as _code_scan
from .exhash import lnhash as _lnhash, line_hash as _line_hash
from . import MAXLEN

__all__ = ['Link', 'Links', 'Section', 'Sections', 'SearchHit', 'SearchHits', 'open_doc']


def _preview(text, maxlen=MAXLEN):
    text = re.sub(r'\n(?:\s*\n)*', '¶', text.strip())
    return text if len(text) <= maxlen else text[:maxlen-1] + '…'


def _row(prefix, preview, width=MAXLEN):
    budget = width - len(prefix) - 1
    if budget < 2: return prefix
    if len(preview) > budget: preview = preview[:budget-1] + '…'
    return f'{prefix} {preview}' if preview else prefix


class Link:
    "One inline link, numbered in document reading order"
    def __init__(self,
        n,    # 1-based number, in document reading order
        txt,  # The link text
        url,  # The link target (never shown by `__repr__`; the number stands for it)
        tail, # The rest of the link's line, as context - an llms.txt entry's description
        line, # Document-absolute 1-based line number
    ): store_attr()
    def __repr__(self): return f'[{self.n}] {self.txt}' + (f': {self.tail}' if self.tail else '')


class Links(list):
    "A list of `Link` rows, displayed one per line"
    def __repr__(self): return '\n'.join(map(repr, self))
    def _repr_pretty_(self, p, cycle): p.text('...' if cycle else repr(self))


class Sections(list):
    "Sections listed as fixed-width `token title [size] preview` rows; each row is the live node. Code rows show the def line instead of a title"
    def __init__(self, items=None, width=MAXLEN):
        super().__init__(items or [])
        self.width = width
    def __getitem__(self, k):
        if isinstance(k, slice): return Sections(list.__getitem__(self, k), self.width)
        return list.__getitem__(self, k)
    def _row(self, n):
        parts = [n.token, n.title if n.show_title else '', f'[{humanize(len(n.src))}]']
        pre = ' '.join(x for x in parts if x)
        return _row(pre, n.preview(self.width), self.width)
    def __repr__(self): return '\n'.join(self._row(n) for n in self)
    def _repr_pretty_(self, p, cycle): p.text('...' if cycle else repr(self))


class SearchHit:
    "One matching source line: its containing `section`, verified line `address`, and ¶-joined continuation `preview`"
    def __init__(self, section, address, preview): store_attr()
    def __repr__(self): return _row(f'{self.section.token} {self.address}', self.preview)


class SearchHits(list):
    "Search hits in source order, displayed as `section-token line-address preview` rows within 180 characters"
    def __getitem__(self, k):
        res = list.__getitem__(self, k)
        return SearchHits(res) if isinstance(k, slice) else res
    def __repr__(self): return '\n'.join(map(repr, self))
    def _repr_pretty_(self, p, cycle): p.text('...' if cycle else repr(self))


class Section(dict):
    "One document section: numbered child sections, source span, and a verified address"
    title,addr,show_title = '','',True
    def __init__(self, src='', start_line=1):
        super().__init__()
        self.src,self.start_line = src,start_line

    @property
    def end_line(self):
        "Document-absolute 1-based line number of this section's last line."
        return self.start_line + len(self.src.splitlines()) - 1

    @property
    def token(self):
        "The verified address `addr.|start_lnhash,end_lnhash`, as shown in listings; the root's is `.|...`"
        lines = self.src.splitlines() or ['']
        return f'{self.addr}.|{_lnhash(self.start_line, lines[0])},{_lnhash(self.end_line, lines[-1])}'

    @property
    def text(self):
        "Section Markdown, with each parsed inline link rendered as `[text][n]`."
        links = getattr(getattr(self, 'root', None), '_links', None)
        if not links: return self.src
        res = []
        for i,line in enumerate(self.src.splitlines()):
            here = [l for l in links if l.line == self.start_line+i]
            if here: line = _numbered(line, here)
            res.append(line)
        return '\n'.join(res)

    def preview(self, maxlen=MAXLEN):
        "This section's own body before any subsection, one ¶-joined truncated line; the heading line is omitted where the listing row already shows the title"
        head = 1 if self.title and self.show_title else 0
        end = min((c.start_line for c in self.values()), default=self.end_line+1) - self.start_line
        return _preview('\n'.join(self.text.splitlines()[head:end]), maxlen)

    def _walk(self):
        for node in self.values():
            yield node
            yield from node._walk()

    def paths(self,
        depth=None, # Deepest address depth to include; None for all
    ):
        "Outline of every section as address-token rows; each row is the live node"
        ns = self._walk()
        if depth: ns = (n for n in ns if n.addr.count('.') < depth)
        return Sections(ns)

    def at(self, token):
        "The section at a verified address copied from a listing: the `addr.|...` form a view shows (the root's addr is `.`)"
        addr,_,rest = token.partition('|')
        if not addr.endswith('.') or not rest:
            raise ValueError(f"Section addresses come from the listing - copy the token, e.g. '1.2.|12|Py|' (got {token!r})")
        node = self.root
        try:
            for k in addr[:-1].split('.') if addr != '.' else []: node = node[int(k)]
        except (KeyError, ValueError): raise ValueError(f'No section at {addr!r} - re-view and copy a fresh token') from None
        node._verify(addr, rest)
        return node

    def _verify(self, addr, rest):
        "Check an address payload against this section's current heading line; raise if stale"
        m = re.fullmatch(r'(\d+)\|([A-Za-z0-9_-]{2})\|.*', rest)
        if not m: raise ValueError(f"Section addresses come from the listing - copy the token, e.g. '1.2.|12|Py|' (got {addr}|{rest})")
        lineno,h = int(m[1]),m[2]
        head = self.src.splitlines()[0] if self.src else ''
        if lineno != self.start_line or h != _line_hash(head):
            raise ValueError(f'Stale address for section {addr}: expected {_lnhash(self.start_line, head)}, got {lineno}|{h}| - re-view and copy a fresh token')

    def find(self, title):
        "The unique section titled `title`; raises if absent or ambiguous"
        ms = [n for n in self._walk() if n.title == title]
        if len(ms) != 1: raise KeyError(f'Expected one heading named {title!r}, found {len(ms)}: {[n.addr for n in ms]}')
        return ms[0]

    def search(self,
        pat, # Case-insensitive regex, matched line by line; an invalid regex matches literally
    ):
        """Return `SearchHits`, one `SearchHit` per matching source line in source order, not grouped by section.

        Each hit exposes `section` (the containing live section), `address` (the matching line's verified address),
        and `preview`. Rows are `section-token line-address preview`, capped at 180 characters. The preview starts
        at the matching line and continues across newlines as ¶, up to the section's end; links display as `[text][n]`.
        Matching uses raw source, and multiple regex matches on one line still produce only one hit.

        Section tokens repeat for hits in the same section: copy the token to `view()` to read that section,
        or use the line address to edit the hit. Notebook line addresses are `cellid:lineno|hash|`.
        Slicing the results preserves their display format.
        """
        try: r = re.compile(pat, re.IGNORECASE)
        except re.error: r = re.compile(re.escape(pat), re.IGNORECASE)
        subtree = [self, *self._walk()]
        hits = SearchHits()
        text = self.text.splitlines()
        for i,line in enumerate(self.src.splitlines()):
            if not r.search(line): continue
            ln = self.start_line + i
            own = max((n for n in subtree if n.start_line <= ln <= n.end_line), key=lambda n: (n.start_line, len(n.addr)))
            preview = _preview('\n'.join(text[i:own.end_line-self.start_line+1]))
            hits.append(SearchHit(own, own._line_address(ln), preview))
        return hits

    def _line_address(self, lineno):
        return _lnhash(lineno, self.src.splitlines()[lineno-self.start_line])

    def links(self,
        pat='', # Case-insensitive regex matched against each link's text, target, and tail
    ):
        "This section's `Link` rows, numbered document-wide, filtered by `pat`"
        ls = [l for l in self.root._links if self.start_line <= l.line <= self.end_line]
        if pat:
            r = re.compile(pat, re.IGNORECASE)
            ls = [l for l in ls if r.search(l.txt) or r.search(l.url) or r.search(l.tail)]
        return Links(ls)

    def open(self,
        n, # A link number, as shown by `links`
    ):
        "The document behind link `n`: fetched or read, parsed, with `base` recorded"
        ls = self.root._links
        if not 1 <= n <= len(ls): raise IndexError(f'Link {n} is not in 1..{len(ls)}')
        url = ls[n-1].url
        base = getattr(self.root, 'base', None)
        if not re.match(r'https?://', url):
            if base is None: raise ValueError(f'{url!r} is relative and this document has no `base`')
            if isinstance(base, Path): return open_doc(base.parent/url)
            from urllib.parse import urljoin
            url = urljoin(str(base), url)
        return open_doc(url)

    def refresh(self):
        "Fresh tree re-read from `path` (file-backed roots only)"
        return open_doc(self.root.path)

    def view(self, *tokens, # Section address tokens copied from a listing (see `at`); none: this section
        nums=False, # Prefix stored lines with document-absolute line numbers, `lineno: ` (`cellid:lineno: ` in notebooks)
        lnhashs=False # Prefix `lineno|hash|` exhash addresses instead (`cellid:lineno|hash|` in notebooks); wins over `nums`
    ):
        "Rendered text, links as `[text][n]`; `nums`/`lnhashs` instead show stored lines with edit-ready addresses; `tokens` views those sections, each under a `# token` header when more than one"
        if tokens: return SectionViews([self.at(t) for t in tokens], tokens, nums, lnhashs)
        if nums or lnhashs: return PrettyString(self._addressed(nums, lnhashs))
        return PrettyString(self.text)

    def _addressed(self, nums, lnhashs):
        "Stored lines prefixed with `lineno: ` addresses, or `lineno|hash|` when `lnhashs`"
        lines = self.src.splitlines()
        if lnhashs: return '\n'.join(_lnhash(self.start_line+i, l)+l for i,l in enumerate(lines))
        return '\n'.join(f'{self.start_line+i}: {l}' for i,l in enumerate(lines))

    def __repr__(self):
        "Own row, then up to two heading levels below, as an orientation view"
        seg = lambda a: a.count('.')+1 if a else 0
        base = seg(self.addr)
        rows = [n for n in self._walk() if 1 <= seg(n.addr)-base <= 2]
        return repr(Sections([self, *rows]))
    def _repr_pretty_(self, p, cycle): p.text('...' if cycle else repr(self))


class CodeSection(Section):
    "A code definition: its first line is the signature, so previews keep it and listing rows show it in place of a title"
    show_title = False


class SectionViews(list):
    "Live sections from `view(*tokens)`, displayed as each section's view, under `# token` headers when more than one"
    def __init__(self, secs, tokens, nums=False, lnhashs=False):
        super().__init__(secs)
        self.tokens,self.nums,self.lnhashs = tokens,nums,lnhashs
    def __repr__(self):
        bodies = [s.view(nums=self.nums, lnhashs=self.lnhashs) for s in self]
        if len(self) == 1: return bodies[0]
        return '\n\n'.join(f'# {t}\n{b}' for t,b in zip(self.tokens, bodies))
    def _repr_pretty_(self, p, cycle): p.text('...' if cycle else repr(self))

_LINK_RE = re.compile(r'(?<!\!)\[([^\]]*)\]\(([^)\s]+)\)')


def _numbered(line, links):
    "Render `line`'s inline links as `[text][n]` using its `Link` rows, in order"
    it = iter(links)
    return _LINK_RE.sub(lambda m: f'[{m[1]}][{next(it).n}]', line)

_LANGS = dict(py='python', js='javascript', mjs='javascript', cjs='javascript', ts='typescript', tsx='tsx', rs='rust', zig='zig', swift='swift')

def _build(text, rows, links=(), base=None, cls=None):
    "Build a `Section` tree from preorder `(level, title, start_line, end_line)` rows"
    cls = cls or Section
    lines = text.splitlines()
    root = cls(text)
    root.root,root.base,root.path = root,base,None
    root._links = [Link(*l) for l in links]
    stack,levels = [root],[0]
    for level,title,start,end in rows:
        while len(stack) > 1 and levels[-1] >= level:
            stack.pop()
            levels.pop()
        parent,k = stack[-1],len(stack[-1])+1
        parent[k] = node = cls('\n'.join(lines[start-1:end]), start)
        node.root,node.title = root,title
        node.addr = f'{parent.addr}.{k}' if parent.addr else str(k)
        stack.append(node)
        levels.append(level)
    return root

def _parse_md(text, rm_fenced=True, base=None):
    "Build a `Section` tree from Markdown using the Rust scan"
    headings,links = _md_scan(text, rm_fenced)
    return _build(text, headings, links, base)


class NbSection(Section):
    "A notebook section: md-cell headings over cells; `cells` holds `(cell_id, cell_type, source)` rows"
    cell_id,cells = '',()

    @property
    def token(self):
        "The verified address `addr.|headingcellid|headinghash|`, as shown in listings"
        lines = self.src.splitlines() or ['']
        return f'{self.addr}.|{self.cell_id}|{_line_hash(lines[0])}|'

    def _verify(self, addr, rest):
        m = re.fullmatch(r'([\w-]+)\|([A-Za-z0-9_-]{2})\|', rest)
        if not m: raise ValueError(f"Section addresses come from the listing - copy the token, e.g. '1.2.|ab12cd34|86|' (got {addr}|{rest})")
        head = self.src.splitlines()[0] if self.src else ''
        if m[1] != self.cell_id or m[2] != _line_hash(head):
            raise ValueError(f'Stale address for section {addr}: expected {self.cell_id}|{_line_hash(head)}| - re-view and copy a fresh token')

    def _addressed(self, nums, lnhashs):
        "Stored cell sources as `cellid:lineno: ` rows, or `cellid:lineno|hash|` when `lnhashs`, ready for `cell_exhash`"
        res = []
        for cid,_,src in self.cells:
            for i,l in enumerate(src.splitlines()): res.append(f'{cid}:{_lnhash(i+1, l)}{l}' if lnhashs else f'{cid}:{i+1}: {l}')
        return '\n'.join(res)

    def _line_address(self, lineno):
        for cid,_,src in self.root.cells:
            lines = src.splitlines() or ['']
            if lineno <= len(lines): return f'{cid}:{_lnhash(lineno, lines[lineno-1])}'
            lineno -= len(lines)


def _parse_nb(path):
    "Build an `NbSection` tree from the ipynb file at `path`: md-cell headings over cells"
    def _cell_text(c): return c["source"] if isinstance(c["source"], str) else "".join(c["source"])
    path = Path(path).expanduser()
    nb = json.loads(path.read_text())
    cells = [(c.get('id',''), c['cell_type'], _cell_text(c).rstrip('\n')) for c in nb['cells']]
    offs,off = [],0
    for _,_,t in cells:
        offs.append(off)
        off += len(t.splitlines()) or 1
    total = off
    heads,links = [],[]
    for (cid,ctype,t),o in zip(cells, offs):
        if ctype != 'markdown': continue
        hs,ls = _md_scan(t, True)
        heads += [(lv, title, o+s) for lv,title,s,_ in hs]
        links += [(len(links)+1+i, txt, url, tail, o+line) for i,(_,txt,url,tail,line) in enumerate(ls)]
    vtext = '\n'.join('\n'.join(t.splitlines() or ['']) for _,_,t in cells)
    vlines = vtext.split('\n') if cells else []
    rows = []
    for i,(lv,title,start) in enumerate(heads):
        end = next((s for l,_,s in heads[i+1:] if l <= lv), total + 1) - 1
        while end > start and not vlines[end-1].strip(): end -= 1
        rows.append((lv, title, start, end))
    root = _build(vtext, rows, links, base=path, cls=NbSection)
    root.path = path
    for node in [root, *root._walk()]:
        node.cells = [c for c,o in zip(cells, offs) if o < node.end_line and o + (len(c[2].splitlines()) or 1) >= node.start_line]
        node.cell_id = next((c[0] for c,o in zip(cells, offs) if o < node.start_line <= o + (len(c[2].splitlines()) or 1)), cells[0][0] if cells else '')
    return root


def open_doc(
    src:str|Path=None, # `Path`: a file to read (expands `~`); `https?://` str: a URL to fetch; any other str: the text itself
    rm_fenced=True, # Ignore headings inside fenced code blocks?
    fname:str=None, # File name to open (expands `~`), as an alternative to passing a `Path` as `src`
):
    """Open a file, URL, or text as a `Section` tree for hierarchical reading and verified edit addresses.

    Pass a file with `fname=` or a `Path` as `src` (retained for `refresh()` and edits), a URL as an `https?://` string, or held text as any other string. Trees use Markdown headings, tree-sitter definitions for code files (py/js/ts/tsx/rs/zig/swift), or notebook heading cells.

    Display the tree bare to see its outline. Listing rows are `token title [size] preview`; code previews start with the definition/signature instead of a title. Newlines display as ¶ and links as `[text][n]`.

    Tokens, the idiomatic usage, combine dotted section addresses (root `.`, trailing dot otherwise) and boundary hashes: `1.2.|12|Py|,45|HD|`. `at()` accepts copied listing tokens, not bare dotted addresses, and verifies the first hash; the boundary pair is an edit-ready range. Live navigation uses `d[1][6]`, `find(title)`, `search(pat)`, and `paths(depth)`. Use `links(pat)` to list links and `open(n)` to open one by number. Opened links record `base`; non-Markdown targets become leaves with text in `.src`.

    `view()` renders links as `[text][n]`; `.src` is raw text. `view(*tokens)` reads multiple sections under `# token` headers; `nums=True` or `lnhashs=True` shows stored lines with line numbers or hash addresses. Notebook tokens contain heading cell IDs (`1.2.|ab12cd34|86|`); hashed views use `cellid:lineno|hash|` for cell edits.

    For llms.txt: `toc = open_doc(url)` → `toc.links(topic)` → `page = toc.open(n)` → display `page`. Read `page.view()` when small; otherwise use `page.search(topic)` and `page.view(*tokens)`.
    """
    if fname: src = Path(fname)
    if isinstance(src, Path):
        path = src.expanduser()
        if path.suffix == '.ipynb': return _parse_nb(path)
        lang = _LANGS.get(path.suffix.lstrip('.'))
        text = path.read_text()
        res = _build(text, _code_scan(text, lang), cls=CodeSection) if lang else _parse_md(text, rm_fenced, base=path)
        res.path = path
        return res
    if isinstance(src, str) and re.match(r'https?://', src):
        import httpx
        r = httpx.get(src, follow_redirects=True)
        r.raise_for_status()
        return _parse_md(r.text, rm_fenced, base=src)
    return _parse_md(src, rm_fenced)
