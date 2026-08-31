import json, subprocess, pytest
from exhash import lnhash, lnhashview_cell, lnhashview_cells, cell_exhash, file_exhash
from exhash._cli import exhash_cell_main

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


def test_lnhashview_cell_cli_accepts_comma_separated_ids(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1'), ('bbbb2222', 'y=2')])
    single = subprocess.run(['lnhashview-cell', str(p), 'aaaa'], text=True, capture_output=True)
    multiple = subprocess.run(['lnhashview-cell', str(p), 'aaaa,bbbb'], text=True, capture_output=True)
    assert single.returncode == multiple.returncode == 0
    assert single.stdout.startswith(f'{lnhash(1, "x=1")}x=1') and '# cell' not in single.stdout
    assert '# cell aaaa1111' in multiple.stdout and '# cell bbbb2222' in multiple.stdout

def test_lnhashview_cell_prefix_and_errors(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1'), ('aabb2222', 'y=2')])
    assert lnhashview_cell(p, 'aabb')[0].endswith('|y=2')
    with pytest.raises(KeyError): lnhashview_cell(p, 'aa')      # ambiguous prefix
    with pytest.raises(KeyError): lnhashview_cell(p, 'zzzz')    # no such cell

def test_cell_exhash_inplace_list_source(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', ['def f():\n', '    return 1'])])
    diff = cell_exhash(p, 'aaaa1111', (lnhash(2, "    return 1"), "s", "1", "2"), inplace=True)
    assert repr(diff) == str(diff)
    assert '+' in diff and 'return 2' in diff
    src = json.loads(p.read_text())['cells'][0]['source']
    assert src == ['def f():\n', '    return 2']              # list form preserved

def test_cell_exhash_inplace_str_source(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1\ny=2')])
    cell_exhash(p, 'aaaa1111', (lnhash(1, "x=1"), "d"), inplace=True)
    src = json.loads(p.read_text())['cells'][0]['source']
    assert src == 'y=2'                                       # str form preserved

def test_cell_exhash_preview_and_stale(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1')])
    res = cell_exhash(p, 'aaaa1111', (lnhash(1, "x=1"), "s", "1", "9"), inplace=False)   # preview
    assert res['lines'] == ['x=9']
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=1'   # untouched
    with pytest.raises(ValueError): cell_exhash(p, 'aaaa1111', ("1|dead|", "s", "x", "y"), inplace=True)
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=1'   # stale hash leaves file alone


def test_cell_exhash_writes_by_default(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1')])
    diff = cell_exhash(p, 'aaaa1111', (lnhash(1, "x=1"), "s", "1", "9"))
    assert isinstance(diff, str) and 'x=9' in diff
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=9'   # written by default


def test_exhash_cell_cli_dry_run_then_write(tmp_path, capsys):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x=1')])
    cmd = f"{lnhash(1, 'x=1')}s/1/2/"
    exhash_cell_main(['--dry-run', str(p), 'aaaa', cmd])
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=1'
    exhash_cell_main([str(p), 'aaaa', cmd])
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x=2'
    assert 'x=2' in capsys.readouterr().out


def test_exhash_cell_cli_help(capsys):
    exhash_cell_main(['--help'])
    help_ = capsys.readouterr().err
    assert "'3|beef|s/old/new/'" in help_
    assert "'3|beef|c'" in help_
    assert 'through EOF' in help_


def test_cell_exhash_stacks_call_start_addresses(tmp_path):
    p = tmp_path/'t.ipynb'
    mk_nb(p, [('aaaa1111', 'x = one + two')])
    addr = lnhash(1, 'x = one + two')
    cell_exhash(p, 'aaaa1111', (addr, 's', 'one', '1'), (addr, 's', 'two', '2'))
    assert json.loads(p.read_text())['cells'][0]['source'] == 'x = 1 + 2'


def test_file_cell_exhash_targets(tmp_path):
    "Cross-target m/t: cell->cell, cell->file, file->cell, whole-cell % source; single write per notebook."
    nb1, nb2, f = tmp_path/'a.ipynb', tmp_path/'b.ipynb', tmp_path/'x.py'
    mk_nb(nb1, [('aaaa1111', ['k = 42\n', 'print(k)']), ('bbbb2222', 'y=1')])
    mk_nb(nb2, [('cccc3333', 'z=3')])
    f.write_text('start\n')
    # copy a line from one cell into another cell of a different notebook (prefix ids)
    diff = file_exhash(str(f), (f'{nb1}:aaaa:{lnhash(1, "k = 42")}', 't', f'{nb2}:cccc:0|0000|'))
    assert json.loads(nb2.read_text())['cells'][0]['source'] == 'k = 42\nz=3'
    assert f'{nb2}:cccc3333' in diff  # diff labelled with the resolved cell id
    # whole-cell source into a file, and a file line into a cell, in one command set
    file_exhash(str(f), (f'{nb1}:bbbb2222:%', 't', f'{f}:$'),
        (f'{f}:{lnhash(1, "start")}', 't', f'{nb1}:aaaa1111:0|0000|'))
    assert f.read_text() == 'start\ny=1\n'
    assert json.loads(nb1.read_text())['cells'][0]['source'] == ['start\n', 'k = 42\n', 'print(k)']
    # cut (m) between cells of the same notebook: one write applies both cells
    file_exhash(str(nb1), (f'{nb1}:aaaa1111:{lnhash(1, "start")}', 'm', f'{nb1}:bbbb2222:$'))
    cells = {c['id']: c['source'] for c in json.loads(nb1.read_text())['cells']}
    assert cells['aaaa1111'] == ['k = 42\n', 'print(k)'] and cells['bbbb2222'] == 'y=1\nstart'
    # a range must stay within one target; missing cells raise; stale hashes raise
    with pytest.raises(ValueError, match='one file or cell'): file_exhash(str(f), (f'{nb1}:aaaa1111:1|0000|,{nb1}:bbbb2222:1|0000|', 'd'))
    with pytest.raises(KeyError): file_exhash(str(f), (f'{nb1}:zzzz:1|0000|', 'd'))
    with pytest.raises(ValueError, match='stale'): file_exhash(str(f), (f'{nb1}:aaaa1111:1|beef|', 'd'))
