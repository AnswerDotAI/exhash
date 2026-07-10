import json, pytest
from pathlib import Path
from exhash import lnhash
from exhash.magic import exhash_magic

def test_exhash_magic(tmp_path):
    p = str(tmp_path / "f.py")
    payload = "x = '''one'''\ny = \"\"\"two\"\"\"\nz = r'\\n raw'"
    exhash_magic(f"{p} 0|0000| a", payload + "\n")  # cell arrives with trailing newline; stripped once
    assert Path(p).read_text() == payload + "\n"
    res = exhash_magic(f"{p} {lnhash(2, 'y = \"\"\"two\"\"\"')} c", "y = 2\n")
    assert "y = 2" in str(res)
    assert repr(res) == str(res)  # displays verbatim under IPython, not as a quoted repr
    assert Path(p).read_text() == "x = '''one'''\ny = 2\nz = r'\\n raw'\n"
    exhash_magic(f"{p} 0|0000| i", "# header\n")
    assert Path(p).read_text().startswith("# header\n")
    nb = dict(cells=[dict(id="abc", cell_type="code", source="x=1\n", metadata={})], metadata={}, nbformat=4, nbformat_minor=5)
    nbp = str(tmp_path / "nb.ipynb")
    Path(nbp).write_text(json.dumps(nb))
    exhash_magic(f"{nbp} abc {lnhash(1, 'x=1')} c", "x = '''nb'''\n")
    assert json.loads(Path(nbp).read_text())["cells"][0]["source"] == "x = '''nb'''\n"
    with pytest.raises(ValueError): exhash_magic(f"{p} 1|abcd| d", "")
    with pytest.raises(ValueError): exhash_magic(p, "text")
