use anyhow::Result;
use std::path::Path;

pub fn run(input: &Path, out_dir: &Path, redact: bool) -> Result<()> {
    let _ = (input, out_dir, redact);
    // TODO: emit report.json + report.md with redaction
    Ok(())
}
