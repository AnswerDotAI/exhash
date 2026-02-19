pub use exhash_core::*;

#[cfg(feature = "pyo3")]
mod python;

#[cfg(test)]
mod tests {
    use exhash_core::*;

    #[test]
    fn line_hash_returns_4_hex_chars() {
        let h = format!("{:04x}", line_hash_u16("hello"));
        assert_eq!(h.len(), 4);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn lnhash_format() {
        let addr = format_lnhash(1, "hello");
        assert!(addr.starts_with("1|"));
        assert!(addr.ends_with("|"));
    }

    #[test]
    fn lnhashview_lines() {
        let lines: Vec<String> = "a\nb\nc".lines()
            .enumerate()
            .map(|(i, l)| format!("{}  {}", format_lnhash(i + 1, l), l))
            .collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("  a"));
        assert!(lines[2].ends_with("  c"));
    }

    #[test]
    fn edit_script_substitute() {
        let text = "a\nb\nc\n";
        let a2 = format_lnhash(2, "b");
        let script = format!("{}s/b/B/\n", a2);
        let cmds = parse_commands_from_script(&script).unwrap();
        let res = edit_text(text, &cmds).unwrap();
        assert_eq!(res.lines.join("\n"), "a\nB\nc");
        assert_eq!(res.modified, vec![2]);
    }
}
