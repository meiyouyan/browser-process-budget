# Roadmap (3–6 months)

Research direction remains: **keep Site Isolation**; make bpb the RDS **process-budget lab** that A/B’s process-reuse / Memory Saver / extension / session-density policies with reproducible numbers. Do **not** ship defaults that disable Site Isolation, sandbox, or force single-process. Blink/Servo Rust rewrite and Chromium forks stay **out of scope**.

Milestones are planning stubs. Dates are approximate; no fabricated benchmark results.

## Now (shipped)

- [x] Pipeline: sample → classify → report
- [x] Bench scenarios (`scripts/*.toml`)
- [x] **Process-model profile A/B** in bench (`[[profiles]]`, per-profile out dirs, `compare.json` / `compare.md`)
- [x] Framework docs (architecture / security / this roadmap), bilingual

## Month 1–2

- [ ] Harden Windows RDS sampling (session id coverage, edge cases)
- [x] Expand office scenarios (tab density, common SaaS pages) — `office-dense-ab` (+ optional `office-dense-ab-quick`); same-site multi-tab for process-per-site — `office-samesite-ab` (+ `office-samesite-ab-quick`); still measurement-only
- [x] Optional Memory Saver / discard-related flags as **named profiles** (`office-memory-ab` / `office-memory-ab-quick`; optional Preferences seeding; discard reloads inactive tabs — can understate steady-state memory; never disable isolation)
- [ ] CI: fixture classify/report + `office-basic-ab` smoke (0 Chrome procs OK on Linux)

## Month 3–4

- [ ] **Multi-session budget** (stub): aggregate samples by Windows session id; per-user and host totals
- [ ] Extension-policy profiles (empty vs. managed set) under Site Isolation
- [ ] Compare UX: richer role deltas in `compare.md`; optional CSV export
- [ ] Operator runbook for RDS hosts (launch → idle → sample → compare)

## Month 5–6

- [ ] **Private-WS / PSS-like attribution** (stub): better shared-vs-private estimates where the OS allows
- [ ] Session-density scenarios (N synthetic sessions on a lab host — lab only)
- [ ] Documented “risk-labeled” profile catalog (explicit warnings; no silent weakening)
- [ ] Freeze v0.2 schema notes if compare/session fields stabilize

## Explicitly deferred

- Disabling Site Isolation / `--no-sandbox` / `--single-process` as recommended defaults
- Blink or Servo Rust rewrite
- Maintaining a Chromium fork
- Inventing marketing numbers without host measurements

See also [architecture.md](architecture.md) and [security.md](security.md).
