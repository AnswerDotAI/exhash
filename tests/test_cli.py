"CLI behavior for the `exhash` and `lnhashview` console scripts (ported from tests/cli.rs)."
import subprocess

from exhash import lnhash


def run(args, input=""): return subprocess.run(["exhash", *args], input=input, text=True, capture_output=True)
def runlv(args): return subprocess.run(["lnhashview", *args], input="", text=True, capture_output=True)

def ctx(n, l): return f' {lnhash(n, l)}{l}'
def add(n, l): return f'+{lnhash(n, l)}{l}'
def dele(n, l): return f'-{lnhash(n, l)}{l}'
def diff(body): return f"--- original\n+++ modified\n{body}"


def test_lnhashview_basic_and_range(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("alpha\nbeta\n\ngamma\n")
    out = runlv([str(f)])
    assert out.returncode == 0
    assert out.stdout == "".join(s + "\n" for s in (
        f"{lnhash(1, 'alpha')}alpha", f"{lnhash(2, 'beta')}beta",
        f"{lnhash(3, '')}", f"{lnhash(4, 'gamma')}gamma"))
    out = runlv([str(f), "2", "3"])
    assert out.stdout == f"{lnhash(2, 'beta')}beta\n{lnhash(3, '')}\n"
    out = runlv([str(f), "2", "260"])
    assert out.stdout == "".join(s + "\n" for s in (
        f"{lnhash(2, 'beta')}beta", f"{lnhash(3, '')}", f"{lnhash(4, 'gamma')}gamma"))

def test_inplace_substitute(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("foo\nbar\n")
    out = run([str(f), f"{lnhash(1, 'foo')}s/foo/baz/"])
    assert out.returncode == 0
    assert out.stdout == diff(f"{dele(1, 'foo')}\n{add(1, 'baz')}\n{ctx(2, 'bar')}\n")
    assert f.read_text() == "baz\nbar\n"

def test_inplace_transliterate(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("abc\ncab\n")
    out = run([str(f), f"{lnhash(1, 'abc')}y/abc/ABC/"])
    assert out.returncode == 0
    assert out.stdout == diff(f"{dele(1, 'abc')}\n{add(1, 'ABC')}\n{ctx(2, 'cab')}\n")
    assert f.read_text() == "ABC\ncab\n"

def test_dry_run_does_not_write(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("foo\nbar\n")
    out = run(["--dry-run", str(f), f"{lnhash(1, 'foo')}s/foo/baz/"])
    assert out.returncode == 0
    assert out.stdout == diff(f"{dele(1, 'foo')}\n{add(1, 'baz')}\n{ctx(2, 'bar')}\n")
    assert f.read_text() == "foo\nbar\n"

def test_custom_sw_option(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("a\n")
    out = run(["--sw", "2", str(f), f"{lnhash(1, 'a')}>1"])
    assert out.returncode == 0
    assert out.stdout == diff(f"{dele(1, 'a')}\n{add(1, '  a')}\n")
    assert f.read_text() == "  a\n"

def test_rejects_stale_lnhash(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("hello\nworld\n")
    cmd = f"{lnhash(1, 'hello')}d"
    f.write_text("HELLO\nworld\n")
    out = run([str(f), cmd])
    assert out.returncode != 0
    assert f.read_text() == "HELLO\nworld\n"

def test_rechecks_hashes_between_commands(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("a\nb\n")
    a1 = lnhash(1, "a")
    out = run([str(f), f"{a1}s/a/A/", f"{a1}d"])
    assert out.returncode != 0
    assert "stale lnhash" in out.stderr
    assert f.read_text() == "a\nb\n"

def test_percent_join_whole_file(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("a\nb\nc\n")
    out = run([str(f), "%j"])
    assert out.returncode == 0
    assert add(1, "a b c") in out.stdout
    assert dele(2, "b") in out.stdout
    assert dele(3, "c") in out.stdout
    assert f.read_text() == "a b c\n"

def test_dollar_deletes_last_line(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("a\nb\nc\n")
    out = run([str(f), "$d"])
    assert out.returncode == 0
    assert out.stdout == diff(f"{ctx(2, 'b')}\n{dele(3, 'c')}\n")
    assert f.read_text() == "a\nb\n"

def test_move_to_last_line_destination(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("a\nb\nc\n")
    out = run([str(f), f"{lnhash(1, 'a')}m$"])
    assert out.returncode == 0
    assert add(3, "a") in out.stdout
    assert f.read_text() == "b\nc\na\n"

def test_multiline_append_from_stdin(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("a\n")
    out = run([str(f), f"{lnhash(1, 'a')}a"], input="x\ny\n.\n")
    assert out.returncode == 0
    assert add(2, "x") in out.stdout
    assert add(3, "y") in out.stdout
    assert f.read_text() == "a\nx\ny\n"

def test_inline_change_from_arg(tmp_path):
    f = tmp_path / "f.txt"
    f.write_text("old\n")
    out = run([str(f), f"{lnhash(1, 'old')}c    new"])
    assert out.returncode == 0
    assert dele(1, "old") in out.stdout
    assert add(1, "    new") in out.stdout
    assert f.read_text() == "    new\n"

def test_creates_missing_file_with_zero_append(tmp_path):
    f = tmp_path / "new.txt"
    out = run([str(f), "0|0000|a"], input="first line\n.\n")
    assert out.returncode == 0
    assert out.stdout == diff(f"{add(1, 'first line')}\n")
    assert f.read_text() == "first line\n"

def test_rejects_binary_file(tmp_path):
    f = tmp_path / "f.bin"
    f.write_bytes(b"a\0b\n")
    out = run([str(f)])
    assert out.returncode != 0

def test_stdin_mode_edits_and_prints_full_file():
    out = run(["--stdin", "-", f"{lnhash(1, 'foo')}s/foo/baz/"], input="foo\nbar\n")
    assert out.returncode == 0
    assert out.stdout == f'{lnhash(1, "baz")}baz\n{lnhash(2, "bar")}bar\n'

def test_tilde_expansion(tmp_path):
    import os, subprocess
    env = {**os.environ, "HOME": str(tmp_path)}
    f = tmp_path / "f.txt"
    f.write_text("foo\nbar\n")
    out = subprocess.run(["lnhashview", "~/f.txt"], text=True, capture_output=True, env=env)
    assert out.returncode == 0 and "foo" in out.stdout
    out = subprocess.run(["exhash", "~/f.txt", f"{lnhash(1, 'foo')}s/foo/baz/"], input="", text=True, capture_output=True, env=env)
    assert out.returncode == 0
    assert f.read_text() == "baz\nbar\n"
