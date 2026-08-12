import pytest, re

from exhash import open_doc, lnhash, line_hash, file_exhash

SAMPLE = '''# Hooks

Shared hook behavior.

## Common input fields

Every hook receives `session_id`.

## Hooks

### SessionStart

Runs at thread start.

```python
# This is code, not a heading
```

### PostCompact

Runs after compaction.
'''


def test_outline_structure():
    d = open_doc(SAMPLE)
    assert [f'{n.addr} {n.title}' for n in d.paths()] == [
        '1 Hooks', '1.1 Common input fields', '1.2 Hooks',
        '1.2.1 SessionStart', '1.2.2 PostCompact']
    assert d[1][2][1].title == 'SessionStart'
    assert d.find('PostCompact').addr == '1.2.2'
    with pytest.raises(KeyError, match='found 2'): d.find('Hooks')
    sec = d.find('SessionStart')
    assert sec.text == '''### SessionStart

Runs at thread start.

```python
# This is code, not a heading
```'''
    leaf = open_doc('just prose\nno headings here')
    assert not len(leaf) and 'prose' in leaf.text


def test_addresses():
    d = open_doc(SAMPLE)
    sec = d.find('PostCompact')
    first = lnhash(sec.start_line, '### PostCompact')
    assert sec.token.startswith(f'{sec.addr}.|{first}')
    assert d.at(sec.token) is sec                      # full range token
    assert d.at(f'{sec.addr}.|{first}') is sec         # nav form: addr + first lnhash
    assert d.at(d.token) is d                          # the root is ordinary: addr `.`
    with pytest.raises(ValueError, match='listing'): d.at('1.2.2')    # bare addr refused, error teaches
    with pytest.raises(ValueError, match='listing'): d.at('1.2.2.')   # dot but no hash: same refusal
    bad = f'{sec.addr}.|{sec.start_line}|beef|'
    with pytest.raises(ValueError): d.at(bad)          # wrong hash fails loudly
    with pytest.raises(ValueError): d.at(f'9.9.|{first}')


def test_repr_rows():
    d = open_doc(SAMPLE)
    rows = repr(d).splitlines()
    assert rows[0].startswith('.|1|')                  # root row: ordinary token, whole-doc range
    r = re.compile(r"^(\d+(?:\.\d+)*\.\|\d+\|[0-9a-f]{4}\|,\d+\|[0-9a-f]{4}\|) (.+) \[\d+[.\w]*\](?: (.*))?$")
    sub = repr(d[1][2]).splitlines()
    m = r.match([x for x in sub if x.startswith('1.2.2.|')][0])
    assert m and m[2] == 'PostCompact'
    assert m[3].startswith('Runs after compaction')
    session = [x for x in sub if x.startswith('1.2.1.|')][0]
    assert '¶' in session                              # multi-line own body joined with pilcrow
    hooks = [x for x in rows if x.startswith('1.2.|')][0]
    assert hooks.endswith(']')                         # bodyless container section: no preview


def test_preview_truncation():
    body = '\n\n'.join(f'para {i} words here' for i in range(30))
    d = open_doc(f'# T\n\n{body}\n')
    row = repr(d).splitlines()[1]
    assert 'para 0 words here¶para 1' in row and row.endswith('…')
    assert len(row) == 180                             # fixed row width when the body suffices


def test_search():
    d = open_doc(SAMPLE)
    hits = d.search('runs')
    assert [n.title for n in hits] == ['SessionStart', 'PostCompact']
    assert hits.counts == [1, 1]
    assert list(d.search('(unbalanced')) == []         # invalid regex: no raise, literal, no hits
    d2 = open_doc(LINKS_MD)
    assert d2.search('(install').counts == [1]         # invalid regex falls back to literal matching


LINKS_MD = '''# Guide

Start with the [install](install.md): five minutes.

## Reference

- [API](https://example.com/api.md): every function
- ![logo](logo.png)

```
[fenced](ignored.md)
```
'''


def test_links():
    d = open_doc(LINKS_MD)
    ls = d.links()
    assert [(l.n, l.txt) for l in ls] == [(1, 'install'), (2, 'API')]
    assert ls[1].tail == 'every function'
    assert '[install][1]' in d.text and '[API][2]' in d.text
    row = [x for x in repr(d).splitlines() if x.startswith('1.|')][0]
    assert '[install][1]' in row and 'install.md' not in row   # previews render links numbered, never URLs
    assert repr(d).splitlines()[0].startswith('.|1|')         # root row: ordinary, addressable, range-editable
    assert '[install][1]' in d.search('five minutes').previews[0]
    assert '![logo](logo.png)' in d.text               # images untouched
    assert '[fenced](ignored.md)' in d.text            # fenced content untouched
    assert d.view() == d.src                           # view: source exactly as stored
    assert [l.n for l in d.links('api')] == [2]
    assert not hasattr(d, 'follow')                    # follow is gone: open(n).src replaces it


def test_file_backed(tmp_path):
    p = tmp_path/'doc.md'
    p.write_text(LINKS_MD)
    (tmp_path/'install.md').write_text('# Install\n\nRun the thing.\n')
    d = open_doc(p)
    assert d.path == p
    assert open_doc(fname=str(p)).path == p
    sec = d.find('Reference')
    v = sec.view(lnhashs=True).splitlines()
    assert v[0] == lnhash(sec.start_line, '## Reference') + '## Reference'
    inst = d.open(1)
    assert inst.title == '' and inst.find('Install').addr == '1'
    tok = d.find('Reference').token
    file_exhash(str(p), (tok.split('|', 1)[1], 'd'))   # delete the section via its own row range
    with pytest.raises(ValueError): d.refresh().at(tok)  # stale token fails loudly after refresh


PY_SRC = '''import os

def top(a, b):
    "doc"
    return a + b

@decorator
class Big:
    x = 1
    def method(self):
        def inner(): pass
        return 2

def tail(): pass
'''

RS_SRC = '''use std::fmt;

pub fn free() -> i32 { 1 }

pub struct Point { x: i32 }

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "p") }
}

mod inner {
    pub fn helper() {}
}
'''

JS_SRC = '''import x from "y";

export function greet(name) { return `hi ${name}`; }

const shout = (s) => s.toUpperCase();

class Runner {
  run() { return 1; }
}
'''

ZIG_SRC = '''const std = @import("std");

pub fn add(a: i32, b: i32) i32 {
    return a + b;
}

test "add works" {
    try std.testing.expect(add(1, 2) == 3);
}
'''


def test_code_outlines(tmp_path):
    p = tmp_path/'mod.py'
    p.write_text(PY_SRC)
    d = open_doc(p)
    assert [f'{n.addr} {n.title}' for n in d.paths()] == [
        '1 top', '2 Big', '2.1 method', '2.1.1 inner', '3 tail']
    big = d.find('Big')
    assert big.start_line == 7                       # decorated span starts at the decorator
    assert d.find('method').view(lnhashs=True).splitlines()[0].endswith('    def method(self):')
    assert d.at(d.find('inner').token).title == 'inner'
    with pytest.raises(ValueError, match='listing'): d.at('2.1.1')

    (tmp_path/'lib.rs').write_text(RS_SRC)
    r = open_doc(tmp_path/'lib.rs')
    assert [n.title for n in r.paths()] == ['free', 'Point', 'impl fmt::Display for Point', 'fmt', 'inner', 'helper']
    assert r.find('helper').addr == '4.1'

    (tmp_path/'app.js').write_text(JS_SRC)
    j = open_doc(tmp_path/'app.js')
    assert [n.title for n in j.paths()] == ['greet', 'shout', 'Runner', 'run']

    (tmp_path/'main.zig').write_text(ZIG_SRC)
    z = open_doc(tmp_path/'main.zig')
    assert [n.title for n in z.paths()] == ['add', 'test "add works"']


def _nb(cells):
    import json
    mk = lambda i,t,s: dict(cell_type=t, id=i, source=s, metadata={}, **({'outputs': [], 'execution_count': None} if t=='code' else {}))
    return json.dumps(dict(nbformat=4, nbformat_minor=5, metadata={}, cells=[mk(i,t,s) for i,t,s in cells]))


def test_nb_outline(tmp_path):
    p = tmp_path/'doc.ipynb'
    p.write_text(_nb([
        ('aaaa1111', 'markdown', '# Weather\n\nFetch and report.'),
        ('bbbb2222', 'code', 'import httpx'),
        ('cccc3333', 'markdown', '## Fetching'),
        ('dddd4444', 'code', 'def fetch(): pass'),
        ('eeee5555', 'markdown', '## Reporting\n\nOne line per field.'),
        ('ffff6666', 'code', 'def report(): pass'),
    ]))
    d = open_doc(p)
    assert [f'{n.addr} {n.title}' for n in d.paths()] == ['1 Weather', '1.1 Fetching', '1.2 Reporting']
    fetch = d.find('Fetching')
    assert fetch.cell_id == 'cccc3333'
    assert [c[0] for c in fetch.cells] == ['cccc3333', 'dddd4444']
    assert fetch.token == f'1.1.|cccc3333|{line_hash("## Fetching")}|'
    assert d.at(fetch.token) is fetch
    assert d.at(d.token) is d                          # root: first cell id, whole-notebook section
    with pytest.raises(ValueError, match='listing'): d.at('1.1')
    with pytest.raises(ValueError): d.at('1.1.|cccc3333|beef|')   # stale heading hash
    v = fetch.view(lnhashs=True).splitlines()
    assert v[0] == f'cccc3333:1|{line_hash("## Fetching")}|## Fetching'
    assert v[-1].startswith('dddd4444:1|')             # rows are cell-qualified, ready for cell_exhash
    assert 'def fetch' in fetch.src and d.find('Reporting').preview().startswith('One line per field.')


SWIFT_SRC = '''import Foundation

func greet(name: String) -> String {
    return "hi \\(name)"
}

class Runner {
    func run() -> Int { return 1 }
}

struct Point {
    var x: Int
}

protocol Speaker {
    func speak()
}
'''


def test_swift_outline(tmp_path):
    (tmp_path/'app.swift').write_text(SWIFT_SRC)
    d = open_doc(tmp_path/'app.swift')
    assert [f'{n.addr} {n.title}' for n in d.paths()] == [
        '1 greet', '2 Runner', '2.1 run', '3 Point', '4 Speaker', '4.1 speak']


@pytest.mark.slow
def test_live_llms_txt():
    toc = open_doc('https://code.claude.com/docs/llms.txt')
    l = toc.links('subagent')[0]
    page = toc.open(l.n)
    assert len(page.paths()) > 3 and page.search('subagent')
