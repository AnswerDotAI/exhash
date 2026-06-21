use std::panic::{catch_unwind, AssertUnwindSafe};

use pyo3::exceptions::{PyRuntimeError, PyUserWarning, PyValueError};
use pyo3::prelude::*;

use crate::{Command, Subcommand};

/// Run a panic-prone pure-Rust step, converting any panic into a clean
/// `RuntimeError` instead of surfacing pyo3's `BaseException`-derived
/// `PanicException`.
fn guard<T>(what: &str, f: impl FnOnce() -> T) -> PyResult<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| {
        PyRuntimeError::new_err(format!(
            "internal error in exhash while {what} (this is a bug, please report it)"
        ))
    })
}

#[pyclass(skip_from_py_object)]
#[derive(Clone)]
struct EditResultPy {
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
    original_text: String,
}

#[pymethods]
impl EditResultPy {
    #[pyo3(signature = (context=1))]
    fn format_diff(&self, context: usize) -> String {
        let original_lines: Vec<&str> = self.original_text.lines().collect();
        let result = crate::EditResult {
            lines: self.lines.clone(),
            hashes: self.hashes.clone(),
            modified: self.modified.clone(),
            deleted: self.deleted.clone(),
            origins: self.origins.clone(),
        };
        result.format_diff(&original_lines, context)
    }

    fn __str__(&self) -> String {
        self.format_diff(1)
    }

    fn __repr__(&self) -> String {
        let diff = self.format_diff(1);
        if diff.is_empty() {
            format!("EditResult({} lines, no changes)", self.lines.len())
        } else {
            format!(
                "EditResult({} lines, {} modified, {} deleted)\n{}",
                self.lines.len(),
                self.modified.len(),
                self.deleted.len(),
                diff
            )
        }
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        match key {
            "lines" => Ok(self.lines.clone().into_pyobject(py)?.into_any().unbind()),
            "hashes" => Ok(self.hashes.clone().into_pyobject(py)?.into_any().unbind()),
            "modified" => Ok(self.modified.clone().into_pyobject(py)?.into_any().unbind()),
            "deleted" => Ok(self.deleted.clone().into_pyobject(py)?.into_any().unbind()),
            "origins" => Ok(self.origins.clone().into_pyobject(py)?.into_any().unbind()),
            _ => Err(pyo3::exceptions::PyKeyError::new_err(key.to_string())),
        }
    }
}

#[pyfunction]
fn line_hash(line: &str) -> String {
    format!("{:04x}", crate::line_hash_u16(line))
}

#[pyfunction]
fn lnhash(lineno: usize, line: &str) -> String {
    crate::format_lnhash(lineno, line)
}

#[pyfunction]
#[pyo3(signature = (text, start=None, end=None))]
fn lnhashview(text: &str, start: Option<usize>, end: Option<usize>) -> PyResult<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    guard("listing lines", || crate::lnhashview(&lines, start, end))?
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "exhash", signature = (text, *cmds, sw=4))]
fn py_exhash(py: Python<'_>, text: &str, cmds: Vec<String>, sw: usize) -> PyResult<EditResultPy> {
    let cmd_refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
    let parsed = guard("parsing commands", || {
        crate::parse_commands_from_strs(&cmd_refs)
    })?
    .map_err(|e| PyValueError::new_err(e.to_string()))?;
    warn_on_ex_style_dot_terminators(py, &cmds, &parsed)?;
    let res = guard("applying edits", || {
        crate::edit_text_with_sw(text, &parsed, sw)
    })?
    .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(EditResultPy {
        lines: res.lines,
        hashes: res.hashes,
        modified: res.modified,
        deleted: res.deleted,
        origins: res.origins,
        original_text: text.to_string(),
    })
}

#[pyfunction]
#[pyo3(signature = (text, cmds, text_block="", sw=4))]
fn exhash_argv(
    text: &str,
    cmds: Vec<String>,
    text_block: &str,
    sw: usize,
) -> PyResult<EditResultPy> {
    let mut stream = std::io::Cursor::new(text_block.as_bytes());
    let parsed = guard("parsing commands", || {
        crate::parse_commands_from_args(&cmds, &mut stream)
    })?
    .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let res = guard("applying edits", || {
        crate::edit_text_with_sw(text, &parsed, sw)
    })?
    .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(EditResultPy {
        lines: res.lines,
        hashes: res.hashes,
        modified: res.modified,
        deleted: res.deleted,
        origins: res.origins,
        original_text: text.to_string(),
    })
}

#[pymodule]
fn exhash(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EditResultPy>()?;
    m.add_function(wrap_pyfunction!(line_hash, m)?)?;
    m.add_function(wrap_pyfunction!(lnhash, m)?)?;
    m.add_function(wrap_pyfunction!(lnhashview, m)?)?;
    m.add_function(wrap_pyfunction!(py_exhash, m)?)?;
    m.add_function(wrap_pyfunction!(exhash_argv, m)?)?;
    Ok(())
}

fn warn_on_ex_style_dot_terminators(
    py: Python<'_>,
    inputs: &[String],
    parsed: &[Command],
) -> PyResult<()> {
    for (i, (input, cmd)) in inputs.iter().zip(parsed.iter()).enumerate() {
        if command_has_text_block(cmd) && looks_like_ex_style_dot_terminator(input) {
            let msg = format!(
                "cmds[{i}] ends with a '.' line. In exhash(text, cmds), a/i/c text blocks do not use ex-style '.' terminators; that final '.' line will be inserted literally."
            );
            let warnings = py.import("warnings")?;
            warnings.call_method1("warn", (msg, py.get_type::<PyUserWarning>(), 2))?;
        }
    }
    Ok(())
}

fn command_has_text_block(cmd: &Command) -> bool {
    match &cmd.cmd {
        Subcommand::Append(_) | Subcommand::Insert(_) | Subcommand::Change(_) => true,
        Subcommand::Global { cmd, .. } => matches!(
            cmd.as_ref(),
            Subcommand::Append(_) | Subcommand::Insert(_) | Subcommand::Change(_)
        ),
        _ => false,
    }
}

fn looks_like_ex_style_dot_terminator(input: &str) -> bool {
    let Some((_, rest)) = input.split_once('\n') else {
        return false;
    };
    let mut lines: Vec<&str> = rest
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    while matches!(lines.last(), Some(&"")) {
        lines.pop();
    }
    matches!(lines.last(), Some(&"."))
}
