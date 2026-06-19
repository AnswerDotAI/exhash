use std::io::BufRead;

use crate::lnhash::{parse_lnhash_prefix, LnHash};
use crate::EditError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Address {
    LnHash(LnHash),
    /// `$` (last line in current buffer)
    LastLine,
    /// `%` (whole file; shorthand for `1,$`)
    WholeFile,
}

/// A fully parsed command, including any multiline text blocks.
#[derive(Debug, Clone)]
pub struct Command {
    pub addr1: Address,
    pub addr2: Option<Address>,
    pub has_comma: bool,
    pub cmd: Subcommand,
}

/// A command operation.
#[derive(Debug, Clone)]
pub enum Subcommand {
    Delete,
    Substitute(Subst),
    Transliterate {
        source: String,
        dest: String,
    },
    Append(Vec<String>),
    Insert(Vec<String>),
    Change(Vec<String>),
    Join,
    Move {
        dest: Address,
    },
    Copy {
        dest: Address,
    },
    /// Global (`g`) and inverted-global (`v`/`g!`).
    Global {
        invert: bool,
        pattern: String,
        cmd: Box<Subcommand>,
    },
    Indent {
        levels: usize,
    },
    Dedent {
        levels: usize,
    },
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
/// Each string is one command. For multiline `a`/`i`/`c`, include the text block
/// in the same string using newline characters. Text after the command character
/// is the first inserted line, so `cfirst\nsecond` and `c\nfirst\nsecond`
/// are both valid. Do not use `.` terminators or split the text block into
/// separate entries; a trailing `.` line is literal text.
/// For other commands, extra lines are an error.
pub fn parse_commands_from_strs(cmds: &[&str]) -> Result<Vec<Command>, EditError> {
    let mut out = Vec::with_capacity(cmds.len());
    for s in cmds {
        if s.trim().is_empty() {
            continue;
        }
        let cmd = parse_command_with_text_from_str(s)?;
        out.push(cmd);
    }
    Ok(out)
}

fn parse_command_with_text_from_str(input: &str) -> Result<Command, EditError> {
    // Try parsing the full string first — handles commands with literal newlines
    // inside delimited sections (e.g. s/foo\nbar/baz/ or y with custom delims).
    let full_err = match parse_command_with_text(input.trim(), || Ok(vec![])) {
        Ok(cmd) => return Ok(cmd),
        Err(e) => e,
    };

    // Fall back to line-split approach for text commands (a/i/c)
    let mut lines = input.split('\n');
    let first = lines.next().unwrap();
    let remaining: Vec<String> = lines
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    if remaining.is_empty() {
        return Err(full_err);
    }
    let mut used_text_block = false;
    let mut cmd = parse_command_with_text(first, || {
        used_text_block = true;
        Ok(remaining.clone())
    })?;
    if !used_text_block {
        append_remaining_text(&mut cmd.cmd, &remaining)?;
    }
    Ok(cmd)
}

fn append_remaining_text(cmd: &mut Subcommand, remaining: &[String]) -> Result<(), EditError> {
    match cmd {
        Subcommand::Append(text) | Subcommand::Insert(text) | Subcommand::Change(text) => {
            text.extend_from_slice(remaining);
            Ok(())
        }
        Subcommand::Global { cmd: sub, .. } => append_remaining_text(sub, remaining),
        _ => Err(EditError::new(
            "unexpected multiline input for this command",
        )),
    }
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
    let (addr1, mut rest) = parse_address_prefix(line)?;
    let mut has_comma = false;
    let mut addr2: Option<Address> = None;

    if rest.starts_with(',') {
        has_comma = true;
        let (a2, r2) = parse_address_prefix(&rest[1..])?;
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

    if matches!(addr1, Address::WholeFile) {
        if has_comma || addr2.is_some() {
            return Err(EditError::new("% is already a whole-file range"));
        }
    }
    if matches!(addr2, Some(Address::WholeFile)) {
        return Err(EditError::new("% is only allowed as a standalone address"));
    }

    // Enforce 0|0000| rules.
    if let Address::LnHash(a1) = addr1 {
        if a1.lineno == 0 {
            if a1.hash != 0 {
                return Err(EditError::new("0|0000| must have hash 0000"));
            }
            if has_comma || addr2.is_some() {
                return Err(EditError::new("0|0000| is not allowed in ranges"));
            }
            match cmd {
                Subcommand::Append(_) | Subcommand::Insert(_) => {}
                _ => return Err(EditError::new("0|0000| is only allowed with i or a")),
            }
        }
    }
    if let Some(Address::LnHash(a2)) = addr2 {
        if a2.lineno == 0 {
            return Err(EditError::new("0|0000| is not allowed in ranges"));
        }
        if matches!(addr1, Address::LnHash(LnHash { lineno: 0, .. })) {
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

fn parse_address_prefix(input: &str) -> Result<(Address, &str), EditError> {
    if let Some(rest) = input.strip_prefix('$') {
        return Ok((Address::LastLine, rest));
    }
    if let Some(rest) = input.strip_prefix('%') {
        return Ok((Address::WholeFile, rest));
    }
    let (lh, rest) = parse_lnhash_prefix(input)?;
    Ok((Address::LnHash(lh), rest))
}

fn parse_destination_address(input: &str, op: char) -> Result<Address, EditError> {
    let (addr, rest) = parse_address_prefix(input)?;
    if !rest.trim().is_empty() {
        return Err(EditError::new(format!(
            "unexpected trailing characters after destination: {:?}",
            rest
        )));
    }
    match addr {
        Address::LnHash(LnHash { lineno: 0, .. }) => Err(EditError::new(format!(
            "destination 0|0000| is not allowed for {op}"
        ))),
        Address::WholeFile => Err(EditError::new(format!(
            "destination % is not allowed for {op}"
        ))),
        _ => Ok(addr),
    }
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
        'y' => {
            let ((source, dest), trailing) = parse_transliterate(rest)?;
            Ok((Subcommand::Transliterate { source, dest }, trailing))
        }
        'a' => {
            let (text, trailing) = parse_text_command(rest, read_text)?;
            Ok((Subcommand::Append(text), trailing))
        }
        'i' => {
            let (text, trailing) = parse_text_command(rest, read_text)?;
            Ok((Subcommand::Insert(text), trailing))
        }
        'c' => {
            let (text, trailing) = parse_text_command(rest, read_text)?;
            Ok((Subcommand::Change(text), trailing))
        }
        'm' => {
            let dest_str = rest.trim();
            let dest = parse_destination_address(dest_str, 'm')?;
            Ok((Subcommand::Move { dest }, ""))
        }
        't' => {
            let dest_str = rest.trim();
            let dest = parse_destination_address(dest_str, 't')?;
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
fn parse_text_command<'a, F>(
    rest: &'a str,
    read_text: &mut F,
) -> Result<(Vec<String>, &'a str), EditError>
where
    F: FnMut() -> Result<Vec<String>, EditError>,
{
    if rest.is_empty() || rest.contains('\n') {
        Ok((read_text()?, rest))
    } else {
        Ok((vec![rest.to_string()], ""))
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
    let delim = rest
        .chars()
        .next()
        .ok_or_else(|| EditError::new("global requires <delim>pat<delim>cmd"))?;
    if delim.is_alphanumeric() || delim == '\\' {
        return Err(EditError::new(
            "global delimiter must not be alphanumeric or backslash",
        ));
    }
    let (pat, after_pat) = parse_delimited(rest, delim)?;
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
    let delim = rest
        .chars()
        .next()
        .ok_or_else(|| EditError::new("substitute requires <delim>pat<delim>rep<delim>[flags]"))?;
    if delim.is_alphanumeric() || delim == '\\' {
        return Err(EditError::new(
            "substitute delimiter must not be alphanumeric or backslash",
        ));
    }

    let (pat, after_pat) = parse_delimited(rest, delim)?;
    let (rep, after_rep) = scan_to_delim(after_pat, delim)?;

    let mut global = false;
    let mut case_insensitive = false;

    for ch in after_rep.trim().chars() {
        match ch {
            'g' => global = true,
            'i' => case_insensitive = true,
            _ => return Err(EditError::new(format!("unknown substitute flag: {ch}"))),
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

fn parse_transliterate(rest: &str) -> Result<((String, String), &str), EditError> {
    let rest = rest.trim_start();
    let delim = rest
        .chars()
        .next()
        .ok_or_else(|| EditError::new("transliterate requires <delim>source<delim>dest<delim>"))?;
    if delim.is_alphanumeric() || delim == '\\' {
        return Err(EditError::new(
            "transliterate delimiter must not be alphanumeric or backslash",
        ));
    }

    let (source, after_source) = parse_delimited(rest, delim)?;
    let (dest, trailing) = scan_to_delim(after_source, delim)?;

    if source.chars().count() != dest.chars().count() {
        return Err(EditError::new(
            "transliterate source and destination must have the same number of characters",
        ));
    }

    Ok(((source, dest), trailing))
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
            if ch == delim {
                out.push(ch);
            } else {
                out.push('\\');
                out.push(ch);
            }
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
        if escaped {
            if ch == delim {
                out.push(ch);
            } else {
                out.push('\\');
                out.push(ch);
            }
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == delim {
            return Ok((out, &input[consumed..]));
        }
        out.push(ch);
    }
    if escaped {
        out.push('\\');
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
    fn parse_whole_file_address() {
        let parsed = parse_commands_from_script("%d").unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(matches!(parsed[0].addr1, Address::WholeFile));
        assert!(parsed[0].addr2.is_none());
        assert!(matches!(parsed[0].cmd, Subcommand::Delete));
    }

    #[test]
    fn parse_last_line_address_forms() {
        let parsed = parse_commands_from_script("$d").unwrap();
        assert!(matches!(parsed[0].addr1, Address::LastLine));
        assert!(parsed[0].addr2.is_none());

        let cmd = format!("{},$d", addr(1, "a"));
        let parsed = parse_commands_from_script(&cmd).unwrap();
        assert!(matches!(parsed[0].addr1, Address::LnHash(_)));
        assert!(matches!(parsed[0].addr2, Some(Address::LastLine)));
    }

    #[test]
    fn parse_last_line_move_destination() {
        let cmd = format!("{}m$", addr(1, "a"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Move { dest } => assert!(matches!(dest, Address::LastLine)),
            _ => panic!("expected move"),
        }
    }

    #[test]
    fn parse_rejects_whole_file_move_destination() {
        let cmd = format!("{}m%", addr(1, "a"));
        let err = parse_commands_from_script(&cmd).unwrap_err();
        assert!(err.message().contains("destination % is not allowed"));
    }

    #[test]
    fn parse_append_reads_text_block() {
        let input = format!("{}a\nhello\nworld\n.\n", addr(1, "line"));
        let cmds = parse_commands_from_script(&input).unwrap();
        match &cmds[0].cmd {
            Subcommand::Append(t) => {
                assert_eq!(t, &vec!["hello".to_string(), "world".to_string()]);
            }
            _ => panic!("expected append"),
        }
    }

    #[test]
    fn parse_inline_text_for_a_i_c() {
        let append = format!("{}a appended", addr(1, "line"));
        let insert = format!("{}i    indented", addr(1, "line"));
        let change = format!("{}c    changed", addr(1, "line"));
        let cmds = parse_commands_from_strs(&[&append, &insert, &change]).unwrap();
        match &cmds[0].cmd {
            Subcommand::Append(t) => assert_eq!(t, &vec![" appended".to_string()]),
            _ => panic!("expected append"),
        }
        match &cmds[1].cmd {
            Subcommand::Insert(t) => assert_eq!(t, &vec!["    indented".to_string()]),
            _ => panic!("expected insert"),
        }
        match &cmds[2].cmd {
            Subcommand::Change(t) => assert_eq!(t, &vec!["    changed".to_string()]),
            _ => panic!("expected change"),
        }
    }

    #[test]
    fn parse_str_text_block_can_start_on_command_line() {
        let change = format!("{}cfirst\nsecond\nthird", addr(1, "line"));
        let insert = format!("{}i    indented\nnext", addr(1, "line"));
        let cmds = parse_commands_from_strs(&[&change, &insert]).unwrap();
        match &cmds[0].cmd {
            Subcommand::Change(t) => assert_eq!(
                t,
                &vec![
                    "first".to_string(),
                    "second".to_string(),
                    "third".to_string()
                ]
            ),
            _ => panic!("expected change"),
        }
        match &cmds[1].cmd {
            Subcommand::Insert(t) => {
                assert_eq!(t, &vec!["    indented".to_string(), "next".to_string()])
            }
            _ => panic!("expected insert"),
        }
    }

    #[test]
    fn parse_global_with_subst() {
        let cmd = format!("{}g/foo/s/bar/baz/", addr(1, "x"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Global {
                invert,
                pattern,
                cmd,
            } => {
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

    #[test]
    fn parse_global_custom_delimiter() {
        let cmd = format!("{}g@foo@s/bar/baz/", addr(1, "x"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Global {
                invert,
                pattern,
                cmd,
            } => {
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

    #[test]
    fn parse_global_same_delim_combo() {
        // g and s both use /
        let cmd = format!("{}g/foo/s/bar/baz/g", addr(1, "x"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Global { pattern, cmd, .. } => {
                assert_eq!(pattern, "foo");
                match cmd.as_ref() {
                    Subcommand::Substitute(s) => {
                        assert_eq!(s.pattern, "bar");
                        assert_eq!(s.replacement, "baz");
                        assert!(s.global);
                    }
                    _ => panic!("expected substitute"),
                }
            }
            _ => panic!("expected global"),
        }
    }

    #[test]
    fn parse_global_mixed_delim_combo() {
        // g uses @ (pattern contains /), s uses /
        let cmd = format!("{}g@a/b@s/old/new/", addr(1, "x"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Global { pattern, cmd, .. } => {
                assert_eq!(pattern, "a/b");
                match cmd.as_ref() {
                    Subcommand::Substitute(s) => {
                        assert_eq!(s.pattern, "old");
                        assert_eq!(s.replacement, "new");
                    }
                    _ => panic!("expected substitute"),
                }
            }
            _ => panic!("expected global"),
        }
    }

    #[test]
    fn parse_substitute_preserves_rust_regex_escapes() {
        let cmd = format!("{}s/\\d+/X/", addr(1, "a1"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "\\d+");
                assert_eq!(s.replacement, "X");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_substitute_preserves_rust_replacement_groups() {
        let cmd = format!("{}s/(a)(b)/$2$1/", addr(1, "ab"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "(a)(b)");
                assert_eq!(s.replacement, "$2$1");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_substitute_supports_escaped_delimiter() {
        let cmd = format!("{}s/a\\/b/c\\/d/", addr(1, "a/b"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "a/b");
                assert_eq!(s.replacement, "c/d");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_transliterate_basic() {
        let cmd = format!("{}y/abc/ABC/", addr(1, "abc"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Transliterate { source, dest } => {
                assert_eq!(source, "abc");
                assert_eq!(dest, "ABC");
            }
            _ => panic!("expected transliterate"),
        }
    }

    #[test]
    fn parse_transliterate_supports_escaped_delimiter() {
        let cmd = format!("{}y/a\\/b/A_B/", addr(1, "a/b"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Transliterate { source, dest } => {
                assert_eq!(source, "a/b");
                assert_eq!(dest, "A_B");
            }
            _ => panic!("expected transliterate"),
        }
    }

    #[test]
    fn parse_transliterate_requires_equal_char_counts() {
        let cmd = format!("{}y/ab/XYZ/", addr(1, "ab"));
        let err = parse_commands_from_script(&cmd).unwrap_err();
        assert!(err.message().contains("same number of characters"));
    }

    #[test]
    fn parse_substitute_custom_delimiter() {
        let cmd = format!("{}s@foo@bar@", addr(1, "foo"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "foo");
                assert_eq!(s.replacement, "bar");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_substitute_custom_delimiter_with_slash_in_content() {
        let cmd = format!("{}s|a/b|c/d|", addr(1, "a/b"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "a/b");
                assert_eq!(s.replacement, "c/d");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_substitute_custom_delimiter_with_flags() {
        let cmd = format!("{}s#foo#bar#gi", addr(1, "foo"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "foo");
                assert_eq!(s.replacement, "bar");
                assert!(s.global);
                assert!(s.case_insensitive);
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_substitute_literal_newline_via_strs() {
        let cmd = format!("{}s/foo\nbar/baz/", addr(1, "foo"));
        let cmds = parse_commands_from_strs(&[&cmd]).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "foo\nbar");
                assert_eq!(s.replacement, "baz");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_substitute_literal_newline_in_replacement_via_strs() {
        let cmd = format!("{}s/foo/bar\nbaz/", addr(1, "foo"));
        let cmds = parse_commands_from_strs(&[&cmd]).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "foo");
                assert_eq!(s.replacement, "bar\nbaz");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_substitute_custom_delim_with_literal_newline() {
        let cmd = format!("{}s@foo\nbar@baz@", addr(1, "foo"));
        let cmds = parse_commands_from_strs(&[&cmd]).unwrap();
        match &cmds[0].cmd {
            Subcommand::Substitute(s) => {
                assert_eq!(s.pattern, "foo\nbar");
                assert_eq!(s.replacement, "baz");
            }
            _ => panic!("expected substitute"),
        }
    }

    #[test]
    fn parse_transliterate_custom_delimiter() {
        let cmd = format!("{}y@abc@ABC@", addr(1, "abc"));
        let cmds = parse_commands_from_script(&cmd).unwrap();
        match &cmds[0].cmd {
            Subcommand::Transliterate { source, dest } => {
                assert_eq!(source, "abc");
                assert_eq!(dest, "ABC");
            }
            _ => panic!("expected transliterate"),
        }
    }
}
