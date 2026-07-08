"Engine command coverage at the Python surface (ported from src/engine.rs + src/parse.rs unit tests)."
import pytest

from exhash import exhash, lnhash


def test_global_delete():
    text = "keep\nTODO one\nTODO two\nkeep2\n"
    a1, a4 = lnhash(1, "keep"), lnhash(4, "keep2")
    res = exhash(text, [(f"{a1},{a4}", "g", "/TODO/d")])
    assert res["lines"] == ["keep", "keep2"]
    assert res["deleted"] == [2, 3]

def test_global_inverted_delete_and_v_alias():
    text = "keep\ndrop\nkeep2\n"
    a1, a3 = lnhash(1, "keep"), lnhash(3, "keep2")
    for cmd in ((f"{a1},{a3}", "g!", "/keep/d"), (f"{a1},{a3}", "v", "/keep/d")):
        res = exhash(text, [cmd])
        assert res["lines"] == ["keep", "keep2"]
        assert res["deleted"] == [2]

def test_sort_range():
    text = "c\na\nb\n"
    a1, a3 = lnhash(1, "c"), lnhash(3, "b")
    res = exhash(text, [(f"{a1},{a3}", "sort")])
    assert res["lines"] == ["a", "b", "c"]
    assert res["modified"] == [1, 2, 3]

def test_print_marks_for_output():
    text = "a\nb\n"
    res = exhash(text, [(lnhash(2, "b"), "p")])
    assert res["lines"] == ["a", "b"]
    assert res["modified"] == [2]

def test_indent_and_dedent():
    text = "a\n    b\n"
    a1, a2 = lnhash(1, "a"), lnhash(2, "    b")
    res = exhash(text, [(a1, ">", "2"), (a2, "<", "1")])
    assert res["lines"] == ["        a", "b"]
    assert res["modified"] == [1, 2]

def test_copy_inserts_new_lines():
    text = "a\nb\nc\n"
    a1, a2, a3 = lnhash(1, "a"), lnhash(2, "b"), lnhash(3, "c")
    res = exhash(text, [(f"{a1},{a2}", "t", a3)])
    assert res["lines"] == ["a", "b", "c", "a", "b"]
    assert res["modified"] == [4, 5]

def test_move_destination_in_range_errors():
    text = "a\nb\nc\n"
    a1, a2 = lnhash(1, "a"), lnhash(2, "b")
    with pytest.raises(ValueError, match="destination is within"): exhash(text, [(f"{a1},{a2}", "m", a2)])

def test_zero_address_delete_rejected():
    with pytest.raises(ValueError, match="only allowed"): exhash("a\n", [("0|0000|", "d")])

def test_substitute_no_match_fails():
    with pytest.raises(ValueError, match="no match"):
        exhash("abc\n", [(lnhash(1, "abc"), "s", "zzz", "yyy")])
    with pytest.raises(ValueError, match="no match"):
        exhash("abc\n", [(lnhash(1, "abc"), "s", "zzz", "yyy", "g")])
    # a match that leaves the text unchanged is not an error
    res = exhash("abc\n", [(lnhash(1, "abc"), "s", "abc", "abc")])
    assert res["lines"] == ["abc"]
    assert res["modified"] == []
    # g// payloads stay lenient: not every selected line need match the sub
    res = exhash("ab\na\n", [("%", "g", "/a/s/b/X/")])
    assert res["lines"] == ["aX", "a"]

def test_substitute_global_case_insensitive():
    res = exhash("Foo foo\n", [(lnhash(1, "Foo foo"), "s", "foo", "bar", "gi")])
    assert res["lines"] == ["bar bar"]
    assert res["modified"] == [1]


def test_raw_command_strings_are_rejected():
    with pytest.raises(TypeError, match="tuples"): exhash("a\n", [f"{lnhash(1, 'a')}d"])


def test_change_replaces_range():
    text = "a\nb\nc\n"
    a1, a2 = lnhash(1, "a"), lnhash(2, "b")
    res = exhash(text, [(f"{a1},{a2}", "c", "X\nY")])
    assert res["lines"] == ["X", "Y", "c"]
    assert res["deleted"] == [1, 2]
    assert res["modified"] == [1, 2]


def test_padded_range_address_is_accepted():
    text = "a\nb\nc\n"
    a1, a2 = lnhash(1, "a"), lnhash(2, "b")
    res = exhash(text, [(f" {a1}, {a2}", "d")])
    assert res["lines"] == ["c"]


def test_tuple_range_address_is_accepted():
    text = "foo\nbar\nbaz\n"
    a1, a2 = lnhash(1, "foo"), lnhash(2, "bar")
    res = exhash(text, [(f"{a1},{a2}", "s", "foo\nbar", "replaced")])
    assert res["lines"] == ["replaced", "baz"]

def test_join_collapses_indented_next_line():
    res = exhash("hello\n    world\n", [(lnhash(1, "hello"), "j")])
    assert res["lines"] == ["hello world"]

def test_transliterate_requires_equal_char_counts():
    with pytest.raises(ValueError): exhash("abc\n", [(lnhash(1, "abc"), "y", "/abc/AB/")])
