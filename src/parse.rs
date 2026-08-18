use std::io::BufRead;

use crate::EditError;
use crate::lnhash::{LnHash, parse_lnhash_prefix};

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
pub fn parse_commands_from_args(args: &[String], stdin: &mut impl BufRead) -> Result<Vec<Command>, EditError> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        let cmd = parse_command_with_text(a, || read_text_block_from_bufread(stdin))?;
        out.push(cmd);
    }
    Ok(out)
}

pub fn split_text_payload(text: &str) -> Vec<String> {
    text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line).to_string()).collect()
}

/// Parse commands from an ex-style script string.
///
/// Commands are separated by newlines. For `a`/`i`/`c` (and for global subcommands
/// that are `a`/`i`/`c`), the following lines up to a `.` line (dot on its own line)
/// are taken as the text block.
pub fn parse_commands_from_script(script: &str) -> Result<Vec<Command>, EditError> {
    let mut lines = script.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).peekable();

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
    let (addr1, addr2, has_comma, rest) = parse_addresses(line)?;
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Err(EditError::new("missing command"));
    }

    let (cmd, trailing) = parse_subcommand_with_text(rest, &mut read_text)?;

    // No trailing junk for a top-level command.
    if !trailing.trim().is_empty() {
        return Err(EditError::new(format!("unexpected trailing characters: {:?}", trailing)));
    }

    build_command(addr1, addr2, has_comma, cmd)
}

/// Parse `addr[,addr]` from the start of `input`, returning the remainder.
fn parse_addresses(input: &str) -> Result<(Address, Option<Address>, bool, &str), EditError> {
    let (addr1, mut rest) = parse_address_prefix(input.trim_start())?;
    let mut has_comma = false;
    let mut addr2: Option<Address> = None;

    if rest.starts_with(',') {
        has_comma = true;
        let (a2, r2) = parse_address_prefix(&rest[1..])?;
        addr2 = Some(a2);
        rest = r2;
    }
    Ok((addr1, addr2, has_comma, rest))
}

/// Build a command from an address string and an already-built subcommand (the tuple form).
pub fn command_from_parts(addr: &str, cmd: Subcommand) -> Result<Command, EditError> {
    let (addr1, addr2, has_comma, rest) = parse_addresses(addr)?;
    if !rest.trim().is_empty() {
        return Err(EditError::new(format!("unexpected trailing characters in address: {:?}", rest)));
    }
    build_command(addr1, addr2, has_comma, cmd)
}

/// Validate address/command combinations and assemble the `Command`.
fn build_command(addr1: Address, addr2: Option<Address>, has_comma: bool, cmd: Subcommand) -> Result<Command, EditError> {
    if matches!(addr1, Address::WholeFile) && (has_comma || addr2.is_some()) {
        return Err(EditError::new("% is already a whole-file range"));
    }
    if matches!(addr2, Some(Address::WholeFile)) {
        return Err(EditError::new("% is only allowed as a standalone address"));
    }

    // Enforce 0|0000| rules.
    if let Address::LnHash(a1) = addr1
        && a1.lineno == 0
    {
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
    if let Some(Address::LnHash(a2)) = addr2
        && (a2.lineno == 0 || matches!(addr1, Address::LnHash(LnHash { lineno: 0, .. })))
    {
        return Err(EditError::new("0|0000| is not allowed in ranges"));
    }

    Ok(Command { addr1, addr2, has_comma, cmd })
}

fn parse_address_prefix(input: &str) -> Result<(Address, &str), EditError> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix('$') {
        return Ok((Address::LastLine, rest));
    }
    if let Some(rest) = input.strip_prefix('%') {
        return Ok((Address::WholeFile, rest));
    }
    let (lh, rest) = parse_lnhash_prefix(input)?;
    Ok((Address::LnHash(lh), rest))
}

pub fn parse_destination_address(input: &str, op: char) -> Result<Address, EditError> {
    parse_destination_address_inner(input, op, false)
}

pub(crate) fn parse_buffer_destination_address(input: &str, op: char) -> Result<Address, EditError> {
    parse_destination_address_inner(input, op, true)
}

fn parse_destination_address_inner(input: &str, op: char, allow_zero: bool) -> Result<Address, EditError> {
    let (addr, rest) = parse_address_prefix(input)?;
    if !rest.trim().is_empty() {
        return Err(EditError::new(format!("unexpected trailing characters after destination: {:?}", rest)));
    }
    match addr {
        Address::LnHash(LnHash { lineno: 0, hash }) if hash != 0 => Err(EditError::new("0|0000| must have hash 0000")),
        Address::LnHash(LnHash { lineno: 0, .. }) if !allow_zero => {
            Err(EditError::new(format!("destination 0|0000| is not allowed for {op}")))
        }
        Address::WholeFile => Err(EditError::new(format!("destination % is not allowed for {op}"))),
        _ => Ok(addr),
    }
}

fn parse_subcommand_with_text<'a, F>(input: &'a str, read_text: &mut F) -> Result<(Subcommand, &'a str), EditError>
where
    F: FnMut() -> Result<Vec<String>, EditError>,
{
    let s = input.trim_start();
    if let Some(trailing) = s.strip_prefix("sort") {
        return Ok((Subcommand::Sort, trailing));
    }

    // g! must be checked before g
    if let Some(rest) = s.strip_prefix("g!") {
        return parse_global(rest, true, read_text);
    }

    let mut chars = s.chars();
    let c = chars.next().ok_or_else(|| EditError::new("missing command"))?;
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
fn parse_text_command<'a, F>(rest: &'a str, read_text: &mut F) -> Result<(Vec<String>, &'a str), EditError>
where
    F: FnMut() -> Result<Vec<String>, EditError>,
{
    if rest.is_empty() {
        Ok((read_text()?, ""))
    } else if rest.contains('\n') {
        Ok((split_text_payload(rest), ""))
    } else {
        Ok((vec![rest.to_string()], ""))
    }
}

pub fn parse_optional_usize(s: &str) -> Result<usize, EditError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(1);
    }
    s.parse::<usize>().map_err(|_| EditError::new(format!("invalid number: {s:?}")))
}

fn parse_global<'a, F>(rest: &'a str, invert: bool, read_text: &mut F) -> Result<(Subcommand, &'a str), EditError>
where
    F: FnMut() -> Result<Vec<String>, EditError>,
{
    let rest = rest.trim_start();
    let delim = rest.chars().next().ok_or_else(|| EditError::new("global requires <delim>pat<delim>cmd"))?;
    if delim.is_alphanumeric() || delim == '\\' {
        return Err(EditError::new("global delimiter must not be alphanumeric or backslash"));
    }
    let (pat, after_pat) = parse_delimited(rest, delim)?;
    let cmd_str = after_pat.trim_start();
    if cmd_str.is_empty() {
        return Err(EditError::new("global requires a subcommand"));
    }
    let (subcmd, trailing) = parse_subcommand_with_text(cmd_str, read_text)?;
    if !trailing.trim().is_empty() {
        return Err(EditError::new(format!("unexpected trailing characters in global subcommand: {:?}", trailing)));
    }
    Ok((Subcommand::Global { invert, pattern: pat, cmd: Box::new(subcmd) }, ""))
}

fn parse_substitute(rest: &str) -> Result<(Subst, &str), EditError> {
    let rest = rest.trim_start();
    let delim = rest.chars().next().ok_or_else(|| EditError::new("substitute requires <delim>pat<delim>rep<delim>[flags]"))?;
    if delim.is_alphanumeric() || delim == '\\' {
        return Err(EditError::new("substitute delimiter must not be alphanumeric or backslash"));
    }

    let (pat, after_pat) = parse_delimited(rest, delim)?;
    let (rep, after_rep) = scan_to_delim(after_pat, delim)?;

    Ok((subst_from_parts(pat, rep, after_rep)?, ""))
}

/// Validate substitute fields and flags (shared by the compact and tuple forms).
pub fn subst_from_parts(pattern: String, replacement: String, flags: &str) -> Result<Subst, EditError> {
    let mut global = false;
    let mut case_insensitive = false;

    for ch in flags.trim().chars() {
        match ch {
            'g' => global = true,
            'i' => case_insensitive = true,
            _ => return Err(EditError::new(format!("unknown substitute flag: {ch}"))),
        }
    }

    if pattern.is_empty() {
        return Err(EditError::new("substitute pattern may not be empty"));
    }

    Ok(Subst { pattern, replacement, global, case_insensitive })
}

fn parse_transliterate(rest: &str) -> Result<((String, String), &str), EditError> {
    let rest = rest.trim_start();
    let delim = rest.chars().next().ok_or_else(|| EditError::new("transliterate requires <delim>source<delim>dest<delim>"))?;
    if delim.is_alphanumeric() || delim == '\\' {
        return Err(EditError::new("transliterate delimiter must not be alphanumeric or backslash"));
    }

    let (source, after_source) = parse_delimited(rest, delim)?;
    let (dest, trailing) = scan_to_delim(after_source, delim)?;

    Ok((translit_from_parts(source, dest)?, trailing))
}

/// Validate transliterate source/dest fields (shared by the compact and tuple forms).
pub fn translit_from_parts(source: String, dest: String) -> Result<(String, String), EditError> {
    if source.chars().count() != dest.chars().count() {
        return Err(EditError::new("transliterate source and destination must have the same number of characters"));
    }
    Ok((source, dest))
}

/// Parse a `/.../` delimited string from the start of `input`.
///
/// Returns (decoded, rest_after_closing_delim).
fn parse_delimited(input: &str, delim: char) -> Result<(String, &str), EditError> {
    let mut chars = input.chars();
    let first = chars.next().ok_or_else(|| EditError::new("missing delimiter"))?;
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
fn scan_to_delim(input: &str, delim: char) -> Result<(String, &str), EditError> {
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
        let n = stdin.read_line(&mut buf).map_err(|e| EditError::new(format!("failed to read stdin: {e}")))?;
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

fn read_text_block_from_iter<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<Vec<String>, EditError> {
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
