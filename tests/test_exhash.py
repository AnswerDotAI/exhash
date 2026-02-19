import pytest
from exhash import line_hash, lnhash, lnhashview, exhash

def test_line_hash_returns_4_hex():
    h = line_hash("hello")
    assert len(h) == 4
    assert all(c in '0123456789abcdef' for c in h)

def test_line_hash_deterministic():
    assert line_hash("foo") == line_hash("foo")
    assert line_hash("foo") != line_hash("bar")

def test_lnhash_format():
    addr = lnhash(1, "hello")
    assert addr.startswith("1|")
    assert addr.endswith("|")
    assert line_hash("hello") in addr

def test_lnhashview():
    lines = lnhashview("a\nb\nc")
    assert len(lines) == 3
    assert lines[0].endswith("  a")
    assert lines[2].endswith("  c")
    assert lines[0].startswith(lnhash(1, "a"))

def test_lnhashview_empty():
    assert lnhashview("") == []

def test_exhash_noop():
    res = exhash("foo\nbar\n")
    assert res.lines == ["foo", "bar"]
    assert res.text() == "foo\nbar"
    assert res.modified == []
    assert res.deleted == []

def test_exhash_substitute():
    text = "foo\nbar\n"
    addr = lnhash(1, "foo")
    res = exhash(text, f"{addr}s/foo/baz/")
    assert res.lines == ["baz", "bar"]
    assert res.modified == [1]
    assert len(res.hashes) == 2

def test_exhash_delete():
    text = "a\nb\nc\n"
    addr = lnhash(2, "b")
    res = exhash(text, f"{addr}d")
    assert res.lines == ["a", "c"]
    assert 2 in res.deleted

def test_exhash_append():
    text = "a\nb\n"
    addr = lnhash(1, "a")
    res = exhash(text, f"{addr}a\nx\ny")
    assert res.lines == ["a", "x", "y", "b"]
    assert res.modified == [2, 3]

def test_exhash_insert():
    text = "a\nb\n"
    addr = lnhash(2, "b")
    res = exhash(text, f"{addr}i\nx")
    assert res.lines == ["a", "x", "b"]
    assert res.modified == [2]

def test_exhash_stale_hash_raises():
    text = "hello\nworld\n"
    addr = lnhash(1, "wrong")
    with pytest.raises(ValueError):
        exhash(text, f"{addr}d")

def test_exhash_repr_shows_modified():
    text = "foo\nbar\n"
    addr = lnhash(1, "foo")
    res = exhash(text, f"{addr}s/foo/baz/")
    r = repr(res)
    assert "baz" in r
    assert "bar" not in r
    assert r == f"{lnhash(1, 'baz')}  baz"

def test_exhash_repr_noop_empty():
    assert repr(exhash("foo\n")) == ""

def test_exhash_view():
    text = "foo\nbar\n"
    res = exhash(text)
    assert res.view() == f"{lnhash(1, 'foo')}  foo\n{lnhash(2, 'bar')}  bar"

def test_exhash_result_hashes_match():
    text = "foo\nbar\n"
    res = exhash(text)
    for i, (h, line) in enumerate(zip(res.hashes, res.lines)):
        assert h == lnhash(i + 1, line)

def test_exhash_multiple_cmds():
    text = "a\nb\nc\n"
    a1, a3 = lnhash(1, "a"), lnhash(3, "c")
    res = exhash(text, f"{a1}s/a/A/", f"{a3}s/c/C/")
    assert res.lines == ["A", "b", "C"]
    assert res.modified == [1, 3]

def test_exhash_append_trailing_newline():
    text = "a\nb\n"
    addr = lnhash(1, "a")
    res = exhash(text, f"{addr}a\nx\n")
    assert res.lines == ["a", "x", "", "b"]

def test_exhash_multiline_non_text_cmd_raises():
    text = "a\nb\n"
    addr = lnhash(1, "a")
    with pytest.raises(ValueError):
        exhash(text, f"{addr}d\nextra")
