//! Emit machine JSON + human Markdown reports from classified samples.

use crate::model::{ClassifiedDocument, HostInfo, SampleDocument};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(input: &Path, out_dir: &Path, redact: bool) -> Result<()> {
    let raw = fs::read_to_string(input)
        .with_context(|| format!("reading {}", input.display()))?;

    let mut classified = parse_classified_or_sample(&raw)?;
    if redact {
        redact_document(&mut classified);
    }

    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating {}", out_dir.display()))?;

    let report_json_path = out_dir.join("report.json");
    let mut summary = build_summary(&classified);
    summary.redacted = redact;
    fs::write(
        &report_json_path,
        format!("{}\n", serde_json::to_string_pretty(&summary)?),
    )
    .with_context(|| format!("writing {}", report_json_path.display()))?;

    let report_md_path = out_dir.join("report.md");
    let md = render_markdown(&classified, &summary, redact);
    fs::write(&report_md_path, md)
        .with_context(|| format!("writing {}", report_md_path.display()))?;

    Ok(())
}

fn parse_classified_or_sample(raw: &str) -> Result<ClassifiedDocument> {
    // Prefer full classified documents; otherwise wrap a sample.
    if let Ok(doc) = serde_json::from_str::<ClassifiedDocument>(raw) {
        if doc.processes.iter().any(|p| p.role.is_some()) || !doc.aggregates.is_empty() {
            return Ok(if doc.aggregates.is_empty() {
                let mut d = doc;
                d.aggregates = crate::model::aggregate_by_role(&d.processes);
                d
            } else {
                doc
            });
        }
    }
    let sample: SampleDocument =
        serde_json::from_str(raw).context("parsing classified/sample JSON")?;
    Ok(ClassifiedDocument::from_sample(sample))
}

#[derive(Debug, serde::Serialize)]
struct ReportSummary {
    version: String,
    captured_at: String,
    host: HostInfo,
    browser: crate::model::BrowserInfo,
    process_count: usize,
    total_working_set_bytes: u64,
    aggregates: Vec<crate::model::RoleAggregate>,
    redacted: bool,
}

fn build_summary(doc: &ClassifiedDocument) -> ReportSummary {
    let total_working_set_bytes = doc
        .processes
        .iter()
        .map(|p| p.working_set_bytes)
        .fold(0u64, u64::saturating_add);
    ReportSummary {
        version: doc.version.clone(),
        captured_at: doc.captured_at.clone(),
        host: doc.host.clone(),
        browser: doc.browser.clone(),
        process_count: doc.processes.len(),
        total_working_set_bytes,
        aggregates: doc.aggregates.clone(),
        redacted: false,
    }
}

fn redact_document(doc: &mut ClassifiedDocument) {
    doc.host.hostname = doc
        .host
        .hostname
        .as_ref()
        .map(|_| "[redacted]".to_string());
    for proc in &mut doc.processes {
        proc.command_line = redact_text(&proc.command_line);
    }
}

fn redact_text(input: &str) -> String {
    let mut s = input.to_string();

    s = replace_windows_user_profile(&s);

    if let Some(stripped) = strip_home_path(&s) {
        s = stripped;
    }

    s = redact_segment_after(&s, "/home/");
    s = redact_segment_after(&s, "/Users/");

    s
}

fn replace_windows_user_profile(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let needle = r"c:\users\";
    let mut out = String::new();
    let mut cursor = 0usize;
    let mut search_from = 0usize;
    while let Some(rel) = lower[search_from..].find(needle) {
        let idx = search_from + rel;
        out.push_str(&input[cursor..idx]);
        // Preserve original casing of "C:\Users\" (or whatever matched).
        out.push_str(&input[idx..idx + needle.len()]);
        out.push_str("[redacted]");
        let after_start = idx + needle.len();
        let after = &input[after_start..];
        let end = after
            .find(|c: char| c == '\\' || c == '/' || c.is_whitespace() || c == '"')
            .unwrap_or(after.len());
        // Skip username (or an already-redacted marker).
        cursor = after_start + end;
        search_from = cursor;
    }
    out.push_str(&input[cursor..]);
    out
}

fn strip_home_path(input: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() || !input.contains(&home) {
        return None;
    }
    Some(input.replace(&home, "/home/[redacted]"))
}

fn redact_segment_after(input: &str, marker: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(idx) = rest.find(marker) {
        out.push_str(&rest[..idx]);
        out.push_str(marker);
        out.push_str("[redacted]");
        let after = &rest[idx + marker.len()..];
        let end = after
            .find(|c: char| c == '/' || c == '\\' || c.is_whitespace() || c == '"')
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn render_markdown(doc: &ClassifiedDocument, summary: &ReportSummary, redact: bool) -> String {
    let mut md = String::new();
    md.push_str("# browser-process-budget report\n\n");
    md.push_str(&format!("- **Captured at:** {}\n", doc.captured_at));
    md.push_str(&format!("- **OS:** {}\n", doc.host.os));
    let host = doc.host.hostname.as_deref().unwrap_or("(none)");
    md.push_str(&format!("- **Hostname:** {host}\n"));
    md.push_str(&format!(
        "- **Browser:** {}{}\n",
        doc.browser.family,
        doc.browser
            .version
            .as_ref()
            .map(|v| format!(" ({v})"))
            .unwrap_or_default()
    ));
    md.push_str(&format!("- **Processes:** {}\n", summary.process_count));
    md.push_str(&format!(
        "- **Total working set:** {}\n",
        format_bytes(summary.total_working_set_bytes)
    ));
    md.push_str(&format!("- **Redacted:** {redact}\n\n"));

    md.push_str("## Per-role aggregates\n\n");
    md.push_str("| Role | Count | Working set |\n");
    md.push_str("| --- | ---: | ---: |\n");
    for agg in &doc.aggregates {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            agg.role.as_str(),
            agg.count,
            format_bytes(agg.working_set_bytes)
        ));
    }
    md.push('\n');

    md.push_str("## Processes\n\n");
    md.push_str("| PID | PPID | Role | Name | Working set | Command line |\n");
    md.push_str("| ---: | ---: | --- | --- | ---: | --- |\n");
    for proc in &doc.processes {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            proc.pid,
            proc.ppid,
            proc.role.as_ref().map(|r| r.as_str()).unwrap_or("?"),
            escape_md_cell(&proc.name),
            format_bytes(proc.working_set_bytes),
            escape_md_cell(&truncate(&proc.command_line, 120)),
        ));
    }
    md.push('\n');
    md
}

fn format_bytes(n: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let f = n as f64;
    if f >= GIB {
        format!("{:.2} GiB ({n} B)", f / GIB)
    } else if f >= MIB {
        format!("{:.2} MiB ({n} B)", f / MIB)
    } else if f >= KIB {
        format!("{:.2} KiB ({n} B)", f / KIB)
    } else {
        format!("{n} B")
    }
}

fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_unix_home() {
        let s = redact_text("/home/alice/.config/google-chrome --type=renderer");
        assert!(s.contains("/home/[redacted]"), "{s}");
        assert!(!s.contains("alice"), "{s}");
    }

    #[test]
    fn redacts_windows_users() {
        let s = redact_text(r"C:\Users\Bob\AppData\Local\Google\Chrome\Application\chrome.exe");
        assert!(s.contains(r"C:\Users\[redacted]"), "{s}");
        assert!(!s.contains("Bob"), "{s}");
    }
}
