use anyhow::Result;
use std::path::Path;

pub fn run(input: &Path, out: Option<&Path>) -> Result<()> {
    let _ = (input, out);
    // TODO: map command-line flags to Browser | Renderer | GPU | Utility | Extension | Other
    Ok(())
}
