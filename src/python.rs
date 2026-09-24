use std::panic::{AssertUnwindSafe, catch_unwind};

use pyo3::exceptions::{PyRuntimeError, PyUserWarning, PyValueError};
use pyo3::prelude::*;

use crate::commands::command_from_fields;
use crate::{Command, CommandField, Subcommand};

/// Run a panic-prone pure-Rust step, converting any panic into a clean
/// `RuntimeError` instead of surfacing pyo3's `BaseException`-derived
/// `PanicException`.
fn guard<T>(what: &str, f: impl FnOnce() -> T) -> PyResult<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| PyRuntimeError::new_err(format!("internal error in exhash while {what} (this is a bug, please report it)")))
}

/// Wrap `s` in fastcore's `PrettyString` so bare display shows it verbatim.
fn pretty_string(py: Python<'_>, s: String) -> PyResult<Py<PyAny>> { Ok(py.import("fastcore.basics")?.getattr("PrettyString")?.call1((s,))?.unbind()) }

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
struct EditResultPy {
    /// The resolved target of a result from `edit_files`; None for text, cell and CLI results
    #[pyo3(get)]
    path: Option<String>,
    #[pyo3(get)]
    cell: Option<String>,
    #[pyo3(get)]
    lines: Vec<String>,
    #[pyo3(get)]
    hashes: Vec<String>,
    #[pyo3(get)]
    modified: Vec<usize>,
    #[pyo3(get)]
    deleted: Vec<usize>,
    #[pyo3(get)]
    origins: Vec<Option<usize>>,
    #[pyo3(get)]
    printed: Vec<usize>,
    #[pyo3(get)]
    original_text: String,
}

impl EditResultPy {
    fn result(&self) -> crate::EditResult {
        crate::EditResult {
            lines: self.lines.clone(),
            hashes: self.hashes.clone(),
            modified: self.modified.clone(),
            deleted: self.deleted.clone(),
            origins: self.origins.clone(),
            printed: self.printed.clone(),
        }
    }

    /// The target-aware form of a result from `edit_files`; text, cell and CLI results have no path.
    fn file_edit(&self) -> Option<crate::FileEdit> {
        self.path.clone().map(|path| crate::FileEdit { path, cell: self.cell.clone(), original_text: self.original_text.clone(), result: self.result() })
    }

    fn diff_text(&self, context: usize, maxlen: Option<usize>) -> String {
        match self.file_edit() {
            Some(f) => f.format_diff_with_maxlen(context, maxlen),
            None => self.result().format_diff_with_maxlen(&self.original_text.lines().collect::<Vec<_>>(), context, maxlen),
        }
    }

    fn report_text(&self, context: usize, trunc: bool) -> String {
        match self.file_edit() {
            Some(f) => f.report(context, trunc),
            None => self.result().report(&self.original_text.lines().collect::<Vec<_>>(), context, trunc),
        }
    }
}

fn edit_result_py(path: Option<String>, cell: Option<String>, original_text: String, result: crate::EditResult) -> EditResultPy {
    EditResultPy {
        path,
        cell,
        lines: result.lines,
        hashes: result.hashes,
        modified: result.modified,
        deleted: result.deleted,
        origins: result.origins,
        printed: result.printed,
        original_text,
    }
}

#[pymethods]
impl EditResultPy {
    #[getter]
    fn original_lines(&self) -> Vec<&str> { self.original_text.lines().collect() }

    #[pyo3(signature = (context=1, maxlen=None))]
    fn format_diff(&self, py: Python<'_>, context: usize, maxlen: Option<usize>) -> PyResult<Py<PyAny>> { pretty_string(py, self.diff_text(context, maxlen)) }

    fn format_printed(&self, py: Python<'_>) -> PyResult<Py<PyAny>> { pretty_string(py, self.result().format_printed()) }

    /// The diff, then the printed lines under a `# printed` header; `trunc` caps it for display.
    #[pyo3(signature = (context=1, trunc=false))]
    fn report(&self, py: Python<'_>, context: usize, trunc: bool) -> PyResult<Py<PyAny>> { pretty_string(py, self.report_text(context, trunc)) }

    fn __str__(&self) -> String { self.report_text(1, false) }

    fn __repr__(&self) -> String {
        let changed = !self.diff_text(1, None).is_empty();
        let mut note = if changed { format!(", {} modified, {} deleted", self.modified.len(), self.deleted.len()) } else { String::new() };
        if !self.printed.is_empty() { note += &format!(", {} printed", self.printed.len()); }
        if !changed { note += ", no changes"; }
        let body = self.report_text(1, true);
        let body = if body.is_empty() { body } else { format!("\n{body}") };
        format!("EditResult({} lines{note}){body}", self.lines.len())
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        match key {
            "lines" => Ok(self.lines.clone().into_pyobject(py)?.into_any().unbind()),
            "hashes" => Ok(self.hashes.clone().into_pyobject(py)?.into_any().unbind()),
            "modified" => Ok(self.modified.clone().into_pyobject(py)?.into_any().unbind()),
            "deleted" => Ok(self.deleted.clone().into_pyobject(py)?.into_any().unbind()),
            "origins" => Ok(self.origins.clone().into_pyobject(py)?.into_any().unbind()),
            "printed" => Ok(self.printed.clone().into_pyobject(py)?.into_any().unbind()),
            _ => Err(pyo3::exceptions::PyKeyError::new_err(key.to_string())),
        }
    }
}

#[pyfunction]
fn truncate_diff(s: &str, max_lines: usize) -> String { crate::truncate_diff(s, max_lines) }

#[pyfunction]
fn line_hash(line: &str) -> String { crate::lnhash::format_hash(crate::line_hash_u16(line)) }

#[pyfunction]
fn lnhash(lineno: usize, line: &str) -> String { crate::format_lnhash(lineno, line) }

#[pyfunction]
#[pyo3(signature = (text, start=None, end=None))]
fn lnhashview(text: &str, start: Option<usize>, end: Option<usize>) -> PyResult<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    guard("listing lines", || crate::lnhashview(&lines, start, end))?.map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Scan Markdown into `(headings, links)` tuples for the Python outline layer.
/// Headings are `(level, title, start_line, end_line)`; links are
/// `(n, txt, url, tail, line)`. Lines are 1-based.
#[pyfunction]
#[pyo3(signature = (text, rm_fenced=true))]
fn md_scan(text: &str, rm_fenced: bool) -> PyResult<(Vec<(usize, String, usize, usize)>, Vec<(usize, String, String, String, usize)>)> {
    let (headings, links) = guard("scanning markdown", || crate::scan_md(text, rm_fenced))?;
    Ok((
        headings.into_iter().map(|h| (h.level, h.title, h.start_line, h.end_line)).collect(),
        links.into_iter().map(|l| (l.n, l.txt, l.url, l.tail, l.line)).collect(),
    ))
}

/// Scan source code into preorder `(level, title, start_line, end_line)` rows
/// via tree-sitter; `level` is section nesting depth. Lines are 1-based.
#[pyfunction]
fn code_scan(text: &str, lang: &str) -> PyResult<Vec<(usize, String, usize, usize)>> {
    let rows = guard("scanning code", || crate::scan_code(text, lang))?.map_err(PyValueError::new_err)?;
    Ok(rows.into_iter().map(|h| (h.level, h.title, h.start_line, h.end_line)).collect())
}
/// A tuple-command field: a string, or a nested tuple (a global's subcommand).
#[derive(FromPyObject)]
enum PyField {
    #[pyo3(transparent)]
    Str(String),
    #[pyo3(transparent)]
    Seq(Vec<PyField>),
}

impl PyField {
    fn native(&self) -> CommandField {
        match self { Self::Str(s) => CommandField::Str(s.clone()), Self::Seq(v) => CommandField::Seq(v.iter().map(Self::native).collect()) }
    }
}

#[pyfunction]
#[pyo3(name = "exhash", signature = (text, *cmds, sw=4))]
fn py_exhash(py: Python<'_>, text: &str, cmds: Vec<Vec<PyField>>, sw: usize) -> PyResult<EditResultPy> {
    let parsed = guard("parsing commands", || {
        cmds.iter().map(|c| command_from_fields(&c.iter().map(PyField::native).collect::<Vec<_>>())).collect::<Result<Vec<_>, _>>()
    })?
    .map_err(|e| PyValueError::new_err(e.to_string()))?;
    warn_on_ex_style_dot_terminators(py, &parsed)?;
    let res = guard("applying edits", || crate::edit_text_with_sw(text, &parsed, sw))?.map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(edit_result_py(None, None, text.to_string(), res))
}

#[pyfunction]
#[pyo3(signature = (text, cmds, text_block="", sw=4))]
fn exhash_argv(text: &str, cmds: Vec<String>, text_block: &str, sw: usize) -> PyResult<EditResultPy> {
    let mut stream = std::io::Cursor::new(text_block.as_bytes());
    let parsed = guard("parsing commands", || crate::parse_commands_from_args(&cmds, &mut stream))?.map_err(|e| PyValueError::new_err(e.to_string()))?;
    let res = guard("applying edits", || crate::edit_text_with_sw(text, &parsed, sw))?.map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(edit_result_py(None, None, text.to_string(), res))
}

fn file_error(error: crate::FileError) -> PyErr {
    use pyo3::exceptions::{PyFileNotFoundError, PyKeyError, PyOSError};
    match error {
        crate::FileError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => PyFileNotFoundError::new_err(e.to_string()),
        crate::FileError::Io(e) => PyOSError::new_err(e.to_string()),
        crate::FileError::Cell(e) => PyKeyError::new_err(e),
        other => PyValueError::new_err(other.to_string()),
    }
}

#[pyfunction]
#[pyo3(signature = (path, start=None, end=None))]
fn view_file(path: &str, start: Option<usize>, end: Option<usize>) -> PyResult<Vec<String>> { crate::view_file(path, start, end).map_err(file_error) }
#[pyfunction]
#[pyo3(signature = (path, cell_id, start=None, end=None))]
fn view_cell(path: &str, cell_id: &str, start: Option<usize>, end: Option<usize>) -> PyResult<Vec<String>> {
    crate::view_cell(path, cell_id, start, end).map_err(file_error)
}
#[pyfunction]
#[pyo3(signature = (path, cell_ids, start=None, end=None))]
fn view_cells(path: &str, cell_ids: Vec<String>, start: Option<usize>, end: Option<usize>) -> PyResult<Vec<String>> {
    crate::view_cells(path, &cell_ids, start, end).map_err(file_error)
}
#[pyfunction]
#[pyo3(signature = (path, commands, sw=4, inplace=true))]
fn edit_files(py: Python<'_>, path: &str, commands: Vec<Vec<PyField>>, sw: usize, inplace: bool) -> PyResult<Vec<EditResultPy>> {
    let commands: Vec<Vec<_>> = commands.iter().map(|c| c.iter().map(PyField::native).collect()).collect();
    // Warnings are Python presentation; qualified address parsing lives in Rust.
    let warning_commands = commands
        .iter()
        .filter_map(|c| {
            let [CommandField::Str(_), CommandField::Str(op), rest @ ..] = c.as_slice() else { return None; };
            if !matches!(op.as_str(), "a" | "i" | "c" | "g" | "g!" | "v") { return None; }
            let mut local = vec![CommandField::Str("%".into()), CommandField::Str(op.clone())];
            local.extend_from_slice(rest);
            command_from_fields(&local).ok()
        })
        .collect::<Vec<_>>();
    warn_on_ex_style_dot_terminators(py, &warning_commands)?;
    let results = guard("editing files", || crate::edit_files(path, &commands, sw, inplace))?.map_err(file_error)?;
    Ok(results.into_iter().map(|r| edit_result_py(Some(r.path), r.cell, r.original_text, r.result)).collect())
}

/// Each shown target's report, as `str(FileSetEditResult)` shows it. `results` must come from `edit_files`.
#[pyfunction]
#[pyo3(signature = (results, context=1, trunc=false))]
fn render(results: Vec<PyRef<'_, EditResultPy>>, context: usize, trunc: bool) -> PyResult<String> {
    let edits = results.iter().map(|r| r.file_edit().ok_or_else(|| PyValueError::new_err("render takes results from edit_files"))).collect::<PyResult<Vec<_>>>()?;
    Ok(crate::render(&edits, context, trunc))
}
#[pyfunction]
#[pyo3(signature = (path, cell_id, commands, sw=4, inplace=true))]
fn edit_cell(py: Python<'_>, path: &str, cell_id: &str, commands: Vec<Vec<PyField>>, sw: usize, inplace: bool) -> PyResult<EditResultPy> {
    let parsed = commands
        .iter()
        .map(|c| command_from_fields(&c.iter().map(PyField::native).collect::<Vec<_>>()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    warn_on_ex_style_dot_terminators(py, &parsed)?;
    let r = guard("editing a cell", || crate::edit_cell(path, cell_id, &parsed, sw, inplace))?.map_err(file_error)?;
    Ok(edit_result_py(None, None, r.original_text, r.result))
}
#[pyfunction]
#[pyo3(signature = (path, cell_id, cmds, text_block="", sw=4, inplace=true))]
fn edit_cell_argv(path: &str, cell_id: &str, cmds: Vec<String>, text_block: &str, sw: usize, inplace: bool) -> PyResult<EditResultPy> {
    let parsed = crate::parse_commands_from_args(&cmds, &mut std::io::Cursor::new(text_block.as_bytes())).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let r = guard("editing a cell", || crate::files::edit_cell_with_writer(path, cell_id, &parsed, sw, inplace, crate::files::atomic_write))?
        .map_err(file_error)?;
    Ok(edit_result_py(None, None, r.original_text, r.result))
}

#[pyfunction]
#[pyo3(signature = (path, cmds, text_block="", sw=4, inplace=true))]
fn edit_file_argv(path: &str, cmds: Vec<String>, text_block: &str, sw: usize, inplace: bool) -> PyResult<EditResultPy> {
    let r = guard("editing a file", || crate::edit_file_argv(path, &cmds, text_block, sw, inplace))?.map_err(file_error)?;
    Ok(edit_result_py(None, None, r.original_text, r.result))
}

#[pymodule]
fn exhash(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(view_file, m)?)?;
    m.add_function(wrap_pyfunction!(view_cell, m)?)?;
    m.add_function(wrap_pyfunction!(view_cells, m)?)?;
    m.add_function(wrap_pyfunction!(edit_files, m)?)?;
    m.add_function(wrap_pyfunction!(edit_cell, m)?)?;
    m.add_function(wrap_pyfunction!(edit_cell_argv, m)?)?;
    m.add_function(wrap_pyfunction!(edit_file_argv, m)?)?;
    m.add_class::<EditResultPy>()?;
    m.add_function(wrap_pyfunction!(line_hash, m)?)?;
    m.add_function(wrap_pyfunction!(lnhash, m)?)?;
    m.add_function(wrap_pyfunction!(lnhashview, m)?)?;
    m.add_function(wrap_pyfunction!(py_exhash, m)?)?;
    m.add_function(wrap_pyfunction!(exhash_argv, m)?)?;
    m.add_function(wrap_pyfunction!(md_scan, m)?)?;
    m.add_function(wrap_pyfunction!(code_scan, m)?)?;
    m.add_function(wrap_pyfunction!(truncate_diff, m)?)?;
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add("MAXLEN", crate::MAXLEN)?;
    Ok(())
}

fn warn_on_ex_style_dot_terminators<'a>(py: Python<'_>, parsed: impl IntoIterator<Item = &'a Command>) -> PyResult<()> {
    for (i, cmd) in parsed.into_iter().enumerate() {
        let Some(text) = command_text_block(cmd) else { continue; };
        let mut lines: Vec<&str> = text.iter().map(|s| s.as_str()).collect();
        while matches!(lines.last(), Some(&"")) { lines.pop(); }
        if lines.len() >= 2 && matches!(lines.last(), Some(&".")) {
            let msg = format!(
                "cmds[{i}] ends with a '.' line. In exhash(text, cmds), a/i/c text blocks do not use ex-style '.' terminators; that final '.' line will be inserted literally."
            );
            let warnings = py.import("warnings")?;
            warnings.call_method1("warn", (msg, py.get_type::<PyUserWarning>(), 2))?;
        }
    }
    Ok(())
}

fn command_text_block(cmd: &Command) -> Option<&[String]> {
    match &cmd.cmd {
        Subcommand::Append(t) | Subcommand::Insert(t) | Subcommand::Change(t) => Some(t),
        Subcommand::Global { cmd, .. } => match cmd.as_ref() { Subcommand::Append(t) | Subcommand::Insert(t) | Subcommand::Change(t) => Some(t), _ => None },
        _ => None,
    }
}
