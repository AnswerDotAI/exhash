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
            lines: self.lines.clone(), hashes: self.hashes.clone(),
            modified: self.modified.clone(), deleted: self.deleted.clone(),
            origins: self.origins.clone(),
        };
        result.format_diff(&original_lines, context)
    }

    fn __str__(&self) -> String { self.format_diff(1) }

    fn __repr__(&self) -> String {
        let diff = self.format_diff(1);
        if diff.is_empty() {
            format!("EditResult({} lines, no changes)", self.lines.len())
        } else {
            format!("EditResult({} lines, {} modified, {} deleted)\n{}",
                self.lines.len(), self.modified.len(), self.deleted.len(), diff)
        }
    }

    fn __getitem__(&self, key: &str) -> PyResult<PyObject> {
        Python::with_gil(|py| match key {
            "lines" => Ok(self.lines.clone().into_pyobject(py)?.into_any().unbind()),
            "hashes" => Ok(self.hashes.clone().into_pyobject(py)?.into_any().unbind()),
            "modified" => Ok(self.modified.clone().into_pyobject(py)?.into_any().unbind()),
            "deleted" => Ok(self.deleted.clone().into_pyobject(py)?.into_any().unbind()),
            "origins" => Ok(self.origins.clone().into_pyobject(py)?.into_any().unbind()),
            _ => Err(pyo3::exceptions::PyKeyError::new_err(key.to_string())),
        })
    }
}

#[pyfunction]
fn line_hash(line: &str) -> String { format!("{:04x}", crate::line_hash_u16(line)) }

#[pyfunction]
fn lnhash(lineno: usize, line: &str) -> String { crate::format_lnhash(lineno, line) }

#[pyfunction]
#[pyo3(signature = (text, start=None, end=None))]
fn lnhashview(text: &str, start: Option<usize>, end: Option<usize>) -> PyResult<Vec<String>> {
    let lines: Vec<&str> = text.lines().collect();
    crate::lnhashview(&lines, start, end)
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

#[pyfunction]
#[pyo3(name = "exhash", signature = (text, *cmds, sw=4))]
fn py_exhash(text: &str, cmds: Vec<String>, sw: usize) -> PyResult<EditResultPy> {
    let cmd_refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
    let parsed = crate::parse_commands_from_strs(&cmd_refs)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let res = crate::edit_text_with_sw(text, &parsed, sw)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(EditResultPy {
        lines: res.lines, hashes: res.hashes, modified: res.modified,
        deleted: res.deleted, origins: res.origins,
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
    Ok(())
}
