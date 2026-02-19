use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process;

use exhash::{edit_text, parse_commands_from_args};

fn usage() {
    eprintln!(
        "Usage: exhash [--dry-run] [--stdin] <file|-> [commands...]\n\n\
         Default mode edits <file> in-place.\n\
         - Commands are passed as separate argv tokens.\n\
         - Multiline text for a/i/c is read from stdin until a line with just '.'\n\
           (as in ex/ed).\n\n\
         With --dry-run, no file is written; stdout shows what would change.\n\
         With --stdin, <file> must be '-' and input is read from stdin;\n\
         output is the entire edited file in lnhash format.\n"
    );
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().any(|&b| b == 0)
}

fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    let perms = fs::metadata(path).map(|m| m.permissions()).ok();

    let pid = process::id();
    let mut attempt: u64 = 0;
    let tmp_path: PathBuf;
    loop {
        let candidate = dir.join(format!(".{file_name}.exhash.tmp.{pid}.{attempt}"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(content.as_bytes())?;
                f.sync_all()?;
                if let Some(p) = perms.clone() {
                    let _ = fs::set_permissions(&candidate, p);
                }
                tmp_path = candidate;
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                attempt += 1;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dry_run = false;
    let mut stdin_mode = false;

    let mut idx = 1;
    while idx < args.len() {
        match args[idx].as_str() {
            "--dry-run" => {
                dry_run = true;
                idx += 1;
            }
            "--stdin" => {
                stdin_mode = true;
                idx += 1;
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            s if s.starts_with('-') && s.len() > 1 => {
                eprintln!("error: unknown flag {s}");
                usage();
                process::exit(2);
            }
            _ => break,
        }
    }

    if idx >= args.len() {
        usage();
        process::exit(2);
    }

    let file = args[idx].clone();
    idx += 1;

    let cmd_args: Vec<String> = args[idx..].iter().cloned().collect();

    if stdin_mode {
        if file != "-" {
            eprintln!("error: with --stdin, file must be '-' (got '{file}')");
            process::exit(2);
        }

        let mut input = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut input) {
            eprintln!("error: failed to read stdin: {e}");
            process::exit(1);
        }

        // In --stdin mode, stdin is consumed by the input. We therefore parse
        // commands with an empty text stream; a/i/c will fail with a clear error.
        let mut empty = io::Cursor::new("");
        let commands = match parse_commands_from_args(&cmd_args, &mut empty) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {e}");
                eprintln!("note: commands requiring text blocks (a/i/c) are not supported with --stdin");
                process::exit(2);
            }
        };

        let result = match edit_text(&input, &commands) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                process::exit(2);
            }
        };

        for (h, line) in result.hashes.iter().zip(result.lines.iter()) {
            println!("{h}  {line}");
        }

        return;
    }

    // File mode.
    let bytes = match fs::read(&file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: failed to read {file}: {e}");
            process::exit(1);
        }
    };

    if is_binary(&bytes) {
        eprintln!("error: binary file rejected (NUL byte found)");
        process::exit(1);
    }

    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("error: non-UTF8 file rejected");
            process::exit(1);
        }
    };

    let mut stdin = io::stdin().lock();
    let commands = match parse_commands_from_args(&cmd_args, &mut stdin) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(2);
        }
    };

    let result = match edit_text(&text, &commands) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(2);
        }
    };

    let new_text = if result.lines.is_empty() {
        String::new()
    } else {
        let mut s = result.lines.join("\n");
        s.push('\n');
        s
    };

    if !dry_run {
        if let Err(e) = write_atomic(Path::new(&file), &new_text) {
            eprintln!("error: failed to write {file}: {e}");
            process::exit(1);
        }
    }

    for lineno in &result.modified {
        let i = lineno - 1;
        if let (Some(h), Some(line)) = (result.hashes.get(i), result.lines.get(i)) {
            println!("{h}  {line}");
        }
    }
}
