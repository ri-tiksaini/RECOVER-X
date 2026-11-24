use pyo3::exceptions::{PyPermissionError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use recoverx_core::core::cancel::CancelToken;
use recoverx_core::error::RxError;
use recoverx_core::export::extractor::export_candidates;
use recoverx_core::fs::{scan_volume, ScanMode};
use recoverx_core::fs::common::candidate::RecoverCandidate;
use recoverx_core::platform::{device, image_reader, partition, path_safety, privilege};
use recoverx_core::platform::raw_reader::RawReader;
use recoverx_core::platform::trim::MediaKind;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

static LIVE_HANDLES: AtomicUsize = AtomicUsize::new(0);

fn map_err(e: RxError) -> PyErr {
    match e {
        RxError::PermissionDenied(m) => PyPermissionError::new_err(m),
        RxError::DestOnSource | RxError::DestInvalid(_) => PyValueError::new_err(e.to_string()),
        _ => PyRuntimeError::new_err(format!("[{}] {}", e.code(), e)),
    }
}

fn guard<T, F: FnOnce() -> PyResult<T>>(f: F) -> PyResult<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(res) => res,
        Err(_) => Err(PyRuntimeError::new_err("[9001] Rust panic caught at binding boundary")),
    }
}

#[pyclass]
pub struct SourceHandle {
    reader: Option<Arc<dyn RawReader>>,
    path: String,
}

impl SourceHandle {
    fn release(&mut self) {
        if self.reader.take().is_some() {
            LIVE_HANDLES.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn active_reader(&self) -> PyResult<Arc<dyn RawReader>> {
        self.reader
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| PyRuntimeError::new_err("SourceHandle is closed"))
    }
}

impl Drop for SourceHandle {
    fn drop(&mut self) {
        self.release();
    }
}

#[pymethods]
impl SourceHandle {
    fn close(&mut self) {
        self.release();
    }

    #[getter]
    fn size_bytes(&self) -> PyResult<u64> {
        Ok(self.active_reader()?.size())
    }

    #[getter]
    fn sector_size(&self) -> PyResult<u32> {
        Ok(self.active_reader()?.sector_size())
    }

    #[getter]
    fn path(&self) -> &str {
        &self.path
    }

    fn partitions_json(&self, py: Python<'_>) -> PyResult<String> {
        guard(|| {
            let r = self.active_reader()?;
            let parts = py.allow_threads(|| partition::parse_partitions(r.as_ref())).map_err(map_err)?;
            serde_json::to_string(&parts).map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }

    #[pyo3(signature = (mode="quick", max_candidates=None))]
    fn scan_json(&self, py: Python<'_>, mode: &str, max_candidates: Option<usize>) -> PyResult<String> {
        guard(|| {
            let r = self.active_reader()?;
            let dev_id = self.path.clone();
            let sm = if mode.eq_ignore_ascii_case("deep") { ScanMode::Deep } else { ScanMode::Quick };
            let (kind, cands) = py
                .allow_threads(|| scan_volume(r, dev_id, MediaKind::Unknown, sm, &CancelToken::new(), max_candidates))
                .map_err(map_err)?;
            let payload = serde_json::json!({ "fs_kind": kind, "candidates": cands });
            serde_json::to_string(&payload).map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }

    fn export_candidates_json(&self, py: Python<'_>, candidates_json: &str, dest_dir: &str) -> PyResult<String> {
        guard(|| {
            let r = self.active_reader()?;
            let cands: Vec<RecoverCandidate> = serde_json::from_str(candidates_json)
                .map_err(|e| PyValueError::new_err(format!("invalid candidates_json: {e}")))?;
            let src = self.path.clone();
            let dst = dest_dir.to_string();
            let recs = py
                .allow_threads(|| {
                    export_candidates(Path::new(&src), r.as_ref(), &cands, Path::new(&dst), &CancelToken::new())
                })
                .map_err(map_err)?;
            serde_json::to_string(&recs).map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }
}

#[pyfunction]
fn check_privilege() -> PyResult<String> {
    guard(|| {
        let info = privilege::check_privilege();
        serde_json::to_string(&info).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    })
}

#[pyfunction]
fn check_same_device(source_path: &str, dest_dir: &str) -> PyResult<bool> {
    guard(|| {
        path_safety::same_physical_device(Path::new(source_path), Path::new(dest_dir)).map_err(map_err)
    })
}

#[pyfunction]
fn device_list(py: Python<'_>) -> PyResult<String> {
    guard(|| {
        let list = py.allow_threads(device::enumerate_devices).map_err(map_err)?;
        serde_json::to_string(&list).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    })
}

#[pyfunction]
#[pyo3(signature = (os_path, partition_index=None))]
fn open_source(py: Python<'_>, os_path: &str, partition_index: Option<usize>) -> PyResult<SourceHandle> {
    guard(|| {
        let p = Path::new(os_path);
        let base: Arc<dyn RawReader> = py.allow_threads(|| -> Result<Arc<dyn RawReader>, RxError> {
            if p.is_file() {
                Ok(Arc::new(image_reader::ImageReader::open(p, 512)?))
            } else {
                #[cfg(unix)]
                {
                    Ok(Arc::new(recoverx_core::platform::raw_reader_linux::LinuxRawReader::open(p)?))
                }
                #[cfg(not(unix))]
                {
                    Ok(Arc::new(recoverx_core::platform::raw_reader_windows::WindowsRawReader::open(p)?))
                }
            }
        }).map_err(map_err)?;

        let reader: Arc<dyn RawReader> = if let Some(idx) = partition_index {
            let parts = partition::parse_partitions(base.as_ref()).map_err(map_err)?;
            let part = parts.get(idx).ok_or_else(|| PyValueError::new_err("invalid partition_index"))?;
            Arc::new(
                partition::SubSliceReader::new(
                    base,
                    recoverx_core::core::types::ByteOffset(part.start_bytes),
                    part.length_bytes,
                )
                .map_err(map_err)?,
            )
        } else {
            base
        };

        LIVE_HANDLES.fetch_add(1, Ordering::SeqCst);
        Ok(SourceHandle {
            reader: Some(reader),
            path: os_path.to_string(),
        })
    })
}

#[pyfunction]
fn live_handle_count() -> usize {
    LIVE_HANDLES.load(Ordering::SeqCst)
}

#[pymodule]
fn _recoverx_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SourceHandle>()?;
    m.add_function(wrap_pyfunction!(check_privilege, m)?)?;
    m.add_function(wrap_pyfunction!(check_same_device, m)?)?;
    m.add_function(wrap_pyfunction!(device_list, m)?)?;
    m.add_function(wrap_pyfunction!(open_source, m)?)?;
    m.add_function(wrap_pyfunction!(live_handle_count, m)?)?;
    Ok(())
}
