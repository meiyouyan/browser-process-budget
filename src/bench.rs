//! Run a named scenario script then sample → classify → report.
//!
//! Scenarios may declare `[[profiles]]` (Chrome/Edge launch arg sets). When
//! present, each profile gets its own `out_dir/<profile_id>/` tree and a
//! cross-profile `compare.json` / `compare.md` is written at `out_dir/`.
//! Multi-profile runs isolate each launch with a dedicated `--user-data-dir`,
//! stop matching browser processes between profiles, and filter the sample to
//! that directory so A/B numbers do not stack.
//! Without profiles, behavior matches the original single-run layout.

use crate::{classify, report, sample};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

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
    /// Optional Chromium Preferences JSON fragment merged into
    /// `user-data/Default/Preferences` before launch (e.g. Memory Saver prefs).
    #[serde(default)]
    preferences: Option<serde_json::Value>,
}

/// Default profile used when a scenario has no `[[profiles]]` (backward compat).
fn implicit_baseline() -> Profile {
    Profile {
        id: "baseline".into(),
        description: "Default Site Isolation (no extra args)".into(),
        args: vec![],
        preferences: None,
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

        // Multi-profile A/B: isolate each launch so samples do not stack.
        let user_data_dir = if multi {
            let ud = run_dir.join("user-data");
            fs::create_dir_all(&ud)
                .with_context(|| format!("creating {}", ud.display()))?;
            let ud = ud.canonicalize().unwrap_or_else(|_| ud.clone());
            let stopped = stop_browser_for_user_data(browser, &ud);
            eprintln!(
                "bpb bench: profile '{}': pre-launch cleanup stopped {stopped} process(es) for {}",
                profile.id,
                ud.display()
            );
            Some(ud)
        } else {
            None
        };

        if let (Some(ref ud), Some(ref prefs)) = (&user_data_dir, &profile.preferences) {
            seed_preferences(ud, prefs).with_context(|| {
                format!(
                    "seeding Preferences for profile '{}' under {}",
                    profile.id,
                    ud.display()
                )
            })?;
            eprintln!(
                "bpb bench: profile '{}': seeded Preferences under {}/Default",
                profile.id,
                ud.display()
            );
        } else if profile.preferences.is_some() && user_data_dir.is_none() {
            eprintln!(
                "bpb bench: profile '{}': preferences set but no user-data-dir (single-run); skipping pref seed",
                profile.id
            );
        }

        let mut launch_args = profile.args.clone();
        if let Some(ref ud) = user_data_dir {
            launch_args.push(format!("--user-data-dir={}", ud.display()));
            launch_args.push("--no-first-run".into());
            launch_args.push("--no-default-browser-check".into());
        }

        let launch_status = maybe_launch_browser(browser, &scen.urls, &launch_args);

        let security_notes = security_notes_for(&profile.args, profile.preferences.as_ref());

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
            "preferences": profile.preferences,
            "launch_args": launch_args,
            "user_data_dir": user_data_dir.as_ref().map(|p| p.display().to_string()),
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
        if let Some(ref ud) = user_data_dir {
            let kept = filter_sample_to_user_data(&sample_path, ud)?;
            eprintln!(
                "bpb bench: profile '{}': sample kept {kept} process(es) matching user-data-dir",
                profile.id
            );
        }

        let classified_path = run_dir.join("classified.json");
        classify::run(&sample_path, Some(&classified_path))?;
        report::run(&classified_path, &run_dir, true)?;

        let summary = summarize_profile(&profile.id, &profile.description, &profile.args, &classified_path)?;
        summaries.push(summary);

        if let Some(ref ud) = user_data_dir {
            let stopped = stop_browser_for_user_data(browser, ud);
            eprintln!(
                "bpb bench: profile '{}': post-sample cleanup stopped {stopped} process(es)",
                profile.id
            );
            // Brief pause so the next profile does not race teardown.
            thread::sleep(Duration::from_secs(1));
        }

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

fn security_notes_for(args: &[String], preferences: Option<&serde_json::Value>) -> Vec<String> {
    let mut notes = vec![
        "Site Isolation remains the default security posture; this lab measures process-reuse / density policies within that constraint.".to_string(),
    ];
    let joined = args.join(" ");
    let prefs_text = preferences
        .map(|v| v.to_string())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mentions_memory_saver = joined.to_ascii_lowercase().contains("highefficiencymode")
        || joined.to_ascii_lowercase().contains("high_efficiency")
        || joined.to_ascii_lowercase().contains("memory saver")
        || joined.to_ascii_lowercase().contains("memory-saver")
        || prefs_text.contains("high_efficiency")
        || prefs_text.contains("highefficiencymode")
        || prefs_text.contains("time_before_discard")
        || prefs_text.contains("aggressiveness");
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
    if mentions_memory_saver {
        notes.push(
            "Memory Saver / high-efficiency discard can drop inactive tabs (they reload on focus), which may understate steady-state memory until reactivation; it does not disable Site Isolation."
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
    if args.is_empty() && preferences.is_none() {
        notes.push("Baseline: no extra Chromium flags; default Site Isolation.".into());
    }
    notes
}

/// Deep-merge `overlay` into `base` (objects merge recursively; other types overwrite).
fn merge_json(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                match base_map.get_mut(k) {
                    Some(existing) => merge_json(existing, v),
                    None => {
                        base_map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (base_slot, overlay_val) => {
            *base_slot = overlay_val.clone();
        }
    }
}

/// Merge profile preferences into `user_data_dir/Default/Preferences` (create dirs/file as needed).
fn seed_preferences(user_data_dir: &Path, preferences: &serde_json::Value) -> Result<()> {
    let default_dir = user_data_dir.join("Default");
    fs::create_dir_all(&default_dir)
        .with_context(|| format!("creating {}", default_dir.display()))?;
    let prefs_path = default_dir.join("Preferences");
    let mut doc = if prefs_path.is_file() {
        let raw = fs::read_to_string(&prefs_path)
            .with_context(|| format!("reading {}", prefs_path.display()))?;
        serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };
    if !doc.is_object() {
        doc = json!({});
    }
    merge_json(&mut doc, preferences);
    fs::write(
        &prefs_path,
        format!("{}
", serde_json::to_string_pretty(&doc)?),
    )
    .with_context(|| format!("writing {}", prefs_path.display()))?;
    Ok(())
}


/// Normalize a path for substring matching in Chromium command lines.
fn normalize_path_needle(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn process_name_matches_browser(browser: &str, name: &OsStr) -> bool {
    let n = name.to_string_lossy().to_ascii_lowercase();
    let base = n
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&n);
    let base = base.trim_end_matches(".exe");
    let family = browser.to_ascii_lowercase();
    match family.as_str() {
        "chrome" | "google-chrome" | "chromium" => matches!(
            base,
            "chrome" | "google-chrome" | "google-chrome-stable" | "chromium" | "chromium-browser"
        ),
        "edge" | "msedge" => matches!(base, "msedge" | "microsoft-edge" | "microsoftedge"),
        "auto" => matches!(
            base,
            "chrome"
                | "google-chrome"
                | "google-chrome-stable"
                | "chromium"
                | "chromium-browser"
                | "msedge"
                | "microsoft-edge"
                | "microsoftedge"
        ),
        _ => false,
    }
}

/// Stop browser processes whose command line references `user_data_dir`.
/// Scoped cleanup: does not kill unrelated Chrome/Edge instances.
fn stop_browser_for_user_data(browser: &str, user_data_dir: &Path) -> usize {
    let needle = normalize_path_needle(user_data_dir);
    let needle_win = needle.replace('/', "\\");
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_exe(UpdateKind::OnlyIfNotSet),
    );
    let mut stopped = 0usize;
    for process in sys.processes().values() {
        if !process_name_matches_browser(browser, process.name()) {
            continue;
        }
        let cmd_parts: Vec<String> = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .collect();
        let cmd_joined = cmd_parts.join(" ");
        let cmd_norm = cmd_joined.replace('\\', "/");
        if !(cmd_norm.contains(&needle)
            || cmd_joined.contains(&needle_win)
            || cmd_joined.contains(&needle))
        {
            continue;
        }
        if process.kill() {
            stopped += 1;
        }
    }
    if stopped > 0 {
        thread::sleep(Duration::from_millis(500));
    }
    stopped
}

/// Keep only sampled processes whose command_line references `user_data_dir`.
/// If none match (cmdline unavailable), leave the sample unchanged and warn.
fn filter_sample_to_user_data(sample_path: &Path, user_data_dir: &Path) -> Result<usize> {
    let raw = fs::read_to_string(sample_path)
        .with_context(|| format!("reading {}", sample_path.display()))?;
    let mut doc: crate::model::SampleDocument =
        serde_json::from_str(&raw).context("parsing sample.json for user-data filter")?;
    let needle = normalize_path_needle(user_data_dir);
    let needle_win = needle.replace('/', "\\");
    let before = doc.processes.len();
    doc.processes.retain(|p| {
        let cl = p.command_line.to_ascii_lowercase();
        let cl_norm = cl.replace('\\', "/");
        cl_norm.contains(&needle) || cl.contains(&needle_win) || cl.contains(&needle)
    });
    if doc.processes.is_empty() && before > 0 {
        eprintln!(
            "bpb bench: warning: user-data-dir filter matched 0/{before} processes; keeping unfiltered sample"
        );
        return Ok(before);
    }
    fs::write(
        sample_path,
        format!("{}\n", serde_json::to_string_pretty(&doc)?),
    )?;
    Ok(doc.processes.len())
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
    fn normalize_path_needle_lowercases_and_slashes() {
        let n = normalize_path_needle(Path::new("Foo/Bar\\Baz"));
        assert_eq!(n, "foo/bar/baz");
    }

    #[test]
    fn security_notes_flag_warnings() {
        let notes = security_notes_for(&["--process-per-site".into()], None);
        assert!(notes.iter().any(|n| n.contains("process-per-site")));
        let warn = security_notes_for(&["--no-sandbox".into()], None);
        assert!(warn.iter().any(|n| n.contains("WARNING")));
    }

    #[test]
    fn security_notes_memory_saver_from_prefs() {
        let prefs = json!({
            "performance_tuning": {
                "high_efficiency_mode": {
                    "state": 2,
                    "time_before_discard_in_minutes": 1,
                    "aggressiveness": 2
                }
            }
        });
        let notes = security_notes_for(&[], Some(&prefs));
        assert!(notes.iter().any(|n| n.contains("Memory Saver")));
        assert!(notes.iter().any(|n| n.contains("does not disable Site Isolation")));
        assert!(!notes.iter().any(|n| n.contains("WARNING")));
    }

    #[test]
    fn security_notes_memory_saver_from_args() {
        let notes = security_notes_for(
            &["--enable-features=HighEfficiencyModeAvailable:time_before_discard/30s/default_state/true".into()],
            None,
        );
        assert!(notes.iter().any(|n| n.contains("Memory Saver")));
    }

    #[test]
    fn parse_scenario_with_preferences() {
        let raw = r#"
id = "office-memory-ab"
browser = "chrome"
urls = []
idle_seconds = 0

[[profiles]]
id = "baseline"
args = []

[[profiles]]
id = "memory-saver-aggressive"
args = ["--enable-features=HighEfficiencyModeAvailable:time_before_discard/30s/default_state/true"]

[profiles.preferences.performance_tuning.high_efficiency_mode]
state = 2
time_before_discard_in_minutes = 1
aggressiveness = 2
"#;
        let s: Scenario = toml::from_str(raw).unwrap();
        assert_eq!(s.profiles.len(), 2);
        assert!(s.profiles[0].preferences.is_none());
        let prefs = s.profiles[1].preferences.as_ref().expect("prefs");
        assert_eq!(
            prefs["performance_tuning"]["high_efficiency_mode"]["state"],
            2
        );
        assert_eq!(
            prefs["performance_tuning"]["high_efficiency_mode"]["time_before_discard_in_minutes"],
            1
        );
        assert_eq!(
            prefs["performance_tuning"]["high_efficiency_mode"]["aggressiveness"],
            2
        );
    }

    #[test]
    fn merge_json_nested_preserves_siblings() {
        let mut base = json!({
            "performance_tuning": {
                "high_efficiency_mode": { "state": 0, "keep_me": true },
                "other": 1
            },
            "bookmark_bar": { "show_on_all_tabs": true }
        });
        let overlay = json!({
            "performance_tuning": {
                "high_efficiency_mode": {
                    "state": 2,
                    "time_before_discard_in_minutes": 1,
                    "aggressiveness": 2
                }
            }
        });
        merge_json(&mut base, &overlay);
        assert_eq!(base["performance_tuning"]["high_efficiency_mode"]["state"], 2);
        assert_eq!(
            base["performance_tuning"]["high_efficiency_mode"]["time_before_discard_in_minutes"],
            1
        );
        assert_eq!(base["performance_tuning"]["high_efficiency_mode"]["keep_me"], true);
        assert_eq!(base["performance_tuning"]["other"], 1);
        assert_eq!(base["bookmark_bar"]["show_on_all_tabs"], true);
    }

    #[test]
    fn seed_preferences_merges_existing_file() {
        let dir = std::env::temp_dir().join(format!(
            "bpb-prefs-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Default")).unwrap();
        let prefs_path = dir.join("Default").join("Preferences");
        fs::write(
            &prefs_path,
            r#"{"bookmark_bar":{"show_on_all_tabs":true},"performance_tuning":{"high_efficiency_mode":{"state":0}}}"#,
        )
        .unwrap();
        let overlay = json!({
            "performance_tuning": {
                "high_efficiency_mode": {
                    "state": 2,
                    "time_before_discard_in_minutes": 1,
                    "aggressiveness": 2
                }
            }
        });
        seed_preferences(&dir, &overlay).unwrap();
        let raw = fs::read_to_string(&prefs_path).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(doc["bookmark_bar"]["show_on_all_tabs"], true);
        assert_eq!(doc["performance_tuning"]["high_efficiency_mode"]["state"], 2);
        assert_eq!(
            doc["performance_tuning"]["high_efficiency_mode"]["time_before_discard_in_minutes"],
            1
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_preferences_creates_default_dir() {
        let dir = std::env::temp_dir().join(format!(
            "bpb-prefs-create-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let overlay = json!({
            "performance_tuning": {
                "high_efficiency_mode": { "state": 0 }
            }
        });
        seed_preferences(&dir, &overlay).unwrap();
        let prefs_path = dir.join("Default").join("Preferences");
        assert!(prefs_path.is_file());
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&prefs_path).unwrap()).unwrap();
        assert_eq!(doc["performance_tuning"]["high_efficiency_mode"]["state"], 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
