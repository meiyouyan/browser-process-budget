# Architecture

`browser-process-budget` (**bpb**) is the **RDS process-budget lab**: keep **Site Isolation**, and A/B Chromium process-reuse / Memory Saver / extension / session-density policies with **reproducible numbers**.

It is **not** a browser fork. It does **not** recommend disabling Site Isolation, `--no-sandbox`, or `--single-process` as defaults. Blink/Servo Rust rewrites and Chromium forks are **deferred**.

## 1. Problem

On Windows **RDS** (and similar multi-session hosts), Chromium’s process model multiplies cost:

| Factor | Why it hurts on RDS |
| --- | --- |
| **Processes** | Site Isolation → many renderers; each session adds another browser tree |
| **Working set (WS)** | Per-process baselines add up across users |
| **Sessions** | N concurrent users ≈ N × (browser + GPU + utilities + renderers) unless policies reuse/consolidate carefully |

Operators need **measured** trade-offs (process density vs. fate-sharing) under policies that **keep** Site Isolation — not folklore that “turn off isolation.”

## 2. Pipeline

```
sample  →  classify  →  report
                ↑
         bench (+ profiles / scenarios)
```

1. **sample** — Enumerate Chrome/Edge processes; capture PID, PPID, session id (Windows), working set, command line. Read-only.
2. **classify** — Map each process to Browser / Renderer / GPU / Utility / Extension / Other from cmdline heuristics.
3. **report** — Emit `report.json` + `report.md` (optional hostname/username redaction).
4. **bench** — Load a scenario TOML from `scripts/`, optional idle, optional best-effort browser launch, then sample→classify→report. With `[[profiles]]`, run once **per profile** into `out_dir/<profile_id>/` and write `compare.json` / `compare.md`.

## 3. Modules

| Module | Role | Status |
| --- | --- | --- |
| **sample** | OS process snapshot (`src/sample.rs`) | Implemented (Windows primary; Linux `/proc` smoke) |
| **classify** | Role labeling (`src/classify.rs`) | Implemented |
| **report** | JSON + Markdown (`src/report.rs`) | Implemented |
| **bench** | Scenario runner + **profile A/B** (`src/bench.rs`) | Implemented (key component this release) |
| **scenarios** | TOML scripts under `scripts/` | Implemented (`office-basic`, `office-basic-ab`, `office-dense-ab`, `office-samesite-ab`, …) |
| **profiles (A/B)** | Launch-arg sets declared in scenario TOML | **Implemented now** |
| **session aggregation** | Cross-session RDS rollups | Planned (roadmap) |
| **private-WS attribution** | Stronger private/PSS-like attribution | Planned (roadmap) |

Shared serde types live in `src/model.rs`. CLI wiring is `src/main.rs` (`bpb` binary).

## 4. Principles

1. **Read-only by default** — sampling and reporting must not change OS or browser policy unless the operator explicitly opts into a documented, risk-labeled launch profile.
2. **Never silently weaken Site Isolation** — default profiles keep isolation; docs and markers warn against `--disable-site-isolation-trials`, `--no-sandbox`, `--single-process` as defaults.
3. **Measure first** — A/B with numbers before recommending density knobs (`--process-per-site`, `--renderer-process-limit`, Memory Saver, extension policy).
4. **No fake benchmarks in docs** — publish only numbers produced by real runs on target hosts.

## 5. Component map

| Concern | Owning path |
| --- | --- |
| CLI | `src/main.rs` |
| Data model | `src/model.rs` |
| Sample (Win/Linux) | `src/sample.rs` |
| Classify + unit tests | `src/classify.rs` |
| Report + redaction | `src/report.rs` |
| Bench + **profile A/B** + compare | `src/bench.rs` |
| Scenarios | `scripts/*.toml` |
| Fixture smoke | `examples/sample-fixture.json`, `scripts/smoke-fixture.sh` |
| Framework docs | `docs/en/*`, `docs/zh/*` |
| Top-level security policy | `SECURITY.md` |

**Implemented now (key component):** process-model **profile A/B** in bench — scenarios declare `[[profiles]]` with `id` / `description` / `args`; each profile gets `out_dir/<id>/` (marker, sample, classified, report); multi-profile runs emit `out_dir/compare.json` and `compare.md`.

## 6. Profile A/B design (bench)

Example scenario fragment:

```toml
id = "office-basic"
browser = "chrome"
urls = ["https://example.com"]
idle_seconds = 5

[[profiles]]
id = "baseline"
description = "Default Site Isolation"
args = []

[[profiles]]
id = "process-per-site"
description = "Same-site consolidation via --process-per-site"
args = ["--process-per-site"]

[[profiles]]
id = "renderer-limit-4"
description = "Soft renderer cap"
args = ["--renderer-process-limit=4"]
```

Behavior:

- **No `[[profiles]]`** → single run; artifacts at `out_dir/` root (backward compatible).
- **With profiles** → for each profile: use a dedicated `--user-data-dir` (`out_dir/<id>/user-data`), stop browser processes whose command line matches that directory before/after the run; write `scenario-marker.json` (includes `profile_id`, `args`, `launch_args`, `user_data_dir`, `security_notes`), idle, sample (filtered to that user-data-dir)→classify→report under `out_dir/<profile_id>/`; then `compare.json` / `compare.md` summarizing `process_count`, `total_ws_bytes`, and role aggregates.
- **Launch** — if `urls` or `args` non-empty, best-effort start Chrome/Edge (common Windows paths; Linux `google-chrome` / `chromium`). Launch failure logs a warning and **still samples** existing processes (bench does not fail).
- **CLI** — `bpb bench --scenario … --browser … --out-dir …`; optional `--profile <id>` to run one profile when several are defined.

Security stance in markers/docs: `--process-per-site` and renderer limits trade fate-sharing / contention **within** Site Isolation; they are measurement knobs, not a license to disable isolation.

## 7. Next components (stubs only)

See [roadmap.md](roadmap.md). Near-term stubs (not implemented here): multi-session budget rollups, PSS-like private attribution. Richer office scenarios: `office-dense-ab` (multi-origin tabs) and `office-samesite-ab` (same eTLD+1 tabs so `--process-per-site` can consolidate renderers) shipped; further SaaS sets may follow.

## Related

- [Security: what we measure vs never recommend](security.md)
- [Roadmap](roadmap.md)
