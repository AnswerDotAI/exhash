use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process;

use exhash::{edit_text, parse_commands_from_args};

fn usage() {
    eprintln!("\
Usage: exhash [-h] [--dry-run] [--stdin] <file|-> [commands...]

Verified line-addressed file editor using lnhash addresses.

ADDRESSING
  Commands use lnhash addresses: lineno|hash| where hash is a 4-char
  hex content hash. Use lnhashview to get addresses:
    lnhashview file.txt          show all lines with addresses
    lnhashview file.txt 10 20    show lines 10-20

  Single:   12|a3f2|cmd
  Range:    12|a3f2|,15|b1c3|cmd
  Special:  0|0000| targets before line 1 (only with a or i)

COMMANDS
  s/pat/rep/[flags]  Substitute (regex). Flags: g=all, i=case-insensitive
  d                  Delete line(s)
  a                  Append text after line (reads text block)
  i                  Insert text before line (reads text block)
  c                  Change/replace line(s) with text block
  j                  Join with next line; with range, joins all lines in range
  m dest             Move line(s) after dest address
  t dest             Copy line(s) after dest address
  >[n]               Indent n levels (default 1, 4 spaces each)
  <[n]               Dedent n levels (default 1)
  sort               Sort lines alphabetically
  p                  Print (include lines in output without changing them)
  g/pat/cmd          Global: run cmd on matching lines
  g!/pat/cmd         Inverted global: run cmd on non-matching lines
  v/pat/cmd          Same as g!

TEXT BLOCKS (a/i/c)
  Text is read from stdin, terminated by a line containing just '.'
  Use '..' to insert a literal '.' line.

OPTIONS
  --dry-run  Don't write; show what would change on stdout
  --stdin    Read input from stdin (file arg must be '-');
             outputs full file in lnhash format.
             Text blocks (a/i/c) not supported in this mode.
  -h, --help Show this help

OUTPUT
  Modified/added lines are printed as: hash  content

EXAMPLES
  lnhashview file.txt
  exhash file.txt '12|abcd|s/foo/bar/g'
  exhash file.txt '2|beef|,4|cafe|d'
  printf 'line1\\nline2\\n.\\n' | exhash file.txt '5|d1e2|a'
  exhash file.txt '0|0000|i' <<< $'header\\n.'
  exhash file.txt '2|aa|,3|bb|m5|cc|'
  exhash file.txt '1|ab|,10|ef|g/TODO/d'
  exhash --dry-run file.txt '3|1234|s/old/new/'
  cat file.txt | exhash --stdin - '1|abcd|s/foo/bar/'
");
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
