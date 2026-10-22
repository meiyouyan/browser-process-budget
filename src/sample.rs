//! Process sampling: Windows (primary) + Linux smoke via /proc.

use crate::model::{
    BrowserInfo, HostInfo, ProcessRecord, SampleDocument, SCHEMA_VERSION,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn run(browser: &str, out: Option<&Path>, session: bool) -> Result<()> {
    let doc = collect_sample(browser, session)?;
    write_sample(&doc, out)
}

pub fn collect_sample(browser: &str, session: bool) -> Result<SampleDocument> {
    let family = normalize_browser_family(browser)?;
    #[cfg(windows)]
    {
        windows_impl::sample(&family, session)
    }
    #[cfg(not(windows))]
    {
        let _ = session; // session id is a Windows RDS concept
        linux_impl::sample(&family)
    }
}

fn normalize_browser_family(browser: &str) -> Result<String> {
    match browser.to_ascii_lowercase().as_str() {
        "chrome" | "google-chrome" | "chromium" => Ok("chrome".into()),
        "edge" | "msedge" => Ok("edge".into()),
        "auto" => Ok("auto".into()),
        other => bail!("unsupported --browser value: {other} (use chrome|edge|auto)"),
    }
}

fn write_sample(doc: &SampleDocument, out: Option<&Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(doc)?;
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            let mut f = fs::File::create(path)
                .with_context(|| format!("creating {}", path.display()))?;
            writeln!(f, "{json}")?;
        }
        None => println!("{json}"),
    }
    Ok(())
}

fn host_info() -> HostInfo {
    HostInfo {
        os: std::env::consts::OS.to_string(),
        hostname: hostname_best_effort(),
    }
}

fn hostname_best_effort() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| sysinfo::System::host_name())
}

fn browser_matches(family: &str, name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let base = n.rsplit(['/', '\\']).next().unwrap_or(&n);
    match family {
        "chrome" => matches!(
            base,
            "chrome"
                | "chrome.exe"
                | "google-chrome"
                | "google-chrome-stable"
                | "chromium"
                | "chromium-browser"
                | "chrome_crashpad_handler"
        ),
        "edge" => matches!(base, "msedge" | "msedge.exe"),
        "auto" => browser_matches("chrome", name) || browser_matches("edge", name),
        _ => false,
    }
}

fn resolved_family(requested: &str, processes: &[ProcessRecord]) -> String {
    if requested != "auto" {
        return requested.to_string();
    }
    let has_edge = processes.iter().any(|p| {
        let n = p.name.to_ascii_lowercase();
        n.contains("msedge")
    });
    let has_chrome = processes.iter().any(|p| {
        let n = p.name.to_ascii_lowercase();
        n.contains("chrome") || n.contains("chromium")
    });
    match (has_chrome, has_edge) {
        (true, false) => "chrome".into(),
        (false, true) => "edge".into(),
        (true, true) => "auto".into(),
        _ => "auto".into(),
    }
}

#[cfg(not(windows))]
mod linux_impl {
    use super::*;
    use std::path::Path;

    pub fn sample(family: &str) -> Result<SampleDocument> {
        let mut processes = Vec::new();
        let proc_dir = fs::read_dir("/proc").context("reading /proc")?;
        for entry in proc_dir.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let pid: u32 = match name.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let proc_path = entry.path();
            let cmdline = read_cmdline(&proc_path.join("cmdline")).unwrap_or_default();
            let comm = read_comm(&proc_path.join("comm")).unwrap_or_default();
            let exe_name = read_exe_basename(&proc_path.join("exe")).unwrap_or_else(|| comm.clone());

            if !browser_matches(family, &exe_name) && !browser_matches(family, &comm) {
                // Also accept when cmdline points at chrome/chromium even if comm is a wrapper.
                if !cmdline_looks_like_browser(family, &cmdline) {
                    continue;
                }
            }

            // Skip crashpad/zygote helpers only when family filter would still keep chrome name —
            // keep them as they are chrome-tree processes useful for overhead pictures.
            let (ppid, _) = read_stat_ppid(&proc_path.join("stat")).unwrap_or((0, 0));
            let rss_bytes = read_status_vmrss_bytes(&proc_path.join("status")).unwrap_or(0);

            let display_name = if !exe_name.is_empty() {
                exe_name
            } else if !comm.is_empty() {
                comm
            } else {
                "chrome".into()
            };

            processes.push(ProcessRecord {
                pid,
                ppid,
                name: display_name,
                working_set_bytes: rss_bytes,
                private_bytes: None,
                session_id: None,
                command_line: if cmdline.is_empty() {
                    display_name_fallback(pid)
                } else {
                    cmdline
                },
                role: None,
                notes: None,
            });
        }

        processes.sort_by_key(|p| p.pid);
        let browser_family = resolved_family(family, &processes);
        Ok(SampleDocument {
            version: SCHEMA_VERSION.into(),
            captured_at: Utc::now().to_rfc3339(),
            host: host_info(),
            browser: BrowserInfo {
                family: browser_family,
                version: detect_chrome_version_linux(),
            },
            processes,
        })
    }

    fn display_name_fallback(pid: u32) -> String {
        format!("(pid {pid})")
    }

    fn cmdline_looks_like_browser(family: &str, cmdline: &str) -> bool {
        // Only trust the executable token (argv0), not incidental path substrings.
        let argv0 = cmdline.split_whitespace().next().unwrap_or("");
        let base = Path::new(argv0)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(argv0);
        browser_matches(family, base)
    }

    fn read_cmdline(path: &Path) -> Result<String> {
        let bytes = fs::read(path)?;
        let s = bytes
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(s)
    }

    fn read_comm(path: &Path) -> Result<String> {
        Ok(fs::read_to_string(path)?.trim().to_string())
    }

    fn read_exe_basename(path: &Path) -> Option<String> {
        let target = fs::read_link(path).ok()?;
        target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
    }

    fn read_stat_ppid(path: &Path) -> Result<(u32, i64)> {
        let content = fs::read_to_string(path)?;
        // stat: pid (comm) state ppid ...
        let close = content
            .rfind(')')
            .context("malformed /proc/pid/stat (no closing paren)")?;
        let after = content[close + 1..].trim_start();
        let mut parts = after.split_whitespace();
        let _state = parts.next();
        let ppid: u32 = parts.next().context("missing ppid")?.parse()?;
        Ok((ppid, 0))
    }

    fn read_status_vmrss_bytes(path: &Path) -> Result<u64> {
        let content = fs::read_to_string(path)?;
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb: u64 = rest
                    .split_whitespace()
                    .next()
                    .context("VmRSS value")?
                    .parse()?;
                return Ok(kb.saturating_mul(1024));
            }
        }
        Ok(0)
    }

    fn detect_chrome_version_linux() -> Option<String> {
        for bin in [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
        ] {
            if let Ok(out) = std::process::Command::new(bin).arg("--version").output() {
                if out.status.success() {
                    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }
        None
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::mem::{size_of, zeroed};
    use sysinfo::{ProcessesToUpdate, System};
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    pub fn sample(family: &str, want_session: bool) -> Result<SampleDocument> {
        // Native enumeration for pid/ppid/name; memory via GetProcessMemoryInfo;
        // command line via sysinfo (PEB/NtQuery) for reliability.
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
            .context("CreateToolhelp32Snapshot")?;

        let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

        let mut processes = Vec::new();
        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
        while ok {
            let name = wchar_to_string(&entry.szExeFile);
            let pid = entry.th32ProcessID;
            let ppid = entry.th32ParentProcessID;
            if browser_matches(family, &name) {
                let (ws, private) = process_memory(pid);
                let session_id = if want_session {
                    process_session_id(pid)
                } else {
                    None
                };
                let command_line = command_line_for_pid(&sys, pid, &name);
                processes.push(ProcessRecord {
                    pid,
                    ppid,
                    name,
                    working_set_bytes: ws,
                    private_bytes: private,
                    session_id,
                    command_line,
                    role: None,
                    notes: None,
                });
            }
            ok = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
        }
        unsafe {
            let _ = CloseHandle(snapshot);
        }

        processes.sort_by_key(|p| p.pid);
        let browser_family = resolved_family(family, &processes);
        Ok(SampleDocument {
            version: SCHEMA_VERSION.into(),
            captured_at: Utc::now().to_rfc3339(),
            host: host_info(),
            browser: BrowserInfo {
                family: browser_family,
                version: None,
            },
            processes,
        })
    }

    fn wchar_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    fn process_memory(pid: u32) -> (u64, Option<u64>) {
        let access = PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION;
        let handle = unsafe { OpenProcess(access, false, pid) };
        let Ok(handle) = handle else {
            return (0, None);
        };
        let result = unsafe {
            let mut pmc: PROCESS_MEMORY_COUNTERS_EX = zeroed();
            pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            let ok = GetProcessMemoryInfo(
                handle,
                &mut pmc as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                pmc.cb,
            );
            let _ = CloseHandle(handle);
            if ok.is_ok() {
                (pmc.WorkingSetSize as u64, Some(pmc.PrivateUsage as u64))
            } else {
                (0, None)
            }
        };
        result
    }

    fn process_session_id(pid: u32) -> Option<u32> {
        let mut sid = 0u32;
        let ok = unsafe { ProcessIdToSessionId(pid, &mut sid) };
        if ok.is_ok() {
            Some(sid)
        } else {
            None
        }
    }

    fn command_line_for_pid(sys: &System, pid: u32, name: &str) -> String {
        use sysinfo::Pid;
        if let Some(proc) = sys.process(Pid::from_u32(pid)) {
            let cmd = proc.cmd();
            if !cmd.is_empty() {
                return cmd
                    .iter()
                    .map(|s| s.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            if let Some(exe) = proc.exe() {
                return exe.to_string_lossy().into_owned();
            }
        }
        // Fallback: image path via QueryFullProcessImageNameW
        if let Some(path) = query_image_name(pid) {
            return path;
        }
        name.to_string()
    }

    fn query_image_name(pid: u32) -> Option<String> {
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        let mut buf = [0u16; MAX_PATH as usize];
        let mut size = buf.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut size,
            )
        };
        unsafe {
            let _ = CloseHandle(handle);
        }
        if ok.is_ok() {
            Some(String::from_utf16_lossy(&buf[..size as usize]))
        } else {
            None
        }
    }
}
