//! Run a named scenario script then sample → classify → report.
//!
//! Scenarios may declare `[[profiles]]` (Chrome/Edge launch arg sets). When
//! present, each profile gets its own `out_dir/<profile_id>/` tree and a
//! cross-profile `compare.json` / `compare.md` is written at `out_dir/`.
//! Without profiles, behavior matches the original single-run layout.

use crate::{classify, report, sample};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    browser: Option<String>,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    idle_seconds: u64,
    #[serde(default)]
    profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Profile {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    args: Vec<String>,
}

/// Default profile used when a scenario has no `[[profiles]]` (backward compat).
fn implicit_baseline() -> Profile {
    Profile {
        id: "baseline".into(),
        description: "Default Site Isolation (no extra args)".into(),
        args: vec![],
    }
}

pub fn run(scenario: &str, browser: &str, out_dir: &Path, profile_filter: Option<&str>) -> Result<()> {
    let script_path = resolve_scenario_path(scenario)?;
    let raw = fs::read_to_string(&script_path)
        .with_context(|| format!("reading scenario {}", script_path.display()))?;
    let scen: Scenario = toml::from_str(&raw)
        .with_context(|| format!("parsing {}", script_path.display()))?;

    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating {}", out_dir.display()))?;

    let browser = if browser.is_empty() {
        scen.browser.as_deref().unwrap_or("chrome")
    } else {
        browser
    };

    let multi = !scen.profiles.is_empty();
    if let Some(want) = profile_filter {
        if !multi {
            bail!(
                "--profile was set to '{want}' but scenario '{}' has no [[profiles]]; omit --profile for single-run scenarios",
                scen.id
            );
        }
    }

    let profiles: Vec<Profile> = if multi {
        scen.profiles.clone()
    } else {
        vec![implicit_baseline()]
    };

    let profiles: Vec<Profile> = if let Some(want) = profile_filter {
        let filtered: Vec<_> = profiles.into_iter().filter(|p| p.id == want).collect();
        if filtered.is_empty() {
            let defined = scen
                .profiles
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "profile '{want}' not found in scenario '{}' (defined: {defined})",
                scen.id
            );
        }
        filtered
    } else {
        profiles
    };

    // In single-run (no [[profiles]]), write at out_dir root.
    // With profiles, write under out_dir/<id>/ and emit compare.* afterward.
    let mut summaries: Vec<ProfileSummary> = Vec::new();

    for profile in &profiles {
        let run_dir = if multi {
            let d = out_dir.join(&profile.id);
            fs::create_dir_all(&d)
                .with_context(|| format!("creating {}", d.display()))?;
            d
        } else {
            out_dir.to_path_buf()
        };

        let launch_status = maybe_launch_browser(browser, &scen.urls, &profile.args);

        let security_notes = security_notes_for(&profile.args);

        let marker = json!({
            "scenario_id": scen.id,
            "description": scen.description,
            "browser": browser,
            "urls": scen.urls,
            "idle_seconds": scen.idle_seconds,
            "script_path": script_path.display().to_string(),
            "profile_id": profile.id,
            "profile_description": profile.description,
            "args": profile.args,
            "security_notes": security_notes,
            "url_launch": launch_status,
            "notes": "Keep Site Isolation enabled. Profile args trade process density vs. fate-sharing within Site Isolation constraints; never recommend --disable-site-isolation-trials / --no-sandbox / --single-process as defaults.",
        });
        fs::write(
            run_dir.join("scenario-marker.json"),
            format!("{}\n", serde_json::to_string_pretty(&marker)?),
        )?;

        if scen.idle_seconds > 0 {
            eprintln!(
                "bpb bench: profile '{}': sleeping {}s idle before sample",
                profile.id, scen.idle_seconds
            );
            thread::sleep(Duration::from_secs(scen.idle_seconds));
        }

        let sample_path = run_dir.join("sample.json");
        sample::run(browser, Some(&sample_path), true)?;

        let classified_path = run_dir.join("classified.json");
        classify::run(&sample_path, Some(&classified_path))?;
        report::run(&classified_path, &run_dir, true)?;

        let summary = summarize_profile(&profile.id, &profile.description, &profile.args, &classified_path)?;
        summaries.push(summary);

        eprintln!(
            "bpb bench: profile '{}': wrote sample/classified/report under {}",
            profile.id,
            run_dir.display()
        );
    }

    if multi {
        write_compare(out_dir, &scen.id, browser, &summaries)?;
        eprintln!(
            "bpb bench: wrote compare.json / compare.md under {}",
            out_dir.display()
        );
    }

    Ok(())
}

fn security_notes_for(args: &[String]) -> Vec<String> {
    let mut notes = vec![
        "Site Isolation remains the default security posture; this lab measures process-reuse / density policies within that constraint.".to_string(),
    ];
    let joined = args.join(" ");
    if joined.contains("process-per-site") {
        notes.push(
            "--process-per-site consolidates same-site renderers (fate-sharing within a site); it does not disable Site Isolation."
                .into(),
        );
    }
    if joined.contains("renderer-process-limit") {
        notes.push(
            "--renderer-process-limit=N is a soft cap that can increase tab contention / fate-sharing under load; measure before deploying."
                .into(),
        );
    }
    if joined.contains("disable-site-isolation")
        || joined.contains("no-sandbox")
        || joined.contains("single-process")
    {
        notes.push(
            "WARNING: args include flags that weaken isolation or sandboxing. Not a recommended default for RDS; document risk acceptance if used."
                .into(),
        );
    }
    if args.is_empty() {
        notes.push("Baseline: no extra Chromium flags; default Site Isolation.".into());
    }
    notes
}

/// Best-effort browser launch. Never fails the bench — logs and continues.
fn maybe_launch_browser(browser: &str, urls: &[String], args: &[String]) -> String {
    if urls.is_empty() && args.is_empty() {
        return "skipped_no_urls_or_args".into();
    }

    let candidates = browser_binaries(browser);
    if candidates.is_empty() {
        eprintln!("bpb bench: no browser binary candidates for '{browser}'; sampling existing processes");
        return "skipped_no_binary_candidates".into();
    }

    for bin in &candidates {
        let mut cmd = Command::new(bin);
        cmd.args(args);
        for url in urls {
            cmd.arg(url);
        }
        // Detach: don't inherit stdio; don't wait.
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        match cmd.spawn() {
            Ok(child) => {
                eprintln!(
                    "bpb bench: launched {} (pid {:?}) with {} arg(s) and {} URL(s)",
                    bin,
                    child.id(),
                    args.len(),
                    urls.len()
                );
                // Intentionally leak/drop child — browser stays up for sampling.
                std::mem::forget(child);
                return format!("launched:{bin}");
            }
            Err(e) => {
                eprintln!("bpb bench: failed to launch {bin}: {e}");
            }
        }
    }

    eprintln!(
        "bpb bench: warning: could not launch browser; continuing with sample of existing processes"
    );
    "failed_continuing_with_existing".into()
}

fn browser_binaries(browser: &str) -> Vec<String> {
    let family = browser.to_ascii_lowercase();
    #[cfg(windows)]
    {
        let mut out = Vec::new();
        match family.as_str() {
            "chrome" | "google-chrome" | "chromium" | "auto" => {
                out.extend([
                    r"C:\Program Files\Google\Chrome\Application\chrome.exe".into(),
                    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe".into(),
                    "chrome.exe".into(),
                    "chrome".into(),
                ]);
            }
            _ => {}
        }
        if matches!(family.as_str(), "edge" | "msedge" | "auto") {
            out.extend([
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".into(),
                r"C:\Program Files\Microsoft\Edge\Application\msedge.exe".into(),
                "msedge.exe".into(),
                "msedge".into(),
            ]);
        }
        out
    }
    #[cfg(not(windows))]
    {
        let mut out = Vec::new();
        match family.as_str() {
            "chrome" | "google-chrome" | "chromium" | "auto" => {
                out.extend([
                    "google-chrome".into(),
                    "google-chrome-stable".into(),
                    "chromium".into(),
                    "chromium-browser".into(),
                    "chrome".into(),
                ]);
            }
            "edge" | "msedge" => {
                out.extend(["microsoft-edge".into(), "msedge".into()]);
            }
            _ => {}
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct RoleCountWs {
    role: String,
    count: usize,
    working_set_bytes: u64,
}

#[derive(Debug, Serialize)]
struct ProfileSummary {
    profile_id: String,
    description: String,
    args: Vec<String>,
    process_count: usize,
    total_ws_bytes: u64,
    aggregates_by_role: Vec<RoleCountWs>,
}

fn summarize_profile(
    id: &str,
    description: &str,
    args: &[String],
    classified_path: &Path,
) -> Result<ProfileSummary> {
    let raw = fs::read_to_string(classified_path)
        .with_context(|| format!("reading {}", classified_path.display()))?;
    let doc: crate::model::ClassifiedDocument =
        serde_json::from_str(&raw).context("parsing classified.json for compare")?;
    let total_ws_bytes = doc
        .processes
        .iter()
        .map(|p| p.working_set_bytes)
        .fold(0u64, u64::saturating_add);
    let aggregates_by_role = doc
        .aggregates
        .iter()
        .map(|a| RoleCountWs {
            role: a.role.as_str().to_string(),
            count: a.count,
            working_set_bytes: a.working_set_bytes,
        })
        .collect();
    Ok(ProfileSummary {
        profile_id: id.to_string(),
        description: description.to_string(),
        args: args.to_vec(),
        process_count: doc.processes.len(),
        total_ws_bytes,
        aggregates_by_role,
    })
}

fn write_compare(
    out_dir: &Path,
    scenario_id: &str,
    browser: &str,
    summaries: &[ProfileSummary],
) -> Result<()> {
    let compare = json!({
        "scenario_id": scenario_id,
        "browser": browser,
        "profiles": summaries,
        "security_reminder": "Compare process density under Site Isolation. Do not treat lower process counts from disabling isolation as a recommended default.",
    });
    fs::write(
        out_dir.join("compare.json"),
        format!("{}\n", serde_json::to_string_pretty(&compare)?),
    )?;

    let mut md = String::new();
    md.push_str("# Profile A/B compare\n\n");
    md.push_str(&format!("- **Scenario:** `{scenario_id}`\n"));
    md.push_str(&format!("- **Browser:** `{browser}`\n\n"));
    md.push_str("| Profile | Args | Processes | Total WS |\n");
    md.push_str("| --- | --- | ---: | ---: |\n");
    for s in summaries {
        let args = if s.args.is_empty() {
            "*(none)*".to_string()
        } else {
            format!("`{}`", s.args.join(" "))
        };
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            s.profile_id,
            args,
            s.process_count,
            format_bytes(s.total_ws_bytes)
        ));
    }
    md.push('\n');
    md.push_str("## Per-role (by profile)\n\n");
    for s in summaries {
        md.push_str(&format!("### {}\n\n", s.profile_id));
        if s.description.is_empty() {
            // skip
        } else {
            md.push_str(&format!("{}\n\n", s.description));
        }
        md.push_str("| Role | Count | Working set |\n");
        md.push_str("| --- | ---: | ---: |\n");
        for a in &s.aggregates_by_role {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                a.role,
                a.count,
                format_bytes(a.working_set_bytes)
            ));
        }
        md.push('\n');
    }
    md.push_str(
        "> Security: keep Site Isolation. Profile flags such as `--process-per-site` or `--renderer-process-limit` trade density vs. fate-sharing; they are measurement knobs, not recommendations to disable isolation.\n",
    );
    fs::write(out_dir.join("compare.md"), md)?;
    Ok(())
}

fn format_bytes(n: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let f = n as f64;
    if f >= GIB {
        format!("{:.2} GiB", f / GIB)
    } else if f >= MIB {
        format!("{:.2} MiB", f / MIB)
    } else if f >= KIB {
        format!("{:.2} KiB", f / KIB)
    } else {
        format!("{n} B")
    }
}

fn resolve_scenario_path(scenario: &str) -> Result<PathBuf> {
    let name = if scenario.ends_with(".toml") {
        scenario.to_string()
    } else {
        format!("{scenario}.toml")
    };

    let candidates = [
        PathBuf::from("scripts").join(&name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join(&name),
        PathBuf::from(&name),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    bail!(
        "scenario script not found for '{scenario}' (tried scripts/{name} and crate scripts/)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scenario_without_profiles() {
        let raw = r#"
id = "office-basic"
description = "x"
browser = "chrome"
urls = ["https://example.com"]
idle_seconds = 1
"#;
        let s: Scenario = toml::from_str(raw).unwrap();
        assert!(s.profiles.is_empty());
        assert_eq!(s.id, "office-basic");
    }

    #[test]
    fn parse_scenario_with_profiles() {
        let raw = r#"
id = "office-basic"
browser = "chrome"
urls = []
idle_seconds = 0

[[profiles]]
id = "baseline"
description = "Default Site Isolation"
args = []

[[profiles]]
id = "process-per-site"
description = "Same-site consolidation"
args = ["--process-per-site"]

[[profiles]]
id = "renderer-limit-4"
description = "Soft renderer cap"
args = ["--renderer-process-limit=4"]
"#;
        let s: Scenario = toml::from_str(raw).unwrap();
        assert_eq!(s.profiles.len(), 3);
        assert_eq!(s.profiles[1].args, vec!["--process-per-site"]);
        assert_eq!(s.profiles[2].args, vec!["--renderer-process-limit=4"]);
    }

    #[test]
    fn security_notes_flag_warnings() {
        let notes = security_notes_for(&["--process-per-site".into()]);
        assert!(notes.iter().any(|n| n.contains("process-per-site")));
        let warn = security_notes_for(&["--no-sandbox".into()]);
        assert!(warn.iter().any(|n| n.contains("WARNING")));
    }
}
