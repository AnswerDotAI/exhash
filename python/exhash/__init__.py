from .exhash import line_hash, lnhash, lnhashview, exhash as _exhash

class EditResult:
    def __init__(self, r):
        self.lines, self.hashes, self.modified, self.deleted = r.lines, r.hashes, r.modified, r.deleted
    def text(self): return '\n'.join(self.lines)
    def view(self): return '\n'.join(f"{h}  {l}" for h, l in zip(self.hashes, self.lines))
    def __repr__(self):
        return '\n'.join(f"{self.hashes[i-1]}  {self.lines[i-1]}" for i in self.modified if i-1 < len(self.hashes))

def exhash(text, *cmds): return EditResult(_exhash(text, *cmds))
