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

def test_lnhashview_empty(): assert lnhashview("") == []

def test_exhash_noop():
    res = exhash("foo\nbar\n", [])
    assert res["lines"] == ["foo", "bar"]
    assert '\n'.join(res["lines"]) == "foo\nbar"
    assert res["modified"] == []
    assert res["deleted"] == []

def test_exhash_substitute():
    text = "foo\nbar\n"
    addr = lnhash(1, "foo")
    res = exhash(text, [addr+"s/foo/baz/"])
    assert res["lines"] == ["baz", "bar"]
    assert res["modified"] == [1]
    assert len(res["hashes"]) == 2

def test_exhash_substitute_rust_capture_groups():
    text = "abc123def\n"
    addr = lnhash(1, "abc123def")
    res = exhash(text, [addr+"s/([a-z]+)([0-9]+)([a-z]+)/$1-<$2>-$3/"])
    assert res["lines"] == ["abc-<123>-def"]

def test_exhash_substitute_rust_whole_match():
    text = "abc123def\n"
    addr = lnhash(1, "abc123def")
    res = exhash(text, [addr+"s/[0-9]+/[$0]/"])
    assert res["lines"] == ["abc[123]def"]

def test_exhash_substitute_rust_named_capture_groups():
    text = "abc123def\n"
    addr = lnhash(1, "abc123def")
    res = exhash(text, [addr+"s/(?P<head>[a-z]+)(?P<num>[0-9]+)(?P<tail>[a-z]+)/${head}<${num}>${tail}/"])
    assert res["lines"] == ["abc<123>def"]

def test_exhash_substitute_preserves_pattern_escapes():
    text = "abc123def\n"
    addr = lnhash(1, "abc123def")
    res = exhash(text, [addr+"s/\\d+/X/"])
    assert res["lines"] == ["abcXdef"]

def test_exhash_transliterate_range():
    text = "abc\ncab\n"
    a1, a2 = lnhash(1, "abc"), lnhash(2, "cab")
    res = exhash(text, [f"{a1},{a2}y/abc/ABC/"])
    assert res["lines"] == ["ABC", "CAB"]
    assert res["modified"] == [1, 2]

def test_exhash_delete():
    text = "a\nb\nc\n"
    addr = lnhash(2, "b")
    res = exhash(text, [addr+"d"])
    assert res["lines"] == ["a", "c"]
    assert 2 in res["deleted"]

def test_exhash_percent_whole_file_join():
    text = "a\nb\nc\n"
    res = exhash(text, ["%j"])
    assert res["lines"] == ["a b c"]
    assert res["deleted"] == [2, 3]

def test_exhash_percent_on_empty_file_is_noop():
    res = exhash("", ["%s/foo/bar/"])
    assert res["lines"] == []
    assert res["modified"] == []
    assert res["deleted"] == []

def test_exhash_dollar_addr1_and_addr2_forms():
    text = "a\nb\nc\n"
    res = exhash(text, ["$d"])
    assert res["lines"] == ["a", "b"]
    a2 = lnhash(2, "b")
    res = exhash(text, [f"{a2},$d"])
    assert res["lines"] == ["a"]

def test_exhash_move_destination_can_use_last_line():
    text = "a\nb\nc\n"
    a1 = lnhash(1, "a")
    res = exhash(text, [f"{a1}m$"])
    assert res["lines"] == ["b", "c", "a"]
    assert res["modified"] == [3]

def test_exhash_custom_sw():
    text = "a\n"
    a1 = lnhash(1, "a")
    res = exhash(text, [f"{a1}>1"], sw=2)
    assert res["lines"] == ["  a"]
    assert res["modified"] == [1]

def test_exhash_append():
    text = "a\nb\n"
    addr = lnhash(1, "a")
    res = exhash(text, [addr+"a\nx\ny"])
    assert res["lines"] == ["a", "x", "y", "b"]
    assert res["modified"] == [2, 3]

def test_exhash_insert():
    text = "a\nb\n"
    addr = lnhash(2, "b")
    res = exhash(text, [addr+"i\nx"])
    assert res["lines"] == ["a", "x", "b"]
    assert res["modified"] == [2]

def test_exhash_stale_hash_raises():
    text = "hello\nworld\n"
    addr = lnhash(1, "wrong")
    with pytest.raises(ValueError): exhash(text, [addr+"d"])

def test_exhash_result_supports_dict_access():
    text = "foo\nbar\n"
    addr = lnhash(1, "foo")
    res = exhash(text, [addr+"s/foo/baz/"])
    assert res["lines"] == ["baz", "bar"]
    assert res["modified"] == [1]
    assert res.lines == ["baz", "bar"]

def test_exhash_format_diff():
    text = "foo\nbar\n"
    addr = lnhash(1, "foo")
    res = exhash(text, [addr+"s/foo/baz/"])
    diff = res.format_diff()
    assert f"-{lnhash(1, 'foo')}  foo" in diff
    assert f"+{lnhash(1, 'baz')}  baz" in diff

def test_exhash_format_diff_no_changes():
    res = exhash("foo\n", [])
    assert res.format_diff() == ""


def test_exhash_view():
    text = "foo\nbar\n"
    res = exhash(text, [])
    view = '\n'.join(f"{h}  {l}" for h, l in zip(res["hashes"], res["lines"]))
    assert view == f"{lnhash(1, 'foo')}  foo\n{lnhash(2, 'bar')}  bar"

def test_exhash_result_hashes_match():
    text = "foo\nbar\n"
    res = exhash(text, [])
    for i, (h, line) in enumerate(zip(res["hashes"], res["lines"])): assert h == lnhash(i + 1, line)

def test_exhash_multiple_cmds():
    text = "a\nb\nc\n"
    a1, a3 = lnhash(1, "a"), lnhash(3, "c")
    res = exhash(text, [f"{a1}s/a/A/", f"{a3}s/c/C/"])
    assert res["lines"] == ["A", "b", "C"]
    assert res["modified"] == [1, 3]

def test_exhash_rechecks_hash_before_each_command():
    text = "a\nb\nc\n"
    a2, a3 = lnhash(2, "b"), lnhash(3, "c")
    with pytest.raises(ValueError, match="stale"): exhash(text, [f"{a2}i\nx", f"{a3}d"])

def test_exhash_append_trailing_newline():
    text = "a\nb\n"
    addr = lnhash(1, "a")
    res = exhash(text, [addr+"a\nx\n"])
    assert res["lines"] == ["a", "x", "", "b"]

def test_exhash_multiline_non_text_cmd_raises():
    text = "a\nb\n"
    addr = lnhash(1, "a")
    with pytest.raises(ValueError): exhash(text, [addr+"d\nextra"])

def test_exhash_accepts_tuple_cmds():
    text = "a\nb\n"
    a1, a2 = lnhash(1, "a"), lnhash(2, "b")
    res = exhash(text, (f"{a1}s/a/A/", f"{a2}s/b/B/"))
    assert res["lines"] == ["A", "B"]

def test_exhash_custom_delimiter():
    text = "a/b\n"
    addr = lnhash(1, "a/b")
    res = exhash(text, [f"{addr}s|a/b|c/d|"])
    assert res["lines"] == ["c/d"]

def test_exhash_literal_newline_in_pattern():
    text = "foo\nbar\nbaz\n"
    a1, a2 = lnhash(1, "foo"), lnhash(2, "bar")
    res = exhash(text, [f"{a1},{a2}s/foo\nbar/replaced/"])
    assert res["lines"] == ["replaced", "baz"]

def test_exhash_literal_newline_in_replacement():
    text = "foobar\nbaz\n"
    addr = lnhash(1, "foobar")
    res = exhash(text, [f"{addr}s/foobar/foo\nbar/"])
    assert res["lines"] == ["foo", "bar", "baz"]

def test_exhash_file_read(tmp_path):
    from exhash import lnhashview_file, exhash_file
    f = tmp_path / "test.txt"
    f.write_text("hello\nworld\n")
    lines = lnhashview_file(str(f))
    assert len(lines) == 2
    assert "hello" in lines[0]

def test_exhash_file_inplace(tmp_path):
    from exhash import exhash_file, lnhash
    f = tmp_path / "test.txt"
    f.write_text("foo\nbar\n")
    addr = lnhash(1, "foo")
    diff = exhash_file(str(f), [f"{addr}s/foo/baz/"], inplace=True)
    assert isinstance(diff, str)
    assert "+{}  baz".format(lnhash(1, "baz")) in diff
    assert f.read_text() == "baz\nbar\n"

def test_exhash_file_inplace_no_change_on_error(tmp_path):
    from exhash import exhash_file, lnhash
    f = tmp_path / "test.txt"
    f.write_text("foo\nbar\n")
    with pytest.raises(ValueError): exhash_file(str(f), ["99|ffff|s/x/y/"], inplace=True)
    assert f.read_text() == "foo\nbar\n"

def test_lnhashview_start_end():
    lines = lnhashview("a\nb\nc\nd", start=2, end=3)
    assert len(lines) == 2
    assert "b" in lines[0]
    assert "c" in lines[1]
    assert lines[0].startswith("2|")
    assert lines[1].startswith("3|")

def test_lnhashview_start_only():
    lines = lnhashview("a\nb\nc", start=2)
    assert len(lines) == 2
    assert lines[0].startswith("2|")

def test_lnhashview_end_only():
    lines = lnhashview("a\nb\nc", end=2)
    assert len(lines) == 2
    assert lines[0].startswith("1|")
    assert lines[1].startswith("2|")

def test_lnhashview_file_start_end(tmp_path):
    from exhash import lnhashview_file
    f = tmp_path / "test.txt"
    f.write_text("a\nb\nc\nd\n")
    lines = lnhashview_file(str(f), start=2, end=3)
    assert len(lines) == 2
    assert lines[0].startswith("2|")
    assert lines[1].startswith("3|")
