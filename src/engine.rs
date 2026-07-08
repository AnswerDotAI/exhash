use std::collections::{BTreeSet, HashMap};

use regex::{Regex, RegexBuilder};

use crate::lnhash::line_hash_u16;
use crate::parse::{Address, Command, Subcommand, Subst};
use crate::EditError;

/// Result of applying an edit script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditResult {
    /// Full edited content, split into lines (without trailing `\n`).
    pub lines: Vec<String>,
    /// lnhash for each line in the edited content (e.g. `"42|a3f2|"`).
    pub hashes: Vec<String>,
    /// New-file 1-based line numbers that are new, changed, reordered, or explicitly printed.
    pub modified: Vec<usize>,
    /// Old-file 1-based line numbers that were removed.
    pub deleted: Vec<usize>,
    /// For each output line, the 1-based original line number it came from (None if inserted).
    pub origins: Vec<Option<usize>>,
}

impl EditResult {
    /// Format a unified-diff-style summary of changes.
    ///
    /// Non-header lines are prefixed with ` ` (context), `+` (added/modified), or `-` (deleted),
    /// followed by the lnhash and content. `context` controls how many unchanged lines
    /// surround each hunk (default 1).
    /// Non-empty diffs start with `--- original` and `+++ modified` headers.
    pub fn format_diff(&self, original_lines: &[&str], context: usize) -> String {
        use crate::lnhash::format_lnhash;

        let mod_set: BTreeSet<usize> = self.modified.iter().copied().collect();
        let del_set: BTreeSet<usize> = self.deleted.iter().copied().collect();

        // Build interleaved sequence of (tag, lnhash, text) where tag is ' ', '+', '-'
        // Walk new lines, inserting deleted old lines at the right positions.
        let mut events: Vec<(char, String, &str)> = Vec::new();
        let mut next_old = 1usize; // next original line we expect

        for (new_idx, line) in self.lines.iter().enumerate() {
            let new_lineno = new_idx + 1;
            let origin = self.origins[new_idx];

            // Emit any deleted old lines that came before this line's origin
            if let Some(orig) = origin {
                while next_old < orig {
                    if del_set.contains(&next_old) {
                        let old_line = original_lines[next_old - 1];
                        events.push(('-', format_lnhash(next_old, old_line), old_line));
                    }
                    next_old += 1;
                }
                next_old = orig + 1;
            }

            if mod_set.contains(&new_lineno) {
                // Show the old line as deleted if this was a modification (not insertion)
                if let Some(orig) = origin {
                    let old_line = original_lines[orig - 1];
                    if old_line != line.as_str() {
                        events.push(('-', format_lnhash(orig, old_line), old_line));
                    }
                }
                events.push(('+', self.hashes[new_idx].clone(), line.as_str()));
            } else {
                events.push((' ', self.hashes[new_idx].clone(), line.as_str()));
            }
        }

        // Emit any remaining deleted lines at the end
        let old_len = original_lines.len();
        while next_old <= old_len {
            if del_set.contains(&next_old) {
                let old_line = original_lines[next_old - 1];
                events.push(('-', format_lnhash(next_old, old_line), old_line));
            }
            next_old += 1;
        }

        // Now group into hunks with context
        let interesting: BTreeSet<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, (tag, _, _))| *tag != ' ')
            .flat_map(|(i, _)| {
                let start = i.saturating_sub(context);
                let end = (i + context).min(events.len() - 1);
                start..=end
            })
            .collect();

        if interesting.is_empty() {
            return String::new();
        }

        let mut out = String::from("--- original\n+++ modified\n");
        let mut last: Option<usize> = None;
        for i in &interesting {
            if let Some(prev) = last {
                if *i > prev + 1 {
                    out.push_str("---\n");
                }
            }
            let (tag, ref hash, text) = events[*i];
            out.push(tag);
            out.push_str(hash);
            out.push_str(text);
            out.push('\n');
            last = Some(*i);
        }
        out
    }
}

#[derive(Debug, Clone)]
struct Line {
    text: String,
    origin: Option<usize>,
    modified: bool,
    global_mark: bool,
}

struct Engine {
    lines: Vec<Line>,
    deleted: BTreeSet<usize>,
    sw: usize,
}

impl Engine {
    fn new(input_lines: Vec<String>, sw: usize) -> Self {
        let lines = input_lines
            .into_iter()
            .enumerate()
            .map(|(i, text)| Line {
                text,
                origin: Some(i + 1),
                modified: false,
                global_mark: false,
            })
            .collect();
        Self {
            lines,
            deleted: BTreeSet::new(),
            sw,
        }
    }

    fn apply_command(&mut self, cmd: &Command) -> Result<(), EditError> {
        let (start, end, is_range) = self.resolve_command_range(cmd)?;
        if start > end && start != 0 {
            return Err(EditError::new(format!("invalid range: {start}..{end}")));
        }
        self.apply_subcommand(start, end, is_range, &cmd.cmd, true)
    }

    fn verify_command(&self, cmd: &Command) -> Result<(), EditError> {
        self.verify_address(cmd.addr1, &cmd.cmd)?;
        if let Some(a2) = cmd.addr2 {
            self.verify_address(a2, &cmd.cmd)?;
        }
        self.verify_subcommand_refs(&cmd.cmd)?;
        Ok(())
    }

    fn verify_subcommand_refs(&self, cmd: &Subcommand) -> Result<(), EditError> {
        match cmd {
            Subcommand::Move { dest } | Subcommand::Copy { dest } => {
                self.verify_destination(*dest)?;
                Ok(())
            }
            Subcommand::Global { cmd, .. } => self.verify_subcommand_refs(cmd),
            _ => Ok(()),
        }
    }

    fn verify_address(&self, addr: Address, cmd: &Subcommand) -> Result<(), EditError> {
        match addr {
            Address::LnHash(lh) => self.verify_lnhash(lh, cmd),
            Address::LastLine => self.resolve_last_line().map(|_| ()),
            Address::WholeFile => Ok(()),
        }
    }

    fn verify_destination(&self, dest: Address) -> Result<(), EditError> {
        match dest {
            Address::LnHash(lh) => self.verify_lnhash_basic(lh),
            Address::LastLine => self.resolve_last_line().map(|_| ()),
            Address::WholeFile => Err(EditError::new("destination % is not allowed")),
        }
    }

    fn verify_lnhash(&self, addr: crate::LnHash, cmd: &Subcommand) -> Result<(), EditError> {
        if addr.lineno == 0 {
            // Only valid for i/a, enforced by parser.
            if addr.hash != 0 {
                return Err(EditError::new("0|0000| must have hash 0000"));
            }
            match cmd {
                Subcommand::Append(_) | Subcommand::Insert(_) => Ok(()),
                _ => Err(EditError::new("0|0000| is only valid with i or a")),
            }
        } else {
            self.verify_lnhash_basic(addr)
        }
    }

    fn verify_lnhash_basic(&self, addr: crate::LnHash) -> Result<(), EditError> {
        if addr.lineno == 0 {
            return Err(EditError::new("address 0 is not allowed here"));
        }
        if addr.lineno > self.lines.len() {
            return Err(EditError::new(format!(
                "address out of range: {} > {}",
                addr.lineno,
                self.lines.len()
            )));
        }
        let actual = line_hash_u16(&self.lines[addr.lineno - 1].text);
        if actual != addr.hash {
            return Err(EditError::new(format!(
                "stale lnhash at line {}: expected {:04x}, got {:04x}",
                addr.lineno, addr.hash, actual
            )));
        }
        Ok(())
    }

    fn resolve_command_range(&self, cmd: &Command) -> Result<(usize, usize, bool), EditError> {
        if matches!(cmd.addr1, Address::WholeFile) {
            if cmd.has_comma || cmd.addr2.is_some() {
                return Err(EditError::new("% is already a whole-file range"));
            }
            return Ok(self.resolve_whole_file_range());
        }

        let start = self.resolve_address_lineno(cmd.addr1)?;
        let end = match cmd.addr2 {
            Some(a) => self.resolve_address_lineno(a)?,
            None => start,
        };
        Ok((start, end, cmd.has_comma))
    }

    fn resolve_address_lineno(&self, addr: Address) -> Result<usize, EditError> {
        match addr {
            Address::LnHash(a) => Ok(a.lineno),
            Address::LastLine => self.resolve_last_line(),
            Address::WholeFile => Err(EditError::new("% is only allowed as the first address")),
        }
    }

    fn resolve_destination_lineno(&self, dest: Address) -> Result<usize, EditError> {
        match dest {
            Address::LnHash(lh) => Ok(lh.lineno),
            Address::LastLine => self.resolve_last_line(),
            Address::WholeFile => Err(EditError::new("destination % is not allowed")),
        }
    }

    fn resolve_last_line(&self) -> Result<usize, EditError> {
        if self.lines.is_empty() {
            Err(EditError::new("address '$' out of range on empty file"))
        } else {
            Ok(self.lines.len())
        }
    }

    fn resolve_whole_file_range(&self) -> (usize, usize, bool) {
        if self.lines.is_empty() {
            (0, 0, true)
        } else {
            (1, self.lines.len(), true)
        }
    }

    fn apply_subcommand(
        &mut self,
        start: usize,
        end: usize,
        has_comma: bool,
        sub: &Subcommand,
        strict: bool,
    ) -> Result<(), EditError> {
        if has_comma && start == 0 && end == 0 {
            return self.apply_empty_range(sub);
        }
        match sub {
            Subcommand::Delete => self.delete_range(start, end),
            Subcommand::Substitute(s) => {
                let matched = self.substitute_range(start, end, s)?;
                if strict && !matched {
                    return Err(EditError::new(format!(
                        "s: no match for pattern `{}` in {start},{end}",
                        s.pattern
                    )));
                }
                Ok(())
            }
            Subcommand::Transliterate { source, dest } => {
                self.transliterate_range(start, end, source, dest)
            }
            Subcommand::Append(text) => self.append_after(start, end, text),
            Subcommand::Insert(text) => self.insert_before(start, text),
            Subcommand::Change(text) => self.change_range(start, end, text),
            Subcommand::Join => {
                if has_comma {
                    self.join_range(start, end)
                } else {
                    self.join_with_next(start)
                }
            }
            Subcommand::Move { dest } => {
                self.move_range(start, end, self.resolve_destination_lineno(*dest)?)
            }
            Subcommand::Copy { dest } => {
                self.copy_range(start, end, self.resolve_destination_lineno(*dest)?)
            }
            Subcommand::Global {
                invert,
                pattern,
                cmd,
            } => self.global(start, end, *invert, pattern, cmd),
            Subcommand::Indent { levels } => self.indent_range(start, end, *levels),
            Subcommand::Dedent { levels } => self.dedent_range(start, end, *levels),
            Subcommand::Sort => self.sort_range(start, end),
            Subcommand::Print => self.print_range(start, end),
        }
    }

    fn apply_empty_range(&mut self, sub: &Subcommand) -> Result<(), EditError> {
        match sub {
            Subcommand::Append(text) | Subcommand::Insert(text) | Subcommand::Change(text) => {
                self.insert_before(0, text)
            }
            _ => Ok(()),
        }
    }

    fn resolve_range(&self, start: usize, end: usize) -> Result<(usize, usize), EditError> {
        if start == 0 || end == 0 {
            return Err(EditError::new("address 0 is not valid for this command"));
        }
        if start > end {
            return Err(EditError::new(format!("invalid range: {start}..{end}")));
        }
        if end > self.lines.len() {
            return Err(EditError::new(format!(
                "address out of range: {end} > {}",
                self.lines.len()
            )));
        }
        Ok((start - 1, end - 1))
    }

    fn delete_range(&mut self, start: usize, end: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        let removed: Vec<Line> = self.lines.drain(s..=e).collect();
        for l in removed {
            if let Some(o) = l.origin {
                self.deleted.insert(o);
            }
        }
        Ok(())
    }

    fn substitute_range(&mut self, start: usize, end: usize, s: &Subst) -> Result<bool, EditError> {
        let (s_idx, e_idx) = self.resolve_range(start, end)?;
        let re = build_regex(&s.pattern, s.case_insensitive)?;
        let multiline = s.pattern.contains('\n') || s.replacement.contains('\n');
        let mut matched = false;
        if multiline {
            // Join range into single string, apply substitute, split back
            let joined: String = (s_idx..=e_idx)
                .map(|i| self.lines[i].text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if !re.is_match(&joined) {
                return Ok(false);
            }
            matched = true;
            let result = if s.global {
                re.replace_all(&joined, s.replacement.as_str()).to_string()
            } else {
                re.replace(&joined, s.replacement.as_str()).to_string()
            };
            if result == joined {
                return Ok(true);
            }
            let new_lines: Vec<String> = result.split('\n').map(|s| s.to_string()).collect();
            let origins: Vec<Option<usize>> =
                (s_idx..=e_idx).map(|i| self.lines[i].origin).collect();
            let new_line_objs: Vec<Line> = new_lines
                .into_iter()
                .enumerate()
                .map(|(i, text)| Line {
                    text,
                    origin: origins.get(i).copied().flatten(),
                    modified: true,
                    global_mark: false,
                })
                .collect();
            self.lines.splice(s_idx..=e_idx, new_line_objs);
        } else {
            for idx in s_idx..=e_idx {
                let old = self.lines[idx].text.clone();
                if !re.is_match(&old) {
                    continue;
                }
                matched = true;
                let new = if s.global {
                    re.replace_all(&old, s.replacement.as_str()).to_string()
                } else {
                    re.replace(&old, s.replacement.as_str()).to_string()
                };
                if new != old {
                    self.lines[idx].text = new;
                    self.lines[idx].modified = true;
                }
            }
        }
        Ok(matched)
    }

    fn transliterate_range(
        &mut self,
        start: usize,
        end: usize,
        source: &str,
        dest: &str,
    ) -> Result<(), EditError> {
        let (s_idx, e_idx) = self.resolve_range(start, end)?;
        let map: HashMap<char, char> = source.chars().zip(dest.chars()).collect();
        for idx in s_idx..=e_idx {
            let old = self.lines[idx].text.clone();
            let new: String = old
                .chars()
                .map(|ch| map.get(&ch).copied().unwrap_or(ch))
                .collect();
            if new != old {
                self.lines[idx].text = new;
                self.lines[idx].modified = true;
            }
        }
        Ok(())
    }

    fn append_after(&mut self, start: usize, end: usize, text: &[String]) -> Result<(), EditError> {
        // Append uses the end of the range if provided.
        let after = if start == 0 { 0 } else { end };
        let insert_at = if after == 0 {
            0
        } else {
            if after > self.lines.len() {
                return Err(EditError::new(format!(
                    "address out of range: {after} > {}",
                    self.lines.len()
                )));
            }
            after
        };

        if text.is_empty() {
            return Ok(());
        }

        let new_lines: Vec<Line> = text
            .iter()
            .map(|t| Line {
                text: t.clone(),
                origin: None,
                modified: true,
                global_mark: false,
            })
            .collect();

        self.lines.splice(insert_at..insert_at, new_lines);
        Ok(())
    }

    fn insert_before(&mut self, before: usize, text: &[String]) -> Result<(), EditError> {
        let insert_at = if before == 0 {
            0
        } else {
            if before > self.lines.len() {
                return Err(EditError::new(format!(
                    "address out of range: {before} > {}",
                    self.lines.len()
                )));
            }
            before - 1
        };

        if text.is_empty() {
            return Ok(());
        }

        let new_lines: Vec<Line> = text
            .iter()
            .map(|t| Line {
                text: t.clone(),
                origin: None,
                modified: true,
                global_mark: false,
            })
            .collect();

        self.lines.splice(insert_at..insert_at, new_lines);
        Ok(())
    }

    fn change_range(&mut self, start: usize, end: usize, text: &[String]) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        let removed: Vec<Line> = self.lines.drain(s..=e).collect();
        for l in removed {
            if let Some(o) = l.origin {
                self.deleted.insert(o);
            }
        }

        if text.is_empty() {
            return Ok(());
        }

        let new_lines: Vec<Line> = text
            .iter()
            .map(|t| Line {
                text: t.clone(),
                origin: None,
                modified: true,
                global_mark: false,
            })
            .collect();

        self.lines.splice(s..s, new_lines);
        Ok(())
    }

    fn join_with_next(&mut self, line: usize) -> Result<(), EditError> {
        if line == 0 {
            return Err(EditError::new("address 0 is not valid for join"));
        }
        if self.lines.len() < 2 {
            return Err(EditError::new("cannot join: file has fewer than 2 lines"));
        }
        if line >= self.lines.len() {
            return Err(EditError::new("cannot join: no next line"));
        }
        let idx = line - 1;
        let joined = join_strings(&self.lines[idx].text, &self.lines[idx + 1].text);
        if joined != self.lines[idx].text {
            self.lines[idx].text = joined;
            self.lines[idx].modified = true;
        }
        let removed = self.lines.remove(idx + 1);
        if let Some(o) = removed.origin {
            self.deleted.insert(o);
        }
        Ok(())
    }

    fn join_range(&mut self, start: usize, end: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        if s == e {
            return Ok(());
        }
        let mut joined = self.lines[s].text.clone();
        for i in (s + 1)..=e {
            joined = join_strings(&joined, &self.lines[i].text);
        }
        if joined != self.lines[s].text {
            self.lines[s].text = joined;
            self.lines[s].modified = true;
        }
        // Remove the rest.
        let removed: Vec<Line> = self.lines.drain((s + 1)..=e).collect();
        for l in removed {
            if let Some(o) = l.origin {
                self.deleted.insert(o);
            }
        }
        Ok(())
    }

    fn move_range(&mut self, start: usize, end: usize, dest: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        if dest == 0 {
            return Err(EditError::new("destination 0 is not allowed"));
        }
        if dest > self.lines.len() {
            return Err(EditError::new(format!(
                "destination out of range: {dest} > {}",
                self.lines.len()
            )));
        }
        if dest >= start && dest <= end {
            return Err(EditError::new("destination is within moved range"));
        }

        let seg_len = e - s + 1;
        let mut seg: Vec<Line> = self.lines.drain(s..=e).collect();
        for l in &mut seg {
            l.modified = true;
        }

        let insert_at = if dest < start {
            dest
        } else {
            // dest > end
            dest - seg_len
        };

        self.lines.splice(insert_at..insert_at, seg);
        Ok(())
    }

    fn copy_range(&mut self, start: usize, end: usize, dest: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        if dest == 0 {
            return Err(EditError::new("destination 0 is not allowed"));
        }
        if dest > self.lines.len() {
            return Err(EditError::new(format!(
                "destination out of range: {dest} > {}",
                self.lines.len()
            )));
        }

        let mut seg: Vec<Line> = self.lines[s..=e]
            .iter()
            .map(|l| Line {
                text: l.text.clone(),
                origin: None,
                modified: true,
                global_mark: false,
            })
            .collect();

        let insert_at = dest;
        self.lines.splice(insert_at..insert_at, seg.drain(..));
        Ok(())
    }

    fn indent_range(&mut self, start: usize, end: usize, levels: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        if levels == 0 || self.sw == 0 {
            return Ok(());
        }
        let prefix = " ".repeat(self.sw * levels);
        for idx in s..=e {
            let new = format!("{}{}", prefix, self.lines[idx].text);
            self.lines[idx].text = new;
            self.lines[idx].modified = true;
        }
        Ok(())
    }

    fn dedent_range(&mut self, start: usize, end: usize, levels: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        if levels == 0 || self.sw == 0 {
            return Ok(());
        }
        for idx in s..=e {
            let old = self.lines[idx].text.clone();
            let new = dedent(&old, levels, self.sw);
            if new != old {
                self.lines[idx].text = new;
                self.lines[idx].modified = true;
            }
        }
        Ok(())
    }

    fn sort_range(&mut self, start: usize, end: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        if s == e {
            return Ok(());
        }
        let before: Vec<String> = self.lines[s..=e].iter().map(|l| l.text.clone()).collect();
        self.lines[s..=e].sort_by(|a, b| a.text.cmp(&b.text));
        let after: Vec<String> = self.lines[s..=e].iter().map(|l| l.text.clone()).collect();
        if before != after {
            for l in &mut self.lines[s..=e] {
                l.modified = true;
            }
        }
        Ok(())
    }

    fn print_range(&mut self, start: usize, end: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        for idx in s..=e {
            self.lines[idx].modified = true;
        }
        Ok(())
    }

    fn global(
        &mut self,
        start: usize,
        end: usize,
        invert: bool,
        pattern: &str,
        subcmd: &Subcommand,
    ) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        let re = build_regex(pattern, false)?;

        // Clear existing marks.
        for l in &mut self.lines {
            l.global_mark = false;
        }

        for idx in s..=e {
            let m = re.is_match(&self.lines[idx].text);
            self.lines[idx].global_mark = if invert { !m } else { m };
        }

        let mut idx = 0usize;
        while idx < self.lines.len() {
            if self.lines[idx].global_mark {
                self.lines[idx].global_mark = false;
                // Apply subcommand to this line (single-line address, no comma).
                let line_no = idx + 1;
                self.apply_subcommand(line_no, line_no, false, subcmd, false)?;
                // Do not increment idx; after mutations, re-check this position.
                continue;
            }
            idx += 1;
        }

        // Ensure marks are cleared.
        for l in &mut self.lines {
            l.global_mark = false;
        }

        Ok(())
    }
}

/// Apply `commands` to the input text.
///
/// Each command's lnhashes are verified against the current text immediately before that
/// command is applied.
pub fn edit_text(input: &str, commands: &[Command]) -> Result<EditResult, EditError> {
    edit_text_with_sw(input, commands, 4)
}

pub fn edit_text_with_sw(
    input: &str,
    commands: &[Command],
    sw: usize,
) -> Result<EditResult, EditError> {
    let input_lines: Vec<String> = input.lines().map(|l| l.to_string()).collect();

    let mut eng = Engine::new(input_lines, sw);
    for c in commands {
        eng.verify_command(c)?;
        eng.apply_command(c)?;
    }

    let lines: Vec<String> = eng.lines.iter().map(|l| l.text.clone()).collect();
    let hashes: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}|{:04x}|", i + 1, line_hash_u16(l)))
        .collect();

    let modified: Vec<usize> = eng
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| if l.modified { Some(i + 1) } else { None })
        .collect();

    let deleted: Vec<usize> = eng.deleted.into_iter().collect();
    let origins: Vec<Option<usize>> = eng.lines.iter().map(|l| l.origin).collect();

    Ok(EditResult {
        lines,
        hashes,
        modified,
        deleted,
        origins,
    })
}

fn build_regex(pattern: &str, case_insensitive: bool) -> Result<Regex, EditError> {
    if case_insensitive {
        RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|e| EditError::new(format!("invalid regex: {e}")))
    } else {
        Regex::new(pattern).map_err(|e| EditError::new(format!("invalid regex: {e}")))
    }
}

fn join_strings(a: &str, b: &str) -> String {
    let b = b.trim_start_matches(char::is_whitespace);
    if a.is_empty() {
        return b.to_string();
    }
    if b.is_empty() {
        return a.to_string();
    }
    let a_end_ws = a.chars().last().map(|c| c.is_whitespace()).unwrap_or(false);
    if a_end_ws {
        format!("{a}{b}")
    } else {
        format!("{a} {b}")
    }
}

fn dedent(line: &str, levels: usize, sw: usize) -> String {
    let mut s = line.to_string();
    if sw == 0 {
        return s;
    }
    for _ in 0..levels {
        if s.starts_with('\t') {
            s = s[1..].to_string();
            continue;
        }
        // Remove up to `sw` leading spaces as one level.
        let mut removed = 0usize;
        let bytes = s.as_bytes();
        while removed < sw && removed < bytes.len() && bytes[removed] == b' ' {
            removed += 1;
        }
        if removed > 0 {
            s = s[removed..].to_string();
            continue;
        }
        break;
    }
    s
}
