use std::fs;
use std::path::Path;

use exhash::{CommandField, FileError, LnHash, edit_cell, edit_files, format_hash, format_lnhash, parse_commands_from_script, parse_lnhash, view_cell};
use serde_json::Value;

fn fields(values: &[&str]) -> Vec<CommandField> { values.iter().map(|s| CommandField::Str((*s).into())).collect() }

#[test]
fn hash_roundtrip() {
    for hash in 0..4096 { assert_eq!(parse_lnhash(&format!("1|{}|", format_hash(hash))).unwrap(), LnHash { lineno: 1, hash }); }
    assert_eq!(format_hash(0), "AA");
    assert_eq!(format_hash(4095), "__");
    for bad in ["1|0000|", "1|A|", "1|AAA|", "1|++|", "1|//|", "1|é|", "1|A|A|"] { assert!(parse_lnhash(bad).is_err()); }
}

#[test]
fn file_transfer_preview_and_failure() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.txt").to_string_lossy().into_owned();
    let dst = dir.path().join("dst.txt").to_string_lossy().into_owned();
    fs::write(&src, "alpha\nbeta\n").unwrap();
    let addr = format_lnhash(1, "alpha");
    let cmds = vec![fields(&[&addr, "m", &format!("{dst}:0|AA|")])];
    let preview = edit_files(&src, &cmds, 4, false).unwrap();
    assert_eq!(preview[0].result.lines, ["beta"]);
    assert_eq!(preview[1].result.lines, ["alpha"]);
    assert!(!Path::new(&dst).exists());
    let mut bad = cmds.clone();
    bad.push(fields(&["1|6t|", "d"]));
    assert!(edit_files(&src, &bad, 4, true).is_err());
    assert_eq!(fs::read_to_string(&src).unwrap(), "alpha\nbeta\n");
    assert!(!Path::new(&dst).exists());
    edit_files(&src, &cmds, 4, true).unwrap();
    assert_eq!(fs::read_to_string(&src).unwrap(), "beta\n");
    assert_eq!(fs::read_to_string(&dst).unwrap(), "alpha\n");
}

#[test]
fn notebook_preserves_metadata_and_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nb.ipynb").to_string_lossy().into_owned();
    let input = r#"{"cells":[{"id":"aaaa","source":["x=1\n","y=2"],"outputs":[{"value":99999999999999999999999999999999999999}]}],"metadata":{"unicode":"世界","ratio":1.00}}"#;
    fs::write(&path, input).unwrap();
    let commands = parse_commands_from_script("%s/1/9/").unwrap();
    edit_cell(&path, "aa", &commands, 4, true).unwrap();
    let before: Value = serde_json::from_str(input).unwrap();
    let after: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(after["metadata"], before["metadata"]);
    assert_eq!(after["cells"][0]["outputs"], before["cells"][0]["outputs"]);
    assert_eq!(after["cells"][0]["source"], serde_json::json!(["x=9\n", "y=2"]));
    assert!(matches!(view_cell(&path, "missing", None, None), Err(FileError::Cell(_))));
}
