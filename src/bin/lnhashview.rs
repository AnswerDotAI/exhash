use std::env;
use std::fs;
use std::process;

use exhash::format_lnhash;

fn usage() {
    eprintln!(
        "Usage: lnhashview <file> [start_line [end_line]]\n\n\
         Prints lines as: <lineno>|<hash>|  <content>\n\
         start_line/end_line are 1-based inclusive."
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
        process::exit(2);
    }

    let file = &args[1];
    let start_opt = args.get(2).map(|s| s.parse::<usize>());
    let end_opt = args.get(3).map(|s| s.parse::<usize>());

    if args.len() > 4 {
        usage();
        process::exit(2);
    }

    let start = match start_opt {
        None => None,
        Some(Ok(v)) => Some(v),
        Some(Err(_)) => {
            eprintln!("error: start_line must be an integer");
            process::exit(2);
        }
    };

    let end = match end_opt {
        None => None,
        Some(Ok(v)) => Some(v),
        Some(Err(_)) => {
            eprintln!("error: end_line must be an integer");
            process::exit(2);
        }
    };

    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: failed to read {file}: {e}");
            process::exit(1);
        }
    };

    if bytes.iter().any(|&b| b == 0) {
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

    let lines: Vec<&str> = text.lines().collect();

    if lines.is_empty() {
        return;
    }

    let (start_line, end_line) = match (start, end) {
        (None, None) => (1, lines.len()),
        (Some(s), None) => (s, s),
        (Some(s), Some(e)) => (s, e),
        (None, Some(_)) => {
            eprintln!("error: end_line requires start_line");
            process::exit(2);
        }
    };

    if start_line == 0 {
        eprintln!("error: start_line is 1-based (must be >= 1)");
        process::exit(2);
    }

    if end_line < start_line {
        eprintln!("error: end_line must be >= start_line");
        process::exit(2);
    }

    if end_line > lines.len() {
        eprintln!(
            "error: end_line {end_line} is beyond EOF (file has {} line(s))",
            lines.len()
        );
        process::exit(2);
    }

    for (idx, line) in lines
        .iter()
        .enumerate()
        .skip(start_line - 1)
        .take(end_line - start_line + 1)
    {
        let lineno = idx + 1;
        let lnh = format_lnhash(lineno, line);
        println!("{lnh}  {line}");
    }
}
