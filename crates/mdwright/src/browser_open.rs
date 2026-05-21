use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};

pub(crate) fn open_html(html: &str) -> Result<PathBuf> {
    let mut file = tempfile::Builder::new()
        .prefix("mdwright-render-")
        .suffix(".html")
        .tempfile()
        .context("create temporary HTML file")?;
    file.write_all(html.as_bytes()).context("write temporary HTML file")?;
    let temp_path = file.into_temp_path();
    let path = temp_path.keep().context("persist temporary HTML file")?;
    if std::env::var_os("MDWRIGHT_OPEN_DRY_RUN").is_none() {
        opener::open(&path).with_context(|| format!("open {}", path.display()))?;
    }
    Ok(path)
}
