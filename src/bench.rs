use anyhow::Result;
use std::path::Path;

pub fn run(scenario: &str, browser: &str, out_dir: &Path) -> Result<()> {
    let _ = (scenario, browser, out_dir);
    // TODO: load scripts/<scenario>.toml, optionally launch URLs, then sample
    Ok(())
}
