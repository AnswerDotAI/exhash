use std::collections::BTreeSet;

use regex::{Regex, RegexBuilder};

use crate::lnhash::line_hash_u16;
use crate::parse::{Command, Subcommand, Subst};
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
}

impl Engine {
    fn new(input_lines: Vec<String>) -> Self {
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
        }
    }

    fn apply_command(&mut self, cmd: &Command) -> Result<(), EditError> {
        let start = cmd.addr1.lineno;
        let end = cmd.addr2.map(|a| a.lineno).unwrap_or(start);
        if start > end && start != 0 {
            return Err(EditError::new(format!(
                "invalid range: {start}..{end}"
            )));
        }
        self.apply_subcommand(start, end, cmd.has_comma, &cmd.cmd)
    }

    fn apply_subcommand(
        &mut self,
        start: usize,
        end: usize,
        has_comma: bool,
        sub: &Subcommand,
    ) -> Result<(), EditError> {
        match sub {
            Subcommand::Delete => self.delete_range(start, end),
            Subcommand::Substitute(s) => self.substitute_range(start, end, s),
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
            Subcommand::Move { dest } => self.move_range(start, end, dest.lineno),
            Subcommand::Copy { dest } => self.copy_range(start, end, dest.lineno),
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

    fn resolve_range(&self, start: usize, end: usize) -> Result<(usize, usize), EditError> {
        if start == 0 || end == 0 {
            return Err(EditError::new("address 0 is not valid for this command"));
        }
        if start > end {
            return Err(EditError::new(format!(
                "invalid range: {start}..{end}"
            )));
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

    fn substitute_range(&mut self, start: usize, end: usize, s: &Subst) -> Result<(), EditError> {
        let (s_idx, e_idx) = self.resolve_range(start, end)?;
        let re = build_regex(&s.pattern, s.case_insensitive)?;
        for idx in s_idx..=e_idx {
            let old = self.lines[idx].text.clone();
            let new = if s.global {
                re.replace_all(&old, s.replacement.as_str()).to_string()
            } else {
                // replace first match
                if !re.is_match(&old) {
                    continue;
                }
                re.replace(&old, s.replacement.as_str()).to_string()
            };
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
        if levels == 0 {
            return Ok(());
        }
        let prefix = "    ".repeat(levels);
        for idx in s..=e {
            let new = format!("{}{}", prefix, self.lines[idx].text);
            self.lines[idx].text = new;
            self.lines[idx].modified = true;
        }
        Ok(())
    }

    fn dedent_range(&mut self, start: usize, end: usize, levels: usize) -> Result<(), EditError> {
        let (s, e) = self.resolve_range(start, end)?;
        if levels == 0 {
            return Ok(());
        }
        for idx in s..=e {
            let old = self.lines[idx].text.clone();
            let new = dedent(&old, levels);
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
                self.apply_subcommand(line_no, line_no, false, subcmd)?;
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
/// All lnhashes in the command list are verified against `input` before any edits are applied.
pub fn edit_text(input: &str, commands: &[Command]) -> Result<EditResult, EditError> {
    let input_lines: Vec<String> = input.lines().map(|l| l.to_string()).collect();

    verify_all(&input_lines, commands)?;

    let mut eng = Engine::new(input_lines);
    for c in commands {
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

    Ok(EditResult {
        lines,
        hashes,
        modified,
        deleted,
    })
}

fn verify_all(input_lines: &[String], commands: &[Command]) -> Result<(), EditError> {
    for c in commands {
        verify_lnhash(input_lines, c.addr1, &c.cmd)?;
        if let Some(a2) = c.addr2 {
            verify_lnhash(input_lines, a2, &c.cmd)?;
        }
        verify_subcommand_refs(input_lines, &c.cmd)?;
    }
    Ok(())
}

fn verify_subcommand_refs(input_lines: &[String], cmd: &Subcommand) -> Result<(), EditError> {
    match cmd {
        Subcommand::Move { dest } | Subcommand::Copy { dest } => {
            verify_lnhash_basic(input_lines, *dest)?;
            Ok(())
        }
        Subcommand::Global { cmd, .. } => verify_subcommand_refs(input_lines, cmd),
        _ => Ok(()),
    }
}

fn verify_lnhash(input_lines: &[String], addr: crate::LnHash, cmd: &Subcommand) -> Result<(), EditError> {
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
        verify_lnhash_basic(input_lines, addr)
    }
}

fn verify_lnhash_basic(input_lines: &[String], addr: crate::LnHash) -> Result<(), EditError> {
    if addr.lineno == 0 {
        return Err(EditError::new("address 0 is not allowed here"));
    }
    if addr.lineno > input_lines.len() {
        return Err(EditError::new(format!(
            "address out of range: {} > {}",
            addr.lineno,
            input_lines.len()
        )));
    }
    let actual = line_hash_u16(&input_lines[addr.lineno - 1]);
    if actual != addr.hash {
        return Err(EditError::new(format!(
            "stale lnhash at line {}: expected {:04x}, got {:04x}",
            addr.lineno, addr.hash, actual
        )));
    }
    Ok(())
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
    if a.is_empty() {
        return b.to_string();
    }
    if b.is_empty() {
        return a.to_string();
    }
    let a_end_ws = a.chars().last().map(|c| c.is_whitespace()).unwrap_or(false);
    let b_start_ws = b.chars().next().map(|c| c.is_whitespace()).unwrap_or(false);
    if a_end_ws || b_start_ws {
        format!("{a}{b}")
    } else {
        format!("{a} {b}")
    }
}

fn dedent(line: &str, levels: usize) -> String {
    let mut s = line.to_string();
    for _ in 0..levels {
        if s.starts_with("    ") {
            s = s[4..].to_string();
            continue;
        }
        if s.starts_with('\t') {
            s = s[1..].to_string();
            continue;
        }
        // Remove up to 4 leading spaces as one level.
        let mut removed = 0usize;
        let bytes = s.as_bytes();
        while removed < 4 && removed < bytes.len() && bytes[removed] == b' ' {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lnhash::{format_lnhash, line_hash_u16};
    use crate::parse::parse_commands_from_script;

    fn addr(lineno: usize, line: &str) -> String {
        format_lnhash(lineno, line)
    }

    #[test]
    fn verify_rejects_stale_hash() {
        let input = "hello\nworld\n";
        let stale = format!("1|{:04x}|d", line_hash_u16("HELLO"));
        let cmds = parse_commands_from_script(&stale).unwrap();
        let err = edit_text(input, &cmds).unwrap_err();
        assert!(err.message().contains("stale"));
    }

    #[test]
    fn delete_range_updates_deleted() {
        let input = "a\nb\nc\n";
        let cmd = format!("{},{}d", addr(1, "a"), addr(2, "b"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["c".to_string()]);
        assert_eq!(res.deleted, vec![1, 2]);
    }

    #[test]
    fn substitute_lenient_no_match() {
        let input = "abc\n";
        let cmd = format!("{}s/zzz/yyy/", addr(1, "abc"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["abc".to_string()]);
        assert!(res.modified.is_empty());
    }

    #[test]
    fn substitute_global_case_insensitive() {
        let input = "Foo foo\n";
        let cmd = format!("{}s/foo/bar/gi", addr(1, "Foo foo"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["bar bar".to_string()]);
        assert_eq!(res.modified, vec![1]);
    }

    #[test]
    fn insert_before_first_line_using_zero_address() {
        let input = "b\n";
        let script = "0|0000|i\na\n.\n";
        let cmds = parse_commands_from_script(script).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(res.modified, vec![1]);
    }

    #[test]
    fn join_single_address_joins_with_next() {
        let input = "hello\nworld\n";
        let cmd = format!("{}j", addr(1, "hello"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["hello world".to_string()]);
        assert_eq!(res.modified, vec![1]);
        assert_eq!(res.deleted, vec![2]);
    }

    #[test]
    fn move_range_marks_moved_lines_modified() {
        let input = "a\nb\nc\nd\n";
        // Move lines 2-3 after line 4.
        let cmd = format!(
            "{},{}m{}",
            addr(2, "b"),
            addr(3, "c"),
            addr(4, "d")
        );
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["a", "d", "b", "c"]);
        // moved lines now at 3 and 4
        assert_eq!(res.modified, vec![3, 4]);
        // nothing deleted
        assert!(res.deleted.is_empty());
    }

    #[test]
    fn global_delete_todo() {
        let input = "keep\nTODO one\nTODO two\nkeep2\n";
        let cmd = format!("{},{}g/TODO/d", addr(1, "keep"), addr(4, "keep2"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["keep".to_string(), "keep2".to_string()]);
        assert_eq!(res.deleted, vec![2, 3]);
    }

    #[test]
    fn indent_and_dedent() {
        let input = "a\n    b\n";
        let cmd1 = format!("{}>2", addr(1, "a"));
        let cmd2 = format!("{}<1", addr(2, "    b"));
        let script = format!("{}\n{}\n", cmd1, cmd2);
        let cmds = parse_commands_from_script(&script).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["        a".to_string(), "b".to_string()]);
        assert_eq!(res.modified, vec![1, 2]);
    }

    #[test]
    fn sort_range() {
        let input = "c\na\nb\n";
        let cmd = format!("{},{}sort", addr(1, "c"), addr(3, "b"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["a", "b", "c"]);
        assert_eq!(res.modified, vec![1, 2, 3]);
    }

    #[test]
    fn print_marks_for_output() {
        let input = "a\nb\n";
        let cmd = format!("{}p", addr(2, "b"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.modified, vec![2]);
    }

    #[test]
    fn global_inverted_delete() {
        let input = "keep\ndrop\nkeep2\n";
        let cmd = format!("{},{}g!/keep/d", addr(1, "keep"), addr(3, "keep2"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["keep".to_string(), "keep2".to_string()]);
        assert_eq!(res.deleted, vec![2]);
    }

    #[test]
    fn parser_rejects_zero_address_for_delete() {
        let script = "0|0000|d";
        let err = parse_commands_from_script(script).unwrap_err();
        assert!(err.message().contains("only allowed"));
    }

    #[test]
    fn move_destination_in_range_errors() {
        let input = "a\nb\nc\n";
        let cmd = format!("{},{}m{}", addr(1, "a"), addr(2, "b"), addr(2, "b"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let err = edit_text(input, &cmds).unwrap_err();
        assert!(err.message().contains("destination is within"));
    }

    #[test]
    fn copy_inserts_new_lines() {
        let input = "a\nb\nc\n";
        let cmd = format!("{},{}t{}", addr(1, "a"), addr(2, "b"), addr(3, "c"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["a", "b", "c", "a", "b"]);
        assert_eq!(res.modified, vec![4, 5]);
    }

    #[test]
    fn change_replaces_range() {
        let input = "a\nb\nc\n";
        let script = format!(
            "{},{}c\nX\nY\n.\n",
            addr(1, "a"),
            addr(2, "b")
        );
        let cmds = parse_commands_from_script(&script).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["X".to_string(), "Y".to_string(), "c".to_string()]);
        assert_eq!(res.deleted, vec![1, 2]);
        assert_eq!(res.modified, vec![1, 2]);
    }

    #[test]
    fn join_range_collapses_all() {
        let input = "a\nb\nc\n";
        let cmd = format!("{},{}j", addr(1, "a"), addr(3, "c"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        assert_eq!(res.lines, vec!["a b c".to_string()]);
        assert_eq!(res.deleted, vec![2, 3]);
        assert_eq!(res.modified, vec![1]);
    }

    #[test]
    fn multi_command_line_numbers_shift() {
        let input = "a\nb\nc\n";
        // Insert X before line 2, then delete line 3 (which was originally 2 before insertion? actually after insertion, line3 is original b)
        let script = format!(
            "{}i\nX\n.\n{}d\n",
            addr(2, "b"),
            addr(3, "c")
        );
        let cmds = parse_commands_from_script(&script).unwrap();
        let res = edit_text(input, &cmds).unwrap();
        // After insertion: a, X, b, c. Then delete line3 -> b removed.
        assert_eq!(res.lines, vec!["a".to_string(), "X".to_string(), "c".to_string()]);
        assert_eq!(res.deleted, vec![2]);
    }
}
