use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::EditError;

/// A verified line address: a 1-based line number paired with a short content hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LnHash {
    pub lineno: usize,
    pub hash: u16,
}

/// Compute the 16-bit lnhash of a line's content.
///
/// The hash is the low 16 bits of `std::collections::hash_map::DefaultHasher` (SipHash-1-3)
/// over the UTF-8 line content (excluding the line ending).
pub fn line_hash_u16(line: &str) -> u16 {
    let mut h = DefaultHasher::new();
    line.hash(&mut h);
    (h.finish() & 0xffff) as u16
}

/// Format a line address as `lineno|hash|`.
pub fn format_lnhash(lineno: usize, line: &str) -> String {
    format!("{}|{:04x}|", lineno, line_hash_u16(line))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lnhash_roundtrip() {
        let line = "hello world";
        let addr = format_lnhash(12, line);
        assert!(addr.starts_with("12|"));
        assert!(addr.ends_with('|'));
        let parsed = parse_lnhash(&addr).unwrap();
        assert_eq!(parsed.lineno, 12);
        assert_eq!(parsed.hash, line_hash_u16(line));
    }

    #[test]
    fn parse_prefix_returns_suffix() {
        let (lh, rest) = parse_lnhash_prefix("3|00ff|d").unwrap();
        assert_eq!(lh.lineno, 3);
        assert_eq!(lh.hash, 0x00ff);
        assert_eq!(rest, "d");
    }
}
