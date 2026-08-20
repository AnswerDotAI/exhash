"Hierarchical document outlines: `Section` trees with verified `addr.|lnhash` addresses, links by number, and `open_doc`."

import json, re
from pathlib import Path
from fastcore.basics import store_attr, humanize, PrettyString
from .exhash import md_scan as _md_scan, code_scan as _code_scan
from .exhash import lnhash as _lnhash, line_hash as _line_hash
from . import MAXLEN

__all__ = ['Link', 'Links', 'Section', 'Sections', 'open_doc']


def _preview(text, maxlen=MAXLEN):
    text = re.sub(r'\n(?:\s*\n)*', '¶', text.strip())
    return text if len(text) <= maxlen else text[:maxlen-1] + '…'


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
    "Sections listed as fixed-width `token title (count) [size] preview` rows; each row is the live node. Code rows have no title: the def line opens the preview"
    def __init__(self, items=None, counts=None, previews=None, width=MAXLEN):
        super().__init__(items or [])
        self.counts,self.previews,self.width = counts,previews,width
    def __getitem__(self, k):
        if isinstance(k, slice):
            sub = lambda xs: xs[k] if xs else xs
            return Sections(list.__getitem__(self, k), sub(self.counts), sub(self.previews), self.width)
        return list.__getitem__(self, k)
    def _row(self, n, c, p):
        parts = [n.token, n.title if n.show_title or p is not None else '', None if c is None else f'({c})', f'[{humanize(len(n.src))}]']
        pre = ' '.join(x for x in parts if x)
        budget = self.width - len(pre) - 1
        if budget < 2: return pre
        if p is None: p = n.preview(budget)
        if len(p) > budget: p = p[:budget-1] + '…'
        return f'{pre} {p}' if p else pre
    def __repr__(self):
        cs = self.counts or [None]*len(self)
        ps = self.previews or [None]*len(self)
        return '\n'.join(self._row(n,c,p) for n,c,p in zip(self, cs, ps))
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
            raise ValueError(f"Section addresses come from the listing - copy the token, e.g. '1.2.|12|a3f2|' (got {token!r})")
        node = self.root
        try:
            for k in addr[:-1].split('.') if addr != '.' else []: node = node[int(k)]
        except (KeyError, ValueError): raise ValueError(f'No section at {addr!r} - re-view and copy a fresh token') from None
        node._verify(addr, rest)
        return node

    def _verify(self, addr, rest):
        "Check an address payload against this section's current heading line; raise if stale"
        m = re.fullmatch(r'(\d+)\|([0-9a-f]{4})\|.*', rest)
        if not m: raise ValueError(f"Section addresses come from the listing - copy the token, e.g. '1.2.|12|a3f2|' (got {addr}|{rest})")
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
        "The deepest sections owning a line matching `pat`, in document order, with match counts and matching-line previews"
        try: r = re.compile(pat, re.IGNORECASE)
        except re.error: r = re.compile(re.escape(pat), re.IGNORECASE)
        subtree = [self, *self._walk()]
        hits = {}
        links = getattr(self.root, '_links', [])
        for i,line in enumerate(self.src.splitlines()):
            if not r.search(line): continue
            ln = self.start_line + i
            own = max((n for n in subtree if n.start_line <= ln <= n.end_line), key=lambda n: (n.start_line, len(n.addr)))
            node,c,first = hits.get(id(own), (own, 0, _numbered(line, [l for l in links if l.line == ln])))
            hits[id(own)] = (node, c+1, first)
        return Sections([n for n,_,_ in hits.values()], counts=[c for _,c,_ in hits.values()],
            previews=[_preview(f) for _,_,f in hits.values()])

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
        m = re.fullmatch(r'([\w-]+)\|([0-9a-f]{4})\|', rest)
        if not m: raise ValueError(f"Section addresses come from the listing - copy the token, e.g. '1.2.|ab12cd34|8f3a|' (got {addr}|{rest})")
        head = self.src.splitlines()[0] if self.src else ''
        if m[1] != self.cell_id or m[2] != _line_hash(head):
            raise ValueError(f'Stale address for section {addr}: expected {self.cell_id}|{_line_hash(head)}| - re-view and copy a fresh token')

    def _addressed(self, nums, lnhashs):
        "Stored cell sources as `cellid:lineno: ` rows, or `cellid:lineno|hash|` when `lnhashs`, ready for `cell_exhash`"
        res = []
        for cid,_,src in self.cells:
            for i,l in enumerate(src.splitlines()): res.append(f'{cid}:{_lnhash(i+1, l)}{l}' if lnhashs else f'{cid}:{i+1}: {l}')
        return '\n'.join(res)


def _parse_nb(path):
    "Build an `NbSection` tree from the ipynb file at `path`: md-cell headings over cells"
    from . import _cell_text
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
    "Open a document as a `Section` tree: a file (`fname`, or a `Path` in `src`; recorded for `refresh` and edits), a URL (fetched), or text"
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
