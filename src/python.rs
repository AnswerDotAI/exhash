use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass]
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
}


impl From<crate::EditResult> for EditResultPy {
    fn from(r: crate::EditResult) -> Self {
        Self { lines: r.lines, hashes: r.hashes, modified: r.modified, deleted: r.deleted }
    }
}

#[pyfunction]
fn line_hash(line: &str) -> String { format!("{:04x}", crate::line_hash_u16(line)) }

#[pyfunction]
fn lnhash(lineno: usize, line: &str) -> String { crate::format_lnhash(lineno, line) }

#[pyfunction]
fn lnhashview(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .map(|(i, l)| format!("{}  {}", crate::format_lnhash(i + 1, l), l))
        .collect()
}

#[pyfunction]
#[pyo3(name = "exhash", signature = (text, *cmds))]
fn py_exhash(text: &str, cmds: Vec<String>) -> PyResult<EditResultPy> {
    let cmd_refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
    let parsed = crate::parse_commands_from_strs(&cmd_refs)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let res = crate::edit_text(text, &parsed)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(res.into())
}

#[pymodule]
fn exhash(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<EditResultPy>()?;
    m.add_function(wrap_pyfunction!(line_hash, m)?)?;
    m.add_function(wrap_pyfunction!(lnhash, m)?)?;
    m.add_function(wrap_pyfunction!(lnhashview, m)?)?;
    m.add_function(wrap_pyfunction!(py_exhash, m)?)?;
    Ok(())
}
