use anyhow::{bail, Result};
use std::path::Path;

/// Collect process snapshots. Windows-only for v0.1.
pub fn run(browser: &str, out: Option<&Path>, session: bool) -> Result<()> {
    let _ = (browser, out, session);
    #[cfg(not(windows))]
    {
        bail!("bpb sample currently targets Windows (RDS). Cross-compile or run on the RDS host.");
    }
    #[cfg(windows)]
    {
        // TODO: enumerate chrome.exe trees, working set, command line, session id
        bail!("Windows sampler not implemented yet — scaffold only.");
    }
}
