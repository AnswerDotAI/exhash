"IPython cell magic carrying a/i/c payload text verbatim, so delimiter-hostile payloads need no Python string quoting."
import shlex
from fastcore.basics import fail_clean
from . import stdexcs

@fail_clean(*stdexcs)
def exhash_magic(line, cell):
    """Apply one exhash a/i/c command with the cell body as its payload.

    Usage: %%exhash <path> [<cell_id>] <address> <a|i|c>
    The payload is the rest of the cell, taken verbatim except that one trailing
    newline is stripped. With <cell_id>, edits that notebook cell via exhash_cell."""
    from . import exhash_file, exhash_cell
    args = shlex.split(line)
    if len(args) not in (3,4): raise ValueError('usage: %%exhash <path> [<cell_id>] <address> <a|i|c>')
    *target, addr, cmd = args
    if cmd not in ('a','i','c'): raise ValueError(f'command must be a, i, or c; got {cmd!r}')
    if cell.endswith('\n'): cell = cell[:-1]
    command = (addr, cmd, cell)
    return exhash_cell(*target, command) if len(target)==2 else exhash_file(target[0], command)

def load_ipython_extension(ipython): ipython.register_magic_function(exhash_magic, 'cell', 'exhash')
