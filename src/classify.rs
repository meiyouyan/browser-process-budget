//! Classify Chromium processes from command-line heuristics.

use crate::model::{ClassifiedDocument, ProcessRole, SampleDocument};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(input: &Path, out: Option<&Path>) -> Result<()> {
    let raw = fs::read_to_string(input)
        .with_context(|| format!("reading sample {}", input.display()))?;
    let sample: SampleDocument =
        serde_json::from_str(&raw).context("parsing sample JSON")?;
    let classified = ClassifiedDocument::from_sample(sample);
    let json = serde_json::to_string_pretty(&classified)?;
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(path, format!("{json}\n"))
                .with_context(|| format!("writing {}", path.display()))?;
        }
        None => println!("{json}"),
    }
    Ok(())
}

/// Map a process name + command line to a Chromium role.
pub fn classify_command_line(name: &str, command_line: &str) -> ProcessRole {
    let name_l = name.to_ascii_lowercase();
    let cmd = command_line.to_ascii_lowercase();

    if name_l.contains("crashpad") || cmd.contains("chrome_crashpad_handler") {
        return ProcessRole::Other;
    }

    let looks_chromium = is_browser_main_binary(&name_l) || is_chromium_binary_in_cmd(&cmd);

    // Extension processes often carry --type=utility with extension mojo or
    // --extension-process / extension-related flags.
    if cmd.contains("--extension-process")
        || cmd.contains("--type=extension")
        || (cmd.contains("extension")
            && (cmd.contains("--type=utility") || cmd.contains("extensions.mojom")))
    {
        return ProcessRole::Extension;
    }

    if let Some(ty) = extract_type_flag(&cmd) {
        return match ty.as_str() {
            "renderer" => ProcessRole::Renderer,
            "gpu-process" | "gpu" => ProcessRole::Gpu,
            "utility" => ProcessRole::Utility,
            "extension" => ProcessRole::Extension,
            "zygote" | "broker" => ProcessRole::Other,
            other if other.contains("extension") => ProcessRole::Extension,
            _ => ProcessRole::Other,
        };
    }

    // No --type= → browser main process (when it looks like Chrome/Edge/Chromium).
    if looks_chromium {
        return ProcessRole::Browser;
    }

    ProcessRole::Other
}

fn is_browser_main_binary(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    matches!(
        base,
        "chrome"
            | "chrome.exe"
            | "msedge"
            | "msedge.exe"
            | "chromium"
            | "chromium.exe"
            | "google-chrome"
            | "google-chrome-stable"
            | "chromium-browser"
    )
}

fn is_chromium_binary_in_cmd(cmd: &str) -> bool {
    cmd.contains("chrome.exe")
        || cmd.contains("msedge.exe")
        || cmd.contains("chromium")
        || cmd.contains("google-chrome")
}

/// Extract the value of `--type=...` (first occurrence), lowercased.
fn extract_type_flag(cmd_lower: &str) -> Option<String> {
    const KEY: &str = "--type=";
    let idx = cmd_lower.find(KEY)?;
    let rest = &cmd_lower[idx + KEY.len()..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .unwrap_or(rest.len());
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn renderer_flag() {
        assert_eq!(
            classify_command_line(
                "chrome.exe",
                r#"C:\Program Files\Google\Chrome\Application\chrome.exe --type=renderer --lang=en-US"#
            ),
            ProcessRole::Renderer
        );
    }

    #[test]
    fn gpu_flag() {
        assert_eq!(
            classify_command_line("chrome", "/usr/bin/google-chrome --type=gpu-process"),
            ProcessRole::Gpu
        );
    }

    #[test]
    fn utility_flag() {
        assert_eq!(
            classify_command_line(
                "msedge.exe",
                r#""C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --type=utility --utility-sub-type=network.mojom.NetworkService"#
            ),
            ProcessRole::Utility
        );
    }

    #[test]
    fn extension_type_and_related() {
        assert_eq!(
            classify_command_line("chrome.exe", "chrome.exe --type=extension"),
            ProcessRole::Extension
        );
        assert_eq!(
            classify_command_line(
                "chrome.exe",
                "chrome.exe --type=utility --extension-process --util=extensions.mojom.KeepAlive"
            ),
            ProcessRole::Extension
        );
    }

    #[test]
    fn browser_main_no_type() {
        assert_eq!(
            classify_command_line(
                "chrome.exe",
                r#"C:\Program Files\Google\Chrome\Application\chrome.exe --prefetch:1"#
            ),
            ProcessRole::Browser
        );
        assert_eq!(
            classify_command_line("google-chrome", "/usr/bin/google-chrome --no-sandbox"),
            ProcessRole::Browser
        );
        assert_eq!(
            classify_command_line("msedge.exe", r#"C:\...\msedge.exe"#),
            ProcessRole::Browser
        );
    }

    #[test]
    fn other_non_browser() {
        assert_eq!(
            classify_command_line("notepad.exe", "notepad.exe file.txt"),
            ProcessRole::Other
        );
        assert_eq!(
            classify_command_line("chrome.exe", "chrome.exe --type=broker"),
            ProcessRole::Other
        );
    }

    #[test]
    fn chromium_linux_name() {
        assert_eq!(
            classify_command_line("chromium", "/usr/lib/chromium/chromium"),
            ProcessRole::Browser
        );
    }

    #[test]
    fn crashpad_is_other() {
        assert_eq!(
            classify_command_line(
                "chrome_crashpad_handler",
                "/opt/google/chrome/chrome_crashpad_handler --monitor-self"
            ),
            ProcessRole::Other
        );
    }
    #[test]
    fn fixture_classifies_roles() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sample-fixture.json");
        let raw = std::fs::read_to_string(&path).expect("fixture");
        let sample: crate::model::SampleDocument = serde_json::from_str(&raw).expect("parse");
        let doc = crate::model::ClassifiedDocument::from_sample(sample);
        let roles: Vec<_> = doc
            .processes
            .iter()
            .map(|p| (p.pid, p.role.clone().unwrap()))
            .collect();
        assert_eq!(roles[0], (1000, ProcessRole::Browser));
        assert_eq!(roles[1], (1001, ProcessRole::Renderer));
        assert_eq!(roles[2], (1002, ProcessRole::Extension));
        assert_eq!(roles[3], (1003, ProcessRole::Gpu));
        assert_eq!(roles[4], (1004, ProcessRole::Utility));
        assert_eq!(roles[5], (1005, ProcessRole::Extension));
        assert_eq!(roles[6], (2000, ProcessRole::Other));
        assert!(!doc.aggregates.is_empty());
    }

}
