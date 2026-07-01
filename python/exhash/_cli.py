"Console-script entry points for the `exhash` and `lnhashview` CLIs."
import os, re, sys, tempfile
from pathlib import Path

from .exhash import exhash_argv as _exhash_argv, lnhashview as _lnhashview

_ADDR_RE = re.compile(r'(?:\$|%|\d+\|[0-9a-fA-F]{4}\|)')

EXHASH_USAGE = """\
Usage: exhash [-h] [--dry-run] [--stdin] [--sw N] <file|-> [commands...]

Verified line-addressed file editor using lnhash addresses.

ADDRESSING
  Commands use lnhash addresses: lineno|hash| where hash is a 4-char hex
  content hash. Use `lnhashview file.txt` to get addresses. Single: 12|a3f2|cmd
  Range: 12|a3f2|,15|b1c3|cmd  Last: $cmd  Whole: %cmd  Before line 1: 0|0000|

COMMANDS
  s/pat/rep/[flags]  Substitute (Rust regex; flags g, i). y/src/dst/ transliterate.
  d delete   a/i/c append/insert/change (inline text, or a text block via stdin
  terminated by a '.' line)   j join   m/t move/copy to dest   >/< indent/dedent
  sort   p print   g/pat/cmd, g!/pat/cmd, v/pat/cmd global

OPTIONS
  --dry-run  Show changes on stdout without writing
  --sw N     Shift width for < and > (default 4)
  --stdin    Read input from stdin (file must be '-'); outputs full file in lnhash
             format. Text blocks (a/i/c) are not supported in this mode.
  -h, --help Show this help
"""

LNHASHVIEW_USAGE = ("Usage: lnhashview <file> [start_line [end_line]]\n\n"
    "Prints lines as: <lineno>|<hash>|<content>; start_line/end_line are 1-based inclusive.")


def _die(msg, code=1):
    print(msg, file=sys.stderr)
    sys.exit(code)

def _read_text_or_die(path):
    data = Path(path).read_bytes()
    if b"\0" in data: _die("error: binary file rejected (NUL byte found)")
    try: return data.decode("utf-8")
    except UnicodeDecodeError: _die("error: non-UTF8 file rejected")

def _atomic_write(path, content):
    p = Path(path)
    d = str(p.parent) or "."
    fd, tmp = tempfile.mkstemp(dir=d, prefix=f".{p.name}.exhash.tmp.")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f: f.write(content)
        try: os.chmod(tmp, os.stat(path).st_mode)
        except FileNotFoundError: pass
        os.replace(tmp, path)
    except BaseException:
        try: os.unlink(tmp)
        except OSError: pass
        raise

def _needs_text_block(cmd):
    "True if `cmd` is an a/i/c command with no inline text (so it reads a stdin block)."
    m = _ADDR_RE.match(cmd)
    if not m: return False
    rest = cmd[m.end():]
    if rest.startswith(","):
        m2 = _ADDR_RE.match(rest[1:])
        if m2: rest = rest[1 + m2.end():]
    return rest[:1] in ("a", "i", "c") and rest[1:] == ""


def exhash_main(argv=None):
    "Entry point for the `exhash` console script."
    argv = list(sys.argv[1:] if argv is None else argv)
    dry_run = stdin_mode = False
    sw, i = 4, 0
    while i < len(argv):
        a = argv[i]
        if a == "--dry-run":
            dry_run = True
            i += 1
        elif a == "--stdin":
            stdin_mode = True
            i += 1
        elif a == "--sw":
            if i + 1 >= len(argv): _die("error: --sw requires an integer argument", 2)
            try: sw = int(argv[i + 1])
            except ValueError: _die(f"error: invalid --sw value {argv[i + 1]!r}", 2)
            i += 2
        elif a in ("-h", "--help"):
            print(EXHASH_USAGE, file=sys.stderr)
            return
        elif a.startswith("-") and len(a) > 1: _die(f"error: unknown flag {a}\n{EXHASH_USAGE}", 2)
        else: break
    if i >= len(argv): _die(EXHASH_USAGE, 2)
    file, cmds = argv[i], argv[i + 1:]

    if stdin_mode:
        if file != "-": _die(f"error: with --stdin, file must be '-' (got '{file}')", 2)
        text = sys.stdin.read()
        try: res = _exhash_argv(text, cmds, "", sw)
        except ValueError as e: _die(f"error: {e}\nnote: commands requiring text blocks (a/i/c) are not supported with --stdin", 2)
        for h, line in zip(res.hashes, res.lines): print(f"{h}{line}")
        return

    text_block = sys.stdin.read() if (not sys.stdin.isatty() or any(_needs_text_block(c) for c in cmds)) else ""
    try: text = _read_text_or_die(file)
    except FileNotFoundError: text = ""
    try: res = _exhash_argv(text, cmds, text_block, sw)
    except ValueError as e: _die(f"error: {e}", 2)
    new_text = "\n".join(res.lines) + "\n" if res.lines else ""
    if not dry_run:
        try: _atomic_write(file, new_text)
        except OSError as e: _die(f"error: failed to write {file}: {e}")
    diff = res.format_diff(1)
    if diff: sys.stdout.write(diff)


def lnhashview_main(argv=None):
    "Entry point for the `lnhashview` console script."
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv or len(argv) > 3: _die(LNHASHVIEW_USAGE, 2)
    file = argv[0]
    def _int(v, name):
        try: return int(v)
        except ValueError: _die(f"error: {name} must be an integer", 2)
    start = _int(argv[1], "start_line") if len(argv) > 1 else None
    end = _int(argv[2], "end_line") if len(argv) > 2 else None
    text = _read_text_or_die(file)
    if start is not None and end is None: end = start  # single arg shows just that line
    for line in _lnhashview(text, start, end): print(line)
