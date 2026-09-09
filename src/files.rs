//! File and notebook orchestration, independent of Python. All commands are
//! validated/applied in memory before any writes; this is not a multi-file transaction.
use crate::commands::buffer_command_from_fields;
use crate::{BufferCommand, Command, CommandField, EditError, EditResult, edit_buffers_with_sw, edit_text_with_sw};
use regex::Regex;
use serde_json::Value;
use std::{collections::BTreeMap, fs, io, path::Path, sync::LazyLock};

#[derive(Debug)]
pub enum FileError {
    Io(io::Error),
    Edit(EditError),
    Cell(String),
    Json(serde_json::Error),
}
impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Edit(e) => e.fmt(f),
            Self::Cell(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for FileError {}
impl From<io::Error> for FileError { fn from(e: io::Error) -> Self { Self::Io(e) } }
impl From<EditError> for FileError { fn from(e: EditError) -> Self { Self::Edit(e) } }
impl From<serde_json::Error> for FileError { fn from(e: serde_json::Error) -> Self { Self::Json(e) } }
type Result<T> = std::result::Result<T, FileError>;
fn invalid(message: impl Into<String>) -> FileError { EditError::new(message).into() }

/// Expand a current-user home prefix and normalize redundant separators/dots,
/// without resolving symlinks or making a relative path absolute.
pub fn normalize_path(path: &str) -> Result<String> {
    let expanded = if path == "~" || path.starts_with("~/") {
        let home = std::env::var("HOME").map_err(|_| invalid("cannot expand ~: HOME is not set"))?;
        format!("{home}{}", &path[1..])
    } else { path.to_owned() };
    let normalized: std::path::PathBuf = Path::new(&expanded).components().filter(|c| !matches!(c, std::path::Component::CurDir)).collect();
    Ok(if normalized.as_os_str().is_empty() { ".".into() } else { normalized.to_string_lossy().into_owned() })
}

fn unexpanded(path: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{[A-Za-z_]\w*\}|\$\{?[A-Za-z_]\w*\}?").unwrap());
    RE.find(path)
        .map(|m| format!(" -- note: {:?} looks like an unexpanded IPython variable (undefined names in a magic line are passed through literally)", m.as_str()))
        .unwrap_or_default()
}
fn missing(message: String) -> FileError { io::Error::new(io::ErrorKind::NotFound, message).into() }

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target { path: String, cell: Option<String> }
impl Target { fn key(&self) -> String { match &self.cell { Some(id) => format!("{}:{id}", self.path), None => self.path.clone() } } }
fn unescape(path: &str) -> String {
    let mut out = String::new();
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        if let Some(next) = chars.next() {
            if next != ':' && next != '\\' { out.push('\\'); }
            out.push(next);
        } else { out.push('\\'); }
    }
    out
}
fn address(input: &str, default: &Target) -> Result<(Target, String, String)> {
    static ADDR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:\$|%|\d+\|[0-9a-fA-F]{4}\|)").unwrap());
    static CELL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(.*\.ipynb):([A-Za-z0-9_-]+)$").unwrap());
    let input = input.trim_start();
    let mut target = default.clone();
    let mut rest = input;
    if !ADDR.is_match(input) {
        let mut escaped = false;
        for (i, c) in input.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c != ':' || !ADDR.is_match(&input[i + 1..]) { continue; }
            if i == 0 { return Err(invalid("empty filename prefix")); }
            let path = normalize_path(&unescape(&input[..i]))?;
            target = match CELL.captures(&path) { Some(m) => Target { path: m[1].into(), cell: Some(m[2].into()) }, None => Target { path, cell: None } };
            rest = &input[i + 1..];
            break;
        }
    }
    let m = ADDR.find(rest).ok_or_else(|| invalid(format!("expected exhash address near {:?}", input.chars().take(40).collect::<String>())))?;
    Ok((target, m.as_str().into(), rest[m.end()..].into()))
}

/// Resolve a cell by exact ID or unique prefix. Return its index, not a copy.
fn cell_index(nb: &Value, id: &str, path: &str) -> Result<usize> {
    let cells = nb.get("cells").and_then(Value::as_array).ok_or_else(|| invalid("notebook cells must be an array"))?;
    let matches: Vec<_> = cells.iter().enumerate().filter(|(_, c)| c.get("id").and_then(Value::as_str).unwrap_or("").starts_with(id)).collect();
    if let Some((i, _)) = matches.iter().find(|(_, c)| c.get("id").and_then(Value::as_str) == Some(id)) { return Ok(*i); }
    match matches.as_slice() {
        [(i, _)] => Ok(*i),
        [] => Err(FileError::Cell(format!("no cell with id {id:?} in {path}"))),
        _ => Err(FileError::Cell(format!("cell id prefix {id:?} is ambiguous in {path}"))),
    }
}
fn cell_text(cell: &Value) -> Result<String> {
    match cell.get("source") {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Array(lines)) => {
            lines.iter().map(|v| v.as_str().ok_or_else(|| invalid("cell source must contain strings"))).collect::<Result<Vec<_>>>().map(|s| s.concat())
        }
        _ => Err(invalid("cell source must be a string or array of strings")),
    }
}
fn read_notebook(path: &str) -> Result<Value> {
    let text = fs::read_to_string(path)
        .map_err(|e| if e.kind() == io::ErrorKind::NotFound { missing(format!("notebook not found: {path}{}", unexpanded(path))) } else { e.into() })?;
    Ok(serde_json::from_str(&text)?)
}
fn set_source(cell: &mut Value, text: String) {
    cell["source"] =
        if cell["source"].is_array() { Value::Array(text.split_inclusive('\n').map(|s| Value::String(s.into())).collect()) } else { Value::String(text) };
}
fn notebook_text(nb: &Value) -> Result<Vec<u8>> {
    // Match Jupyter/Python's sorted, one-space-indented, UTF-8 JSON layout.
    let mut sorted = nb.clone();
    sorted.sort_all_objects();
    let mut out = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b" "));
    serde::Serialize::serialize(&sorted, &mut serializer)?;
    out.push(b'\n');
    Ok(out)
}
fn output_text(lines: &[String], trailing_newline: bool) -> String {
    let mut text = lines.join("\n");
    if trailing_newline && !lines.is_empty() { text.push('\n'); }
    text
}

// Match Python's universal-newline file reads without splitting Unicode content.
fn read_text(path: impl AsRef<Path>) -> io::Result<String> { fs::read_to_string(path).map(|s| s.replace("\r\n", "\n").replace('\r', "\n")) }

pub fn view_file(path: &str, start: Option<usize>, end: Option<usize>) -> Result<Vec<String>> {
    let text = read_text(normalize_path(path)?)?;
    Ok(crate::lnhashview(&text.lines().collect::<Vec<_>>(), start, end)?)
}
pub fn view_cell(path: &str, id: &str, start: Option<usize>, end: Option<usize>) -> Result<Vec<String>> {
    let path = normalize_path(path)?;
    let nb = read_notebook(&path)?;
    let text = cell_text(&nb["cells"][cell_index(&nb, id, &path)?])?;
    Ok(crate::lnhashview(&text.lines().collect::<Vec<_>>(), start, end)?)
}
pub fn view_cells(path: &str, ids: &[String], start: Option<usize>, end: Option<usize>) -> Result<Vec<String>> {
    if ids.is_empty() { return Ok(vec![]); }
    let path = normalize_path(path)?;
    let nb = read_notebook(&path)?;
    let mut out = vec![];
    for id in ids {
        let cell = &nb["cells"][cell_index(&nb, id, &path)?];
        out.push(format!("# cell {}", cell["id"].as_str().unwrap_or(id)));
        out.extend(crate::lnhashview(&cell_text(cell)?.lines().collect::<Vec<_>>(), start, end)?);
    }
    Ok(out)
}

/// The edited state of one file or cell. `path` is its resolved target key.
#[derive(Debug)]
pub struct FileEdit {
    pub path: String,
    pub cell: Option<String>,
    pub original_text: String,
    pub result: EditResult,
}
impl FileEdit {
    pub fn changed(&self) -> bool { self.original_text.lines().ne(self.result.lines.iter().map(String::as_str)) }
    pub fn format_diff(&self, context: usize) -> String {
        let diff = self.result.format_diff(&self.original_text.lines().collect::<Vec<_>>(), context);
        if self.changed() { diff.replacen("--- original\n+++ modified\n", &format!("--- {}\n+++ {}\n", self.path, self.path), 1) } else { diff }
    }
}
struct Buffer { target: Target, text: String, cell_index: Option<usize> }
#[derive(Default)]
struct Files { buffers: Vec<Buffer>, notebooks: BTreeMap<String, Value> }
impl Files {
    fn load(&mut self, mut target: Target, missing_ok: bool) -> Result<String> {
        let (text, index) = if let Some(id) = &target.cell {
            if !self.notebooks.contains_key(&target.path) { self.notebooks.insert(target.path.clone(), read_notebook(&target.path)?); }
            let nb = &self.notebooks[&target.path];
            let index = cell_index(nb, id, &target.path)?;
            let cell = &nb["cells"][index];
            target.cell = Some(cell["id"].as_str().ok_or_else(|| invalid("cell id must be a string"))?.into());
            (cell_text(cell)?, Some(index))
        } else {
            if self.buffers.iter().any(|b| b.target == target) { return Ok(target.key()); }
            let text = match read_text(&target.path) {
                Ok(text) => text,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    let path = &target.path;
                    if !missing_ok {
                        return Err(missing(format!("file not found: {path}{} (a new file can only be created with a 0|0000| a/i command)", unexpanded(path))));
                    }
                    let parent = Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
                    if !parent.exists() {
                        return Err(missing(format!("cannot create {path}: parent directory {} does not exist{}", parent.display(), unexpanded(path))));
                    }
                    String::new()
                }
                Err(e) => return Err(e.into()),
            };
            (text, None)
        };
        let key = target.key();
        if !self.buffers.iter().any(|b| b.target == target) { self.buffers.push(Buffer { target, text, cell_index: index }); }
        Ok(key)
    }
}

/// Apply structured file-aware commands, optionally writing changed files/cells.
/// Cell ID prefixes are resolved once; each notebook is read/written only once.
pub fn edit_files(path: &str, commands: &[Vec<CommandField>], sw: usize, inplace: bool) -> Result<Vec<FileEdit>> {
    let default = Target { path: normalize_path(path)?, cell: None };
    let mut files = Files::default();
    let mut parsed = vec![];
    for fields in commands {
        let [CommandField::Str(addr), CommandField::Str(op), rest @ ..] = fields.as_slice() else {
            return Err(invalid("command must start with (address, op) strings"));
        };
        let (src, addr1, tail) = address(addr, &default)?;
        let (local_addr, tail) = if let Some(tail) = tail.strip_prefix(',') {
            let (src2, addr2, tail) = address(tail, &src)?;
            if src != src2 { return Err(invalid("a range must stay within one file or cell")); }
            (format!("{addr1},{addr2}"), tail)
        } else { (addr1.clone(), tail) };
        if !tail.trim().is_empty() { return Err(invalid(format!("unexpected trailing characters in address: {tail:?}"))); }
        let mut local = vec![CommandField::Str(local_addr), CommandField::Str(op.clone())];
        let (dest, dest_addr) = if matches!(op.as_str(), "m" | "t") {
            let [CommandField::Str(dest)] = rest else { return Err(invalid("m/t requires one destination address")); };
            let (dest, dest_addr, tail) = address(dest, &src)?;
            if !tail.trim().is_empty() { return Err(invalid(format!("unexpected trailing characters after destination: {tail:?}"))); }
            local.push(CommandField::Str(dest_addr.clone()));
            (Some(dest), Some(dest_addr))
        } else {
            local.extend_from_slice(rest);
            (None, None)
        };
        let command = buffer_command_from_fields(&local)?;
        let target = files.load(src, addr1 == "0|0000|" && matches!(op.as_str(), "a" | "i"))?;
        let destination = dest.map(|d| files.load(d, dest_addr.as_deref() == Some("0|0000|"))).transpose()?;
        parsed.push(BufferCommand { target, command, destination });
    }
    if files.buffers.is_empty() { files.load(default, false)?; }
    let buffers = files.buffers.iter().map(|b| (b.target.key(), output_text(&b.text.lines().map(str::to_owned).collect::<Vec<_>>(), true))).collect();
    let results = edit_buffers_with_sw(buffers, parsed, sw)?;
    let mut out = vec![];
    let mut changed_notebooks = std::collections::BTreeSet::new();
    for (buffer, edited) in files.buffers.iter().zip(results) {
        let result = FileEdit { path: edited.target, cell: buffer.target.cell.clone(), original_text: edited.original_text, result: edited.result };
        if inplace && result.changed() {
            if let Some(index) = buffer.cell_index {
                let cell = &mut files.notebooks.get_mut(&buffer.target.path).unwrap()["cells"][index];
                set_source(cell, output_text(&result.result.lines, buffer.text.ends_with('\n')));
                changed_notebooks.insert(buffer.target.path.clone());
            } else { fs::write(&buffer.target.path, output_text(&result.result.lines, true))?; }
        }
        out.push(result);
    }
    for path in changed_notebooks { fs::write(&path, notebook_text(&files.notebooks[&path])?)?; }
    Ok(out)
}

/// Edit a single notebook cell using already parsed, local commands.
pub fn edit_cell(path: &str, id: &str, commands: &[Command], sw: usize, inplace: bool) -> Result<FileEdit> {
    edit_cell_with_writer(path, id, commands, sw, inplace, |path, text| fs::write(path, text))
}

pub(crate) fn edit_cell_with_writer(
    path: &str,
    id: &str,
    commands: &[Command],
    sw: usize,
    inplace: bool,
    write: impl FnOnce(&str, &[u8]) -> io::Result<()>,
) -> Result<FileEdit> {
    let path = normalize_path(path)?;
    let mut nb = read_notebook(&path)?;
    let index = cell_index(&nb, id, &path)?;
    let cell = &mut nb["cells"][index];
    let text = cell_text(cell)?;
    let result = edit_text_with_sw(&text, commands, sw)?;
    let resolved_id = cell["id"].as_str().unwrap_or(id).to_owned();
    let new = output_text(&result.lines, text.ends_with('\n'));
    if inplace && new != text {
        set_source(cell, new);
        write(&path, &notebook_text(&nb)?)?;
    }
    Ok(FileEdit { path: format!("{path}:{resolved_id}"), cell: Some(resolved_id), original_text: text, result })
}

/// CLI replacement semantics: write a same-directory temporary file, preserve
/// existing permissions, then rename. Python's in-place API keeps its write semantics.
pub(crate) fn atomic_write(path: &str, content: &[u8]) -> io::Result<()> {
    use io::Write;
    let path = Path::new(path);
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(content)?;
    match fs::metadata(path) {
        Ok(meta) => temp.as_file().set_permissions(meta.permissions())?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Compact single-file CLI commands, with binary rejection and atomic replacement.
pub fn edit_file_argv(path: &str, args: &[String], text_block: &str, sw: usize, inplace: bool) -> Result<FileEdit> {
    let path = normalize_path(path)?;
    let text = match fs::read_to_string(&path) { Ok(t) => t, Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(), Err(e) => return Err(e.into()) };
    if text.contains('\0') { return Err(invalid("binary file rejected (NUL byte found)")); }
    let commands = crate::parse_commands_from_args(args, &mut io::Cursor::new(text_block.as_bytes()))?;
    let result = edit_text_with_sw(&text, &commands, sw)?;
    if inplace { atomic_write(&path, output_text(&result.lines, true).as_bytes())?; }
    Ok(FileEdit { path, cell: None, original_text: text, result })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fields(values: &[&str]) -> Vec<CommandField> { values.iter().map(|s| CommandField::Str((*s).into())).collect() }

    #[test]
    fn file_transfer_preview_and_failure() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.txt").to_string_lossy().into_owned();
        let dst = dir.path().join("dst.txt").to_string_lossy().into_owned();
        fs::write(&src, "alpha\nbeta\n").unwrap();
        let addr = crate::format_lnhash(1, "alpha");
        let cmds = vec![fields(&[&addr, "m", &format!("{dst}:0|0000|")])];
        let preview = edit_files(&src, &cmds, 4, false).unwrap();
        assert_eq!(preview[0].result.lines, ["beta"]);
        assert_eq!(preview[1].result.lines, ["alpha"]);
        assert!(!Path::new(&dst).exists());
        let mut bad = cmds.clone();
        bad.push(fields(&["1|dead|", "d"]));
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
        let commands = crate::parse_commands_from_script("%s/1/9/").unwrap();
        edit_cell(&path, "aa", &commands, 4, true).unwrap();
        let before: Value = serde_json::from_str(input).unwrap();
        let after: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after["metadata"], before["metadata"]);
        assert_eq!(after["cells"][0]["outputs"], before["cells"][0]["outputs"]);
        assert_eq!(after["cells"][0]["source"], serde_json::json!(["x=9\n", "y=2"]));
        assert!(matches!(view_cell(&path, "missing", None, None), Err(FileError::Cell(_))));
    }
}
