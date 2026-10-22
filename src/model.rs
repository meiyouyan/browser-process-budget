//! Shared serde data model for sample / classify / report / bench.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    Browser,
    Renderer,
    Gpu,
    Utility,
    Extension,
    Other,
}

impl ProcessRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Browser => "Browser",
            Self::Renderer => "Renderer",
            Self::Gpu => "Gpu",
            Self::Utility => "Utility",
            Self::Extension => "Extension",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub os: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserInfo {
    pub family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRecord {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub working_set_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<u32>,
    pub command_line: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ProcessRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleDocument {
    pub version: String,
    /// RFC3339 timestamp
    pub captured_at: String,
    pub host: HostInfo,
    pub browser: BrowserInfo,
    pub processes: Vec<ProcessRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAggregate {
    pub role: ProcessRole,
    pub count: usize,
    pub working_set_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedDocument {
    pub version: String,
    pub captured_at: String,
    pub host: HostInfo,
    pub browser: BrowserInfo,
    pub processes: Vec<ProcessRecord>,
    #[serde(default)]
    pub aggregates: Vec<RoleAggregate>,
}

impl ClassifiedDocument {
    pub fn from_sample(mut sample: SampleDocument) -> Self {
        for proc in &mut sample.processes {
            if proc.role.is_none() {
                proc.role = Some(crate::classify::classify_command_line(&proc.name, &proc.command_line));
            }
        }
        let aggregates = aggregate_by_role(&sample.processes);
        Self {
            version: sample.version,
            captured_at: sample.captured_at,
            host: sample.host,
            browser: sample.browser,
            processes: sample.processes,
            aggregates,
        }
    }
}

pub fn aggregate_by_role(processes: &[ProcessRecord]) -> Vec<RoleAggregate> {
    let mut map: BTreeMap<String, (ProcessRole, usize, u64)> = BTreeMap::new();
    for proc in processes {
        let role = proc
            .role
            .clone()
            .unwrap_or(ProcessRole::Other);
        let key = role.as_str().to_string();
        let entry = map.entry(key).or_insert((role, 0, 0));
        entry.1 += 1;
        entry.2 = entry.2.saturating_add(proc.working_set_bytes);
    }
    map.into_values()
        .map(|(role, count, working_set_bytes)| RoleAggregate {
            role,
            count,
            working_set_bytes,
        })
        .collect()
}
