use crate::error::{RxError, RxResult};
use std::fs::OpenOptions;
use std::path::Path;
use tracing_subscriber::EnvFilter;

pub fn init_tracing(json_sink_path: Option<&Path>, level: &str) -> RxResult<()> {
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    if let Some(path) = json_sink_path {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| RxError::IoFatal(format!("cannot open log sink: {e}")))?;
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .with_writer(file)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .try_init();
    }
    Ok(())
}
