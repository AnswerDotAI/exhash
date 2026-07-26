"Engine command coverage at the Python surface (ported from src/engine.rs + src/parse.rs unit tests)."
import pytest

from exhash import exhash, lnhash, lnhashview, file_exhash


def test_global_delete():
    text = "keep\nTODO one\nTODO two\nkeep2\n"
    a1, a4 = lnhash(1, "keep"), lnhash(4, "keep2")
    res = exhash(text, [(f"{a1},{a4}", "g", "TODO", ("d",))])
    assert res["lines"] == ["keep", "keep2"]
    assert res["deleted"] == [2, 3]

def test_global_inverted_delete_and_v_alias():
    text = "keep\ndrop\nkeep2\n"
    a1, a3 = lnhash(1, "keep"), lnhash(3, "keep2")
    for cmd in ((f"{a1},{a3}", "g!", "keep", ("d",)), (f"{a1},{a3}", "v", "keep", ("d",))):
        res = exhash(text, [cmd])
        assert res["lines"] == ["keep", "keep2"]
        assert res["deleted"] == [2]

def test_sort_range():
    text = "c\na\nb\n"
    a1, a3 = lnhash(1, "c"), lnhash(3, "b")
    res = exhash(text, [(f"{a1},{a3}", "sort")])
    assert res["lines"] == ["a", "b", "c"]
    assert res["modified"] == [1, 2, 3]

def test_print_reports_printed_not_modified():
    text = "a\nb\n"
    res = exhash(text, [(lnhash(2, "b"), "p")])
    assert res["lines"] == ["a", "b"]
    assert res["printed"] == [2]
    assert res["modified"] == []

def test_print_only_diff_is_a_bare_lnhashview():
    text = "".join(f"line {i}\n" for i in range(1, 13))
    lines = text.splitlines()
    res = exhash(text, [(lnhash(2, lines[1]), "p"), (lnhash(11, lines[10]), "p")])
    # line numbers pad to the width of the largest printed number, exactly like lnhashview
    assert res.format_diff() == " 2|8767|line 2\n11|2808|line 11\n"

def test_whole_file_print_equals_lnhashview():
    text = "".join(f"line {i}\n" for i in range(1, 13))
    res = exhash(text, [("%", "p")])
    assert res.format_diff() == "\n".join(lnhashview(text)) + "\n"

def test_printed_lines_are_forced_context_in_a_real_diff():
    text = "".join(f"line {i}\n" for i in range(1, 13))
    lines = text.splitlines()
    res = exhash(text, [(lnhash(11, lines[10]), "p"), (lnhash(2, lines[1]), "s", "line 2", "LINE TWO")])
    assert res["printed"] == [11]
    out = res.format_diff().splitlines()
    assert "-2|8767|line 2" in out
    assert "+2|3a84|LINE TWO" in out
    assert out[-1] == " 11|2808|line 11"
    assert "---" in out[1:]

def test_edited_and_printed_line_shows_once_as_added():
    text = "a\nb\n"
    res = exhash(text, [(lnhash(2, "b"), "p"), (lnhash(2, "b"), "s", "b", "B")])
    assert res["printed"] == [2]
    assert res["modified"] == [2]
    rows = [l for l in res.format_diff().splitlines() if l.endswith("B")]
    assert rows == ["+" + lnhash(2, "B") + "B"]

def test_global_print_emits_a_row_per_match():
    text = "alpha\nTODO one\nbeta\nTODO two\ngamma\n"
    res = exhash(text, [("%", "g", "TODO", ("p",))])
    assert res["printed"] == [2, 4]
    assert res["modified"] == []
    assert res.format_diff() == f"{lnhash(2, 'TODO one')}TODO one\n{lnhash(4, 'TODO two')}TODO two\n"

def test_printed_marks_travel_with_moved_lines():
    text = "a\nb\nc\n"
    res = exhash(text, [(lnhash(1, "a"), "p"), (lnhash(1, "a"), "m", lnhash(3, "c"))])
    assert res["lines"] == ["b", "c", "a"]
    assert res["printed"] == [3]

def test_indent_and_dedent():
    text = "a\n    b\n"
    a1, a2 = lnhash(1, "a"), lnhash(2, "    b")
    res = exhash(text, [(a1, ">", "2"), (a2, "<", "1")])
    assert res["lines"] == ["        a", "b"]
    assert res["modified"] == [1, 2]

def test_call_start_hashes_for_stacked_single_line_commands():
    text, addr = "abc def\nnext\n", lnhash(1, "abc def")
    res = exhash(text, [(addr, "s", "abc", "ABC"), (addr, "s", "def", "DEF")])
    assert res["lines"] == ["ABC DEF", "next"]
    assert exhash(text, [(addr, "s", "abc", "ABC"), (addr, "d")])["lines"] == ["next"]
    assert exhash(text, [(addr, "c", "changed"), (addr, "s", "changed", "CHANGED")])["lines"] == ["CHANGED", "next"]

    with pytest.raises(ValueError, match="changed since your view"): exhash(text, [(addr, "d"), (addr, "s", "abc", "ABC")])
    with pytest.raises(ValueError, match="already edited by an earlier command"):
        exhash(text, [(addr, "s", "abc", "ABC"), ("1|1234|", "d")])
    end = lnhash(2, "next")
    with pytest.raises(ValueError, match="stale lnhash"): exhash(text, [(f"{addr},{end}", "s", "a", "A"), (f"{addr},{end}", "s", "b", "B")])
    edited = exhash(text, [(addr, "s", "abc", "ABC")])
    with pytest.raises(ValueError, match="changed since your view"): exhash("\n".join(edited["lines"]), [(addr, "d")])

    text, addr = "TODO one\nkeep\nTODO two\n", lnhash(3, "TODO two")
    res = exhash(text, [("%", "g", "TODO", ("s", "TODO", "DONE")), (addr, "s", "two", "TWO")])
    assert res["lines"] == ["DONE one", "keep", "DONE TWO"]
    addr = lnhash(1, "abc")
    res = exhash("abc\n", [(addr, "s", "a", "A"), (addr, ">"), (addr, "s", "bc", "BC")])
    assert res["lines"] == ["    ABC"]
    inserted = lnhash(1, "inserted")
    with pytest.raises(ValueError, match="changed since your view"):
        exhash("base\n", [("0|0000|", "a", "inserted"), (inserted, "s", "inserted", "EDITED"), (inserted, "d")])
    joined = lnhash(1, "left")
    with pytest.raises(ValueError, match="changed since your view"):
        exhash("left\nright\n", [(joined, "s", "left", "LEFT"), (joined, "j"), (joined, "d")])

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
    with pytest.raises(ValueError, match="no match"): exhash("abc\n", [(lnhash(1, "abc"), "s", "zzz", "yyy")])
    with pytest.raises(ValueError, match="no match"): exhash("abc\n", [(lnhash(1, "abc"), "s", "zzz", "yyy", "g")])
    # a match that leaves the text unchanged is not an error
    res = exhash("abc\n", [(lnhash(1, "abc"), "s", "abc", "abc")])
    assert res["lines"] == ["abc"]
    assert res["modified"] == []
    # g// payloads stay lenient: not every selected line need match the sub
    res = exhash("ab\na\n", [("%", "g", "a", ("s", "b", "X"))])
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


def test_clean_traceback(tmp_path):
    "Errors from bad addresses show the caller's frame, not exhash internals"
    import traceback
    p = tmp_path/'t.txt'
    p.write_text('a\n')
    for fn,args in [(exhash, ('a\n', [('1|dead|', 'c', 'x')])), (file_exhash, (str(p), ('1|dead|', 'c', 'x')))]:
        try: fn(*args)
        except ValueError as e:
            assert e.__cause__ is None
            assert len(traceback.extract_tb(e.__traceback__)) <= 2, fn.__name__
        else: assert False


def test_global_structural_tuple():
    "g/g!/v take (pattern, inner-subcommand-tuple); string payloads are a clean-break rejection"
    text = "keep\nTODO one\nTODO two\nkeep2\n"
    res = exhash(text, [("%", "g", "TODO", ("d",))])
    assert res["lines"] == ["keep", "keep2"]
    res = exhash(text, [("%", "g", "TODO", ("s", "TODO", "DONE"))])
    assert res["lines"] == ["keep", "DONE one", "DONE two", "keep2"]
    res = exhash("ab\na\n", [("%", "g", "a", ("s", "b", "X", "g"))])
    assert res["lines"] == ["aX", "a"]
    for op in ("g!", "v"):
        res = exhash("keep\ndrop\nkeep2\n", [("%", op, "keep", ("d",))])
        assert res["lines"] == ["keep", "keep2"]
    res = exhash("x\ny\n", [("%", "g", "x", ("a", "after x"))])
    assert res["lines"] == ["x", "after x", "y"]
    with pytest.raises((TypeError, ValueError)): exhash(text, [("%", "g", "/TODO/d")])
    with pytest.raises(ValueError): exhash(text, [("%", "g", "a", ("g", "b", ("d",)))])  # no nesting


def test_transliterate_structural_tuple():
    "y takes (source, dest) fields; the /src/dst/ payload is a clean-break rejection"
    res = exhash("abc\n", [(lnhash(1, "abc"), "y", "abc", "ABC")])
    assert res["lines"] == ["ABC"]
    with pytest.raises(ValueError): exhash("abc\n", [(lnhash(1, "abc"), "y", "abc", "AB")])  # unequal counts
    with pytest.raises((TypeError, ValueError)): exhash("abc\n", [(lnhash(1, "abc"), "y", "/abc/ABC/")])
