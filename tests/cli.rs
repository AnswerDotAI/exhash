use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use exhash::format_lnhash;

fn mk_temp_dir(name: &str) -> PathBuf {
    let mut dir = env::temp_dir();
    dir.push(format!("exhash-test-{}-{}", name, std::process::id()));
    // Best-effort cleanup from previous crashed runs.
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(path: &Path, contents: &str) {
    fs::write(path, contents.as_bytes()).unwrap();
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn ctx(lineno: usize, line: &str) -> String {
    format!(" {}  {}", format_lnhash(lineno, line), line)
}
fn add(lineno: usize, line: &str) -> String {
    format!("+{}  {}", format_lnhash(lineno, line), line)
}
fn del(lineno: usize, line: &str) -> String {
    format!("-{}  {}", format_lnhash(lineno, line), line)
}

#[test]
fn lnhashview_basic_and_range() {
    let dir = mk_temp_dir("lnhashview_basic");
    let file = dir.join("f.txt");
    write_file(&file, "alpha\nbeta\n\ngamma\n");

    let bin = env!("CARGO_BIN_EXE_lnhashview");

    // Full file
    let out = Command::new(bin).arg(&file).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected_lines = vec![
        format!("{}  alpha", format_lnhash(1, "alpha")),
        format!("{}  beta", format_lnhash(2, "beta")),
        format!("{}  ", format_lnhash(3, "")),
        format!("{}  gamma", format_lnhash(4, "gamma")),
    ];
    let expected = expected_lines.join("\n") + "\n";
    assert_eq!(stdout, expected);

    // Range 2..3
    let out = Command::new(bin)
        .arg(&file)
        .arg("2")
        .arg("3")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = vec![
        format!("{}  beta", format_lnhash(2, "beta")),
        format!("{}  ", format_lnhash(3, "")),
    ]
    .join("\n")
        + "\n";
    assert_eq!(stdout, expected);

    // End past EOF clamps to the last line.
    let out = Command::new(bin)
        .arg(&file)
        .arg("2")
        .arg("260")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = vec![
        format!("{}  beta", format_lnhash(2, "beta")),
        format!("{}  ", format_lnhash(3, "")),
        format!("{}  gamma", format_lnhash(4, "gamma")),
    ]
    .join("\n")
        + "\n";
    assert_eq!(stdout, expected);
}

#[test]
fn exhash_inplace_substitute_and_stdout_modified_only() {
    let dir = mk_temp_dir("exhash_subst");
    let file = dir.join("f.txt");
    write_file(&file, "foo\nbar\n");

    let a1 = format_lnhash(1, "foo");
    let cmd = format!("{}s/foo/baz/", a1);

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin).arg(&file).arg(cmd).output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!("{}\n{}\n{}\n", del(1, "foo"), add(1, "baz"), ctx(2, "bar"));
    assert_eq!(stdout, expected);

    assert_eq!(read_file(&file), "baz\nbar\n");
}

#[test]
fn exhash_inplace_transliterate_and_stdout_modified_only() {
    let dir = mk_temp_dir("exhash_translit");
    let file = dir.join("f.txt");
    write_file(&file, "abc\ncab\n");

    let a1 = format_lnhash(1, "abc");
    let cmd = format!("{}y/abc/ABC/", a1);

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin).arg(&file).arg(cmd).output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!("{}\n{}\n{}\n", del(1, "abc"), add(1, "ABC"), ctx(2, "cab"));
    assert_eq!(stdout, expected);

    assert_eq!(read_file(&file), "ABC\ncab\n");
}

#[test]
fn exhash_dry_run_does_not_write() {
    let dir = mk_temp_dir("exhash_dry_run");
    let file = dir.join("f.txt");
    write_file(&file, "foo\nbar\n");

    let a1 = format_lnhash(1, "foo");
    let cmd = format!("{}s/foo/baz/", a1);

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin)
        .arg("--dry-run")
        .arg(&file)
        .arg(cmd)
        .output()
        .unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!("{}\n{}\n{}\n", del(1, "foo"), add(1, "baz"), ctx(2, "bar"));
    assert_eq!(stdout, expected);

    // File unchanged.
    assert_eq!(read_file(&file), "foo\nbar\n");
}

#[test]
fn exhash_custom_sw_option_changes_shift_width() {
    let dir = mk_temp_dir("exhash_custom_sw");
    let file = dir.join("f.txt");
    write_file(&file, "a\n");

    let a1 = format_lnhash(1, "a");
    let cmd = format!("{a1}>1");

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin)
        .arg("--sw")
        .arg("2")
        .arg(&file)
        .arg(cmd)
        .output()
        .unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!("{}\n{}\n", del(1, "a"), add(1, "  a"));
    assert_eq!(stdout, expected);
    assert_eq!(read_file(&file), "  a\n");
}

#[test]
fn exhash_rejects_stale_lnhash_and_leaves_file_unchanged() {
    let dir = mk_temp_dir("exhash_stale");
    let file = dir.join("f.txt");
    write_file(&file, "hello\nworld\n");

    // Compute lnhash from the original content.
    let a1 = format_lnhash(1, "hello");
    let cmd = format!("{}d", a1);

    // Mutate the file so the lnhash is stale.
    write_file(&file, "HELLO\nworld\n");

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin).arg(&file).arg(cmd).output().unwrap();
    assert!(!out.status.success());

    // File unchanged by exhash.
    assert_eq!(read_file(&file), "HELLO\nworld\n");
}

#[test]
fn exhash_rechecks_hashes_between_commands() {
    let dir = mk_temp_dir("exhash_stale_between_commands");
    let file = dir.join("f.txt");
    write_file(&file, "a\nb\n");

    let a1 = format_lnhash(1, "a");
    let cmd1 = format!("{}s/a/A/", a1);
    let cmd2 = format!("{}d", a1);

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin)
        .arg(&file)
        .arg(cmd1)
        .arg(cmd2)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("stale lnhash"));

    // No partial write on command failure.
    assert_eq!(read_file(&file), "a\nb\n");
}

#[test]
fn exhash_percent_join_whole_file() {
    let dir = mk_temp_dir("exhash_percent_join");
    let file = dir.join("f.txt");
    write_file(&file, "a\nb\nc\n");

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin).arg(&file).arg("%j").output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    // Join marks line 1 as modified (content changed) and lines 2,3 as deleted
    assert!(stdout.contains(&add(1, "a b c")));
    assert!(stdout.contains(&del(2, "b")));
    assert!(stdout.contains(&del(3, "c")));
    assert_eq!(read_file(&file), "a b c\n");
}

#[test]
fn exhash_dollar_deletes_last_line() {
    let dir = mk_temp_dir("exhash_dollar_delete");
    let file = dir.join("f.txt");
    write_file(&file, "a\nb\nc\n");

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin).arg(&file).arg("$d").output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!("{}\n{}\n", ctx(2, "b"), del(3, "c"));
    assert_eq!(stdout, expected);
    assert_eq!(read_file(&file), "a\nb\n");
}

#[test]
fn exhash_move_to_last_line_destination() {
    let dir = mk_temp_dir("exhash_move_last_line_dest");
    let file = dir.join("f.txt");
    write_file(&file, "a\nb\nc\n");

    let a1 = format_lnhash(1, "a");
    let cmd = format!("{a1}m$");

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin).arg(&file).arg(cmd).output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    // Move marks the moved line as modified at its new position
    assert!(stdout.contains(&add(3, "a")));
    assert_eq!(read_file(&file), "b\nc\na\n");
}

#[test]
fn exhash_multiline_append_from_stdin() {
    let dir = mk_temp_dir("exhash_multiline");
    let file = dir.join("f.txt");
    write_file(&file, "a\n");

    let a1 = format_lnhash(1, "a");
    let cmd = format!("{}a", a1);

    let bin = env!("CARGO_BIN_EXE_exhash");
    let mut child = Command::new(bin)
        .arg(&file)
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"x\ny\n.\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains(&add(2, "x")));
    assert!(stdout.contains(&add(3, "y")));

    assert_eq!(read_file(&file), "a\nx\ny\n");
}

#[test]
fn exhash_creates_missing_file_with_zero_append() {
    let dir = mk_temp_dir("exhash_create_missing");
    let file = dir.join("new.txt");

    let bin = env!("CARGO_BIN_EXE_exhash");
    let mut child = Command::new(bin)
        .arg(&file)
        .arg("0|0000|a")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"first line\n.\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!("{}\n", add(1, "first line"));
    assert_eq!(stdout, expected);
    assert_eq!(read_file(&file), "first line\n");
}

#[test]
fn exhash_rejects_binary_file() {
    let dir = mk_temp_dir("exhash_binary");
    let file = dir.join("f.bin");
    fs::write(&file, b"a\0b\n").unwrap();

    let bin = env!("CARGO_BIN_EXE_exhash");
    let out = Command::new(bin).arg(&file).output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn exhash_stdin_mode_edits_and_prints_full_file() {
    let bin = env!("CARGO_BIN_EXE_exhash");

    let input = "foo\nbar\n";
    let a1 = format_lnhash(1, "foo");
    let cmd = format!("{}s/foo/baz/", a1);

    let mut child = Command::new(bin)
        .arg("--stdin")
        .arg("-")
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(input.as_bytes()).unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!(
        "{}  baz\n{}  bar\n",
        format_lnhash(1, "baz"),
        format_lnhash(2, "bar")
    );
    assert_eq!(stdout, expected);
}
