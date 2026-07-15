
use crate::EditError;

/// A verified line address: a 1-based line number paired with a short content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LnHash {
    pub lineno: usize,
    pub hash: u16,
}

/// Compute the 16-bit lnhash of a line's content.
///
/// The hash is the low 16 bits of CRC-32 (IEEE) over the UTF-8 line content
/// (excluding the line ending), matching Python's `zlib.crc32(line) & 0xffff`.
pub fn line_hash_u16(line: &str) -> u16 {
    (crc32fast::hash(line.as_bytes()) & 0xffff) as u16
}

/// Format a line address as `lineno|hash|`.
pub fn format_lnhash(lineno: usize, line: &str) -> String {
    format!("{}|{:04x}|", lineno, line_hash_u16(line))
}

/// Format lines as `lineno|hash|content`, with line numbers space-padded to align the shown range.
/// `start` and `end` are 1-based inclusive. Pass `None` for defaults (1 and len).
/// `end` past EOF is clamped to the last line.
/// Returns an error if start is 0, end < start, or start is beyond EOF.
pub fn lnhashview(
    lines: &[&str],
    start: Option<usize>,
    end: Option<usize>,
) -> Result<Vec<String>, EditError> {
    if lines.is_empty() {
        return Ok(vec![]);
    }
    let s = start.unwrap_or(1);
    let requested_e = end.unwrap_or(lines.len());
    if s == 0 {
        return Err(EditError::new("start_line is 1-based (must be >= 1)"));
    }
    if requested_e < s {
        return Err(EditError::new("end_line must be >= start_line"));
    }
    if s > lines.len() {
        return Err(EditError::new(format!(
            "start_line {} is beyond EOF (file has {} line(s))",
            s,
            lines.len()
        )));
    }
    let e = requested_e.min(lines.len());
    let width = e.to_string().len();
    Ok(lines
        .iter()
        .enumerate()
        .skip(s - 1)
        .take(e - s + 1)
        .map(|(i, l)| {
            format!(
                "{:>width$}|{:04x}|{}",
                i + 1,
                line_hash_u16(l),
                l,
                width = width
            )
        })
        .collect())
}

/// Parse a `lineno|hash|` address.
pub fn parse_lnhash(s: &str) -> Result<LnHash, EditError> {
    let (lh, rest) = parse_lnhash_prefix(s)?;
    if !rest.is_empty() {
        return Err(EditError::new(format!(
            "invalid lnhash: trailing characters after address: {:?}",
            rest
        )));
    }
    Ok(lh)
}

/// Parse a `lineno|hash|` from the start of `input`, returning the address and the remaining suffix.
pub fn parse_lnhash_prefix(input: &str) -> Result<(LnHash, &str), EditError> {
    let mut it = input.splitn(2, '|');
    let lineno_str = it
        .next()
        .ok_or_else(|| EditError::new("invalid lnhash: missing line number"))?;
    let rest = it
        .next()
        .ok_or_else(|| EditError::new("invalid lnhash: missing '|' after line number"))?;

    if lineno_str.is_empty() {
        return Err(EditError::new("invalid lnhash: empty line number"));
    }
    let lineno: usize = lineno_str
        .parse()
        .map_err(|_| EditError::new(format!("invalid lnhash: bad line number: {lineno_str:?}")))?;

    // Now parse hash|suffix
    let mut it2 = rest.splitn(2, '|');
    let hash_str = it2
        .next()
        .ok_or_else(|| EditError::new("invalid lnhash: missing hash"))?;
    let suffix = it2
        .next()
        .ok_or_else(|| EditError::new("invalid lnhash: missing trailing '|' after hash"))?;

    if hash_str.len() != 4 {
        return Err(EditError::new(format!(
            "invalid lnhash: hash must be 4 hex chars, got {hash_str:?}"
        )));
    }

    let hash = u16::from_str_radix(hash_str, 16)
        .map_err(|_| EditError::new(format!("invalid lnhash: bad hash: {hash_str:?}")))?;

    Ok((LnHash { lineno, hash }, suffix))
}
