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

    // Only the modified line should be printed.
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!("{}  baz\n", format_lnhash(1, "baz"));
    assert_eq!(stdout, expected);

    assert_eq!(read_file(&file), "baz\nbar\n");
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
    let expected = format!("{}  baz\n", format_lnhash(1, "baz"));
    assert_eq!(stdout, expected);

    // File unchanged.
    assert_eq!(read_file(&file), "foo\nbar\n");
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

    // Two inserted lines should be printed.
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!(
        "{}  x\n{}  y\n",
        format_lnhash(2, "x"),
        format_lnhash(3, "y")
    );
    assert_eq!(stdout, expected);

    assert_eq!(read_file(&file), "a\nx\ny\n");
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
