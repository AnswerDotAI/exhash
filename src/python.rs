use std::panic::{catch_unwind, AssertUnwindSafe};

use pyo3::exceptions::{PyRuntimeError, PyUserWarning, PyValueError};
use pyo3::prelude::*;

use crate::parse::{
    command_from_parts, parse_destination_address, parse_optional_usize, split_text_payload,
    subst_from_parts, translit_from_parts,
};
use crate::{Command, EditError, Subcommand};

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

/// Wrap `s` in fastcore's `PrettyString` so bare display shows it verbatim.
fn pretty_string(py: Python<'_>, s: String) -> PyResult<Py<PyAny>> {
    Ok(py.import("fastcore.basics")?.getattr("PrettyString")?.call1((s,))?.unbind())
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

impl EditResultPy {
    fn diff_text(&self, context: usize) -> String {
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
}

#[pymethods]
impl EditResultPy {
    #[pyo3(signature = (context=1))]
    fn format_diff(&self, py: Python<'_>, context: usize) -> PyResult<Py<PyAny>> {
        pretty_string(py, self.diff_text(context))
    }


    fn __str__(&self) -> String {
        self.diff_text(1)
    }

    fn __repr__(&self) -> String {
        let diff = truncate_diff(&self.diff_text(1), 15, 120);
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

fn truncate_diff(s: &str, max_lines: usize, maxlen: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let mut out: Vec<String> = lines
        .iter()
        .take(max_lines)
        .map(|l| {
            if l.chars().count() <= maxlen {
                (*l).to_string()
            } else {
                let cut: String = l.chars().take(maxlen).collect();
                format!("{cut}…")
            }
        })
        .collect();
    if lines.len() > max_lines {
        out.push(format!("…{} lines elided…", lines.len() - max_lines));
    }
    if out.is_empty() {
        String::new()
    } else {
        out.join("\n") + "\n"
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

/// A tuple-command field: a string, or a nested tuple (a global's subcommand).
#[derive(FromPyObject)]
enum PyField {
    #[pyo3(transparent)]
    Str(String),
    #[pyo3(transparent)]
    Seq(Vec<PyField>),
}

fn command_from_pyfields(fields: &[PyField]) -> Result<Command, EditError> {
    let [PyField::Str(addr), PyField::Str(op), rest @ ..] = fields else {
        return Err(EditError::new(
            "command must start with (address, op) strings",
        ));
    };
    command_from_parts(addr, subcommand_from_pyfields(op, rest)?)
}

fn str_fields<'a>(op: &str, fields: &'a [PyField]) -> Result<Vec<&'a str>, EditError> {
    fields
        .iter()
        .map(|f| match f {
            PyField::Str(s) => Ok(s.as_str()),
            PyField::Seq(_) => Err(EditError::new(format!("{op} fields must be strings"))),
        })
        .collect()
}

fn subcommand_from_pyfields(op: &str, fields: &[PyField]) -> Result<Subcommand, EditError> {
    if let "g" | "g!" | "v" = op {
        let [PyField::Str(pattern), PyField::Seq(inner)] = fields else {
            return Err(EditError::new(format!(
                "{op} takes (pattern, (subcommand, ...))"
            )));
        };
        let [PyField::Str(iop), irest @ ..] = inner.as_slice() else {
            return Err(EditError::new("global subcommand must start with an op string"));
        };
        if matches!(iop.as_str(), "g" | "g!" | "v") {
            return Err(EditError::new("global commands cannot nest"));
        }
        return Ok(Subcommand::Global {
            invert: op != "g",
            pattern: pattern.clone(),
            cmd: Box::new(subcommand_from_pyfields(iop, irest)?),
        });
    }
    let f = str_fields(op, fields)?;
    match (op, f.as_slice()) {
        ("d", []) => Ok(Subcommand::Delete),
        ("p", []) => Ok(Subcommand::Print),
        ("j", []) => Ok(Subcommand::Join),
        ("sort", []) => Ok(Subcommand::Sort),
        ("a", [text]) => Ok(Subcommand::Append(split_text_payload(text))),
        ("i", [text]) => Ok(Subcommand::Insert(split_text_payload(text))),
        ("c", [text]) => Ok(Subcommand::Change(split_text_payload(text))),
        ("s", [pat, rep]) => Ok(Subcommand::Substitute(subst_from_parts(
            (*pat).into(),
            (*rep).into(),
            "",
        )?)),
        ("s", [pat, rep, flags]) => Ok(Subcommand::Substitute(subst_from_parts(
            (*pat).into(),
            (*rep).into(),
            flags,
        )?)),
        ("y", [source, dest]) => {
            let (source, dest) = translit_from_parts((*source).into(), (*dest).into())?;
            Ok(Subcommand::Transliterate { source, dest })
        }
        ("m", [dest]) => Ok(Subcommand::Move {
            dest: parse_destination_address(dest, 'm')?,
        }),
        ("t", [dest]) => Ok(Subcommand::Copy {
            dest: parse_destination_address(dest, 't')?,
        }),
        (">", rest @ ([] | [_])) => Ok(Subcommand::Indent {
            levels: parse_optional_usize(rest.first().copied().unwrap_or(""))?,
        }),
        ("<", rest @ ([] | [_])) => Ok(Subcommand::Dedent {
            levels: parse_optional_usize(rest.first().copied().unwrap_or(""))?,
        }),
        _ => Err(EditError::new(format!(
            "invalid tuple command: {op:?} with {} field(s)",
            f.len()
        ))),
    }
}

#[pyfunction]
#[pyo3(name = "exhash", signature = (text, *cmds, sw=4))]
fn py_exhash(py: Python<'_>, text: &str, cmds: Vec<Vec<PyField>>, sw: usize) -> PyResult<EditResultPy> {
    let parsed = guard("parsing commands", || {
        cmds.iter()
            .map(|c| command_from_pyfields(c))
            .collect::<Result<Vec<_>, _>>()
    })?
    .map_err(|e| PyValueError::new_err(e.to_string()))?;
    warn_on_ex_style_dot_terminators(py, &parsed)?;
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

fn warn_on_ex_style_dot_terminators(py: Python<'_>, parsed: &[Command]) -> PyResult<()> {
    for (i, cmd) in parsed.iter().enumerate() {
        let Some(text) = command_text_block(cmd) else { continue };
        let mut lines: Vec<&str> = text.iter().map(|s| s.as_str()).collect();
        while matches!(lines.last(), Some(&"")) {
            lines.pop();
        }
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
        Subcommand::Global { cmd, .. } => match cmd.as_ref() {
            Subcommand::Append(t) | Subcommand::Insert(t) | Subcommand::Change(t) => Some(t),
            _ => None,
        },
        _ => None,
    }
}
