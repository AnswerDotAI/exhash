use std::io::BufRead;

use crate::lnhash::{parse_lnhash, parse_lnhash_prefix, LnHash};
use crate::EditError;

/// A fully parsed command, including any multiline text blocks.
#[derive(Debug, Clone)]
pub struct Command {
    pub addr1: LnHash,
    pub addr2: Option<LnHash>,
    pub has_comma: bool,
    pub cmd: Subcommand,
}

/// A command operation.
#[derive(Debug, Clone)]
pub enum Subcommand {
    Delete,
    Substitute(Subst),
    Append(Vec<String>),
    Insert(Vec<String>),
    Change(Vec<String>),
    Join,
    Move { dest: LnHash },
    Copy { dest: LnHash },
    /// Global (`g`) and inverted-global (`v`/`g!`).
    Global {
        invert: bool,
        pattern: String,
        cmd: Box<Subcommand>,
    },
    Indent { levels: usize },
    Dedent { levels: usize },
    Sort,
    Print,
}

#[derive(Debug, Clone)]
pub struct Subst {
    pub pattern: String,
    pub replacement: String,
    pub global: bool,
    pub case_insensitive: bool,
}

/// Parse commands from CLI argv, reading any multiline text blocks from `stdin`.
///
/// Each element of `args` is a single command line (e.g. `42|a3f2|s/foo/bar/g`).
pub fn parse_commands_from_args(
    args: &[String],
    stdin: &mut impl BufRead,
) -> Result<Vec<Command>, EditError> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        let cmd = parse_command_with_text(a, || read_text_block_from_bufread(stdin))?;
        out.push(cmd);
    }
    Ok(out)
}

/// Parse commands from a list of individual command strings (for programmatic APIs).
///
/// Each string is one command. For `a`/`i`/`c`, lines after the first are the text
/// block (no `.` terminator needed). For other commands, extra lines are an error.
pub fn parse_commands_from_strs(cmds: &[&str]) -> Result<Vec<Command>, EditError> {
    let mut out = Vec::with_capacity(cmds.len());
    for s in cmds {
        if s.trim().is_empty() { continue; }
        let cmd = parse_command_with_text_from_str(s)?;
        out.push(cmd);
    }
    Ok(out)
}

fn parse_command_with_text_from_str(input: &str) -> Result<Command, EditError> {
    let mut lines = input.split('\n');
    let first = lines.next().unwrap(); // split always yields at least one
    let remaining: Vec<String> = lines.map(|l| l.strip_suffix('\r').unwrap_or(l).to_string()).collect();
    let has_text = !remaining.is_empty();
    let cmd = parse_command_with_text(first, || {
        if has_text { Ok(remaining.clone()) }
        else { Ok(vec![]) }
    })?;
    // For non-text commands, extra lines are an error
    if has_text {
        match &cmd.cmd {
            Subcommand::Append(_) | Subcommand::Insert(_) | Subcommand::Change(_) => {}
            Subcommand::Global { cmd: sub, .. } => match sub.as_ref() {
                Subcommand::Append(_) | Subcommand::Insert(_) | Subcommand::Change(_) => {}
                _ if has_text => return Err(EditError::new("unexpected multiline input for this command")),
                _ => {}
            },
            _ => return Err(EditError::new("unexpected multiline input for this command")),
        }
    }
    Ok(cmd)
}

/// Parse commands from an ex-style script string.
///
/// Commands are separated by newlines. For `a`/`i`/`c` (and for global subcommands
/// that are `a`/`i`/`c`), the following lines up to a `.` line (dot on its own line)
/// are taken as the text block.
pub fn parse_commands_from_script(script: &str) -> Result<Vec<Command>, EditError> {
    let mut lines = script
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .peekable();

    let mut out = Vec::new();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let cmd = parse_command_with_text(line, || read_text_block_from_iter(&mut lines))?;
        out.push(cmd);
    }
    Ok(out)
}

fn parse_command_with_text<F>(line: &str, mut read_text: F) -> Result<Command, EditError>
where
    F: FnMut() -> Result<Vec<String>, EditError>,
{
    let line = line.trim();
    let (addr1, mut rest) = parse_lnhash_prefix(line)?;
    let mut has_comma = false;
    let mut addr2: Option<LnHash> = None;

    if rest.starts_with(',') {
        has_comma = true;
        let (a2, r2) = parse_lnhash_prefix(&rest[1..])?;
        addr2 = Some(a2);
        rest = r2;
    }

    let rest = rest.trim();
    if rest.is_empty() {
        return Err(EditError::new("missing command"));
    }

    let (cmd, trailing) = parse_subcommand_with_text(rest, &mut read_text)?;

    // No trailing junk for a top-level command.
    if !trailing.trim().is_empty() {
        return Err(EditError::new(format!(
            "unexpected trailing characters: {:?}",
            trailing
        )));
    }

    // Enforce 0|0000| rules.
    if addr1.lineno == 0 {
        if addr1.hash != 0 {
            return Err(EditError::new("0|0000| must have hash 0000"));
        }
        if has_comma || addr2.is_some() {
            return Err(EditError::new("0|0000| is not allowed in ranges"));
        }
        match cmd {
            Subcommand::Append(_) | Subcommand::Insert(_) => {}
            _ => {
                return Err(EditError::new(
                    "0|0000| is only allowed with i or a",
                ))
            }
        }
    }
    if let Some(a2) = addr2 {
        if a2.lineno == 0 {
            return Err(EditError::new("0|0000| is not allowed in ranges"));
        }
        if addr1.lineno == 0 {
            return Err(EditError::new("0|0000| is not allowed in ranges"));
        }
    }

    Ok(Command {
        addr1,
        addr2,
        has_comma,
        cmd,
    })
}

fn parse_subcommand_with_text<'a, F>(
    input: &'a str,
    read_text: &mut F,
) -> Result<(Subcommand, &'a str), EditError>
where
    F: FnMut() -> Result<Vec<String>, EditError>,
{
    let s = input.trim_start();
    if s.starts_with("sort") {
        let trailing = &s[4..];
        return Ok((Subcommand::Sort, trailing));
    }

    // g! must be checked before g
    if s.starts_with("g!") {
        return parse_global(&s[2..], true, read_text);
    }

    let mut chars = s.chars();
    let c = chars
        .next()
        .ok_or_else(|| EditError::new("missing command"))?;
    let rest = chars.as_str();

    match c {
        'd' => Ok((Subcommand::Delete, rest)),
        'p' => Ok((Subcommand::Print, rest)),
        'j' => Ok((Subcommand::Join, rest)),
        's' => {
            let (subst, trailing) = parse_substitute(rest)?;
            Ok((Subcommand::Substitute(subst), trailing))
        }
        'a' => {
            let text = read_text()?;
            Ok((Subcommand::Append(text), rest))
        }
        'i' => {
            let text = read_text()?;
            Ok((Subcommand::Insert(text), rest))
        }
        'c' => {
            let text = read_text()?;
            Ok((Subcommand::Change(text), rest))
        }
        'm' => {
            let dest_str = rest.trim();
            let dest = parse_lnhash(dest_str)?;
            if dest.lineno == 0 {
                return Err(EditError::new(
                    "destination 0|0000| is not allowed for m",
                ));
            }
            Ok((Subcommand::Move { dest }, ""))
        }
        't' => {
            let dest_str = rest.trim();
            let dest = parse_lnhash(dest_str)?;
            if dest.lineno == 0 {
                return Err(EditError::new(
                    "destination 0|0000| is not allowed for t",
                ));
            }
            Ok((Subcommand::Copy { dest }, ""))
        }
        'g' => parse_global(rest, false, read_text),
        'v' => parse_global(rest, true, read_text),
        '>' => {
            let levels = parse_optional_usize(rest)?;
            Ok((Subcommand::Indent { levels }, ""))
        }
        '<' => {
            let levels = parse_optional_usize(rest)?;
            Ok((Subcommand::Dedent { levels }, ""))
        }
        _ => Err(EditError::new(format!("unknown command: {c}"))),
    }
}

fn parse_optional_usize(s: &str) -> Result<usize, EditError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(1);
    }
    s.parse::<usize>()
        .map_err(|_| EditError::new(format!("invalid number: {s:?}")))
}

fn parse_global<'a, F>(
    rest: &'a str,
    invert: bool,
    read_text: &mut F,
) -> Result<(Subcommand, &'a str), EditError>
where
    F: FnMut() -> Result<Vec<String>, EditError>,
{
    let rest = rest.trim_start();
    if !rest.starts_with('/') {
        return Err(EditError::new("global requires /pat/cmd"));
    }
    let (pat, after_pat) = parse_delimited(rest, '/')?;
    let cmd_str = after_pat.trim_start();
    if cmd_str.is_empty() {
        return Err(EditError::new("global requires a subcommand"));
    }
    let (subcmd, trailing) = parse_subcommand_with_text(cmd_str, read_text)?;
    if !trailing.trim().is_empty() {
        return Err(EditError::new(format!(
            "unexpected trailing characters in global subcommand: {:?}",
            trailing
        )));
    }
    Ok((
        Subcommand::Global {
            invert,
            pattern: pat,
            cmd: Box::new(subcmd),
        },
        "",
    ))
}

fn parse_substitute(rest: &str) -> Result<(Subst, &str), EditError> {
    let rest = rest.trim_start();
    if !rest.starts_with('/') {
        return Err(EditError::new("substitute requires /pat/rep/[flags]"));
    }

    let (pat, after_pat) = parse_delimited(rest, '/')?;
    let (rep, after_rep) = scan_to_delim(after_pat, '/')?;

    let mut global = false;
    let mut case_insensitive = false;

    for ch in after_rep.trim().chars() {
        match ch {
            'g' => global = true,
            'i' => case_insensitive = true,
            _ => {
                return Err(EditError::new(format!(
                    "unknown substitute flag: {ch}"
                )))
            }
        }
    }

    if pat.is_empty() {
        return Err(EditError::new("substitute pattern may not be empty"));
    }

    Ok((
        Subst {
            pattern: pat,
            replacement: rep,
            global,
            case_insensitive,
        },
        "",
    ))
}

/// Parse a `/.../` delimited string from the start of `input`.
///
/// Returns (decoded, rest_after_closing_delim).
fn parse_delimited<'a>(input: &'a str, delim: char) -> Result<(String, &'a str), EditError> {
    let mut chars = input.chars();
    let first = chars
        .next()
        .ok_or_else(|| EditError::new("missing delimiter"))?;
    if first != delim {
        return Err(EditError::new("missing delimiter"));
    }

    let mut out = String::new();
    let mut escaped = false;
    let mut consumed = 1; // delim

    for ch in chars {
        consumed += ch.len_utf8();
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == delim {
            let rest = &input[consumed..];
            return Ok((out, rest));
        }
        out.push(ch);
    }

    Err(EditError::new("unterminated delimited string"))
}

/// Scan for the next unescaped `delim`, returning (content, rest_after_delim).
/// Unlike `parse_delimited`, does not expect a leading delimiter.
/// If no delimiter is found, returns all remaining input as content (allows optional trailing delim).
fn scan_to_delim<'a>(input: &'a str, delim: char) -> Result<(String, &'a str), EditError> {
    let mut out = String::new();
    let mut escaped = false;
    let mut consumed = 0;
    for ch in input.chars() {
        consumed += ch.len_utf8();
        if escaped { out.push(ch); escaped = false; continue; }
        if ch == '\\' { escaped = true; continue; }
        if ch == delim { return Ok((out, &input[consumed..])); }
        out.push(ch);
    }
    Ok((out, ""))
}

fn read_text_block_from_bufread(stdin: &mut impl BufRead) -> Result<Vec<String>, EditError> {
    let mut out = Vec::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = stdin
            .read_line(&mut buf)
            .map_err(|e| EditError::new(format!("failed to read stdin: {e}")))?;
        if n == 0 {
            return Err(EditError::new("unexpected EOF while reading text block"));
        }
        // Trim \n, then optional \r.
        if buf.ends_with('\n') {
            buf.pop();
            if buf.ends_with('\r') {
                buf.pop();
            }
        }
        if buf == "." {
            break;
        }
        if buf == ".." {
            out.push(".".to_string());
        } else {
            out.push(buf.clone());
        }
    }
    Ok(out)
}

fn read_text_block_from_iter<'a>(
    it: &mut impl Iterator<Item = &'a str>,
) -> Result<Vec<String>, EditError> {
    let mut out = Vec::new();
    loop {
        match it.next() {
            None => return Err(EditError::new("unexpected EOF while reading text block")),
            Some(line) => {
                let line = line.strip_suffix('\r').unwrap_or(line);
                if line == "." {
                    break;
                }
                if line == ".." {
                    out.push(".".to_string());
                } else {
                    out.push(line.to_string());
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lnhash::{format_lnhash, line_hash_u16};

    fn addr(lineno: usize, line: &str) -> String {
        format!("{}{}", format_lnhash(lineno, line), "")
    }

    #[test]
    fn parse_delete_range() {
        let l1 = "a";
        let l2 = "b";
        let cmd = format!(
            "{}{},{}{}d",
            1,
            format!("|{:04x}|", line_hash_u16(l1)),
            2,
            format!("|{:04x}|", line_hash_u16(l2))
        );
        let parsed = parse_commands_from_script(&cmd).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(matches!(parsed[0].cmd, Subcommand::Delete));
        assert!(parsed[0].has_comma);
    }

    #[test]
    fn parse_append_reads_text_block() {
        let input = format!(
            "{}a\nhello\nworld\n.\n",
            addr(1, "line")
        );
        let cmds = parse_commands_from_script(&input).unwrap();
        match &cmds[0].cmd {
            Subcommand::Append(t) => {
                assert_eq!(t, &vec!["hello".to_string(), "world".to_string()]);
            }
            _ => panic!("expected append"),
        }
    }

    #[test]
    fn parse_global_with_subst() {
        let cmd = format!("{}g/foo/s/bar/baz/", addr(1, "x"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Global { invert, pattern, cmd } => {
                assert!(!invert);
                assert_eq!(pattern, "foo");
                match cmd.as_ref() {
                    Subcommand::Substitute(s) => {
                        assert_eq!(s.pattern, "bar");
                        assert_eq!(s.replacement, "baz");
                    }
                    _ => panic!("expected substitute"),
                }
            }
            _ => panic!("expected global"),
        }
    }
}
