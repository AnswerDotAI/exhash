import json, pytest
from exhash import lnhash, lnhashview_cell, lnhashview_cells, exhash_cell

def mk_nb(path, cells):
    "Write a minimal notebook; `cells` is a list of (id, source) with source str or list"
    cells = [dict(cell_type='code', id=i, metadata={}, execution_count=None, outputs=[], source=s) for i,s in cells]
    nb = dict(nbformat=4, nbformat_minor=5, metadata={}, cells=cells)
    path.write_text(json.dumps(nb))
    return nb

def test_lnhashview_cell(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', ['def f():\n', '    return 1']), ('bbbb2222', 'x=1')])
    lines = lnhashview_cell(p, 'aaaa1111')
    assert len(lines) == 2
    assert lines[0].startswith(lnhash(1, 'def f():'))
    assert lines[1].endswith('|    return 1')
    assert str(lines) == chr(10).join(lines)


def test_lnhashview_cells(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', ['def f():\n', '    return 1']), ('bbbb2222', 'x=1')])
    lines = lnhashview_cells(p, 'aaaa', 'bbbb')
    assert lines[0] == '# cell aaaa1111'
    assert lines[1].startswith(lnhash(1, 'def f():'))
    assert lines[3] == '# cell bbbb2222'
    assert lines[4].startswith(lnhash(1, 'x=1'))
    assert str(lines) == chr(10).join(lines)

def test_lnhashview_cell_prefix_and_errors(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1'), ('aabb2222', 'y=2')])
    assert lnhashview_cell(p, 'aabb')[0].endswith('|y=2')
    with pytest.raises(KeyError): lnhashview_cell(p, 'aa')      # ambiguous prefix
    with pytest.raises(KeyError): lnhashview_cell(p, 'zzzz')    # no such cell

def test_exhash_cell_inplace_list_source(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', ['def f():\n', '    return 1'])])
    diff = exhash_cell(p, 'aaaa1111', [(lnhash(2, "    return 1"), "s", "1", "2")], inplace=True)
    assert repr(diff) == str(diff)
    assert '+' in diff and 'return 2' in diff
    src = json.loads(p.read_text())['cells'][0]['source']
    assert src == ['def f():\n', '    return 2']              # list form preserved

def test_exhash_cell_inplace_str_source(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1\ny=2')])
    exhash_cell(p, 'aaaa1111', [(lnhash(1, "x=1"), "d")], inplace=True)
    src = json.loads(p.read_text())['cells'][0]['source']
    assert src == 'y=2'                                       # str form preserved

def test_exhash_cell_preview_and_stale(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1')])
    res = exhash_cell(p, 'aaaa1111', [(lnhash(1, "x=1"), "s", "1", "9")], inplace=False)   # preview
    assert res['lines'] == ['x=9']
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=1'   # untouched
    with pytest.raises(ValueError): exhash_cell(p, 'aaaa1111', [("1|dead|", "s", "x", "y")], inplace=True)
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=1'   # stale hash leaves file alone


def test_exhash_cell_writes_by_default(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1')])
    diff = exhash_cell(p, 'aaaa1111', [(lnhash(1, "x=1"), "s", "1", "9")])
    assert isinstance(diff, str) and 'x=9' in diff
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=9'   # written by default
