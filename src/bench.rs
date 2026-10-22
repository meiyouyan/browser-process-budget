//! Run a named scenario script then sample → classify → report.

use crate::{classify, report, sample};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
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
}

pub fn run(scenario: &str, browser: &str, out_dir: &Path) -> Result<()> {
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

    // Marker with scenario meta (always written).
    let marker = serde_json::json!({
        "scenario_id": scen.id,
        "description": scen.description,
        "browser": browser,
        "urls": scen.urls,
        "idle_seconds": scen.idle_seconds,
        "script_path": script_path.display().to_string(),
        "url_launch": "skipped_best_effort",
        "notes": "URL launch is best-effort and skipped in this build; idle sleep still applies.",
    });
    fs::write(
        out_dir.join("scenario-marker.json"),
        format!("{}\n", serde_json::to_string_pretty(&marker)?),
    )?;

    if !scen.urls.is_empty() {
        eprintln!(
            "bpb bench: {} URL(s) listed; launch skipped (best-effort). Sleeping {}s idle.",
            scen.urls.len(),
            scen.idle_seconds
        );
    }

    if scen.idle_seconds > 0 {
        thread::sleep(Duration::from_secs(scen.idle_seconds));
    }

    let sample_path = out_dir.join("sample.json");
    sample::run(browser, Some(&sample_path), true)?;

    let classified_path = out_dir.join("classified.json");
    classify::run(&sample_path, Some(&classified_path))?;
    report::run(&classified_path, out_dir, true)?;

    eprintln!(
        "bpb bench: wrote sample/classified/report under {}",
        out_dir.display()
    );
    Ok(())
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
