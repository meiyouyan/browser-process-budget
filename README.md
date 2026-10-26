# browser-process-budget (`bpb`)

Measure and attribute Chromium-family browser process overhead on constrained Windows hosts — especially **RDS / Remote Desktop** sessions.

中文说明见 [README.zh-CN.md](README.zh-CN.md)。

## Why

Site Isolation improves security but multiplies renderer processes. On RDS, thin clients, and low-end office PCs, process count and working set often dominate latency. This project is the **RDS process-budget lab**: keep Site Isolation, A/B process-reuse / density policies, and turn the cost into **reproducible numbers**. It is not a browser fork, and it does **not** recommend turning off isolation, sandbox, or single-process as defaults.

Framework (problem, pipeline, modules, principles, component map):

- English: [docs/en/architecture.md](docs/en/architecture.md) · [security](docs/en/security.md) · [roadmap](docs/en/roadmap.md)
- 中文: [docs/zh/architecture.md](docs/zh/architecture.md) · [安全](docs/zh/security.md) · [路线图](docs/zh/roadmap.md)

## Status

**v0.1 — classify/report on Linux; Windows sampler for RDS; bench with process-model profile A/B.**

| Command | Linux (this box / CI) | Windows RDS (primary) |
| --- | --- | --- |
| `bpb classify` | Works | Works |
| `bpb report` | Works (JSON + Markdown, optional redact) | Works |
| `bpb sample` | **Smoke only** via `/proc` (google-chrome / chromium) | Native Toolhelp + working set + session id |
| `bpb bench` | Scenario TOML → idle → sample→classify→report; optional `[[profiles]]` A/B + `compare.*`; best-effort browser launch | Same (launch via common Chrome/Edge paths) |

Windows APIs are behind `cfg(windows)` and are not exercised on Linux CI. Run `bpb sample` / `bpb bench` on the RDS host for production numbers. Docs never invent fake benchmark figures.

**Key component now:** profile A/B in bench — see [architecture §6](docs/en/architecture.md#6-profile-ab-design-bench).

## CLI

```bash
bpb sample --browser chrome --out sample.json
bpb classify --input sample.json --out classified.json
bpb report --input classified.json --out-dir ./out --redact
bpb bench --scenario office-basic --browser chrome --out-dir ./out
```

### Process-model A/B (profiles)

```bash
# Three profiles → out/baseline, out/process-per-site, out/renderer-limit-4 + out/compare.json|md
cargo build
./target/debug/bpb bench --scenario office-basic-ab --browser chrome --out-dir ./out-ab

# Run a single profile from a multi-profile scenario
./target/debug/bpb bench --scenario office-basic-ab --browser chrome --out-dir ./out-ab --profile baseline
```

`scripts/office-basic.toml` stays profile-free (single run at `out_dir` root). `scripts/office-basic-ab.toml` declares baseline / `--process-per-site` / `--renderer-process-limit=4` with a short idle for CI. For RDS process-model divergence under Site Isolation, use **`office-dense-ab`** (6–8 multi-origin URLs, `idle_seconds = 20`); optional **`office-dense-ab-quick`** (4 URLs, idle 3s) for a faster smoke on the test host. Multi-profile runs use a dedicated `--user-data-dir` per profile, stop matching browser processes between profiles, and filter the sample to that directory so counts do not stack. None of the default profiles disable Site Isolation.

```bash
# Denser multi-origin A/B (RDS-oriented idle)
./target/debug/bpb bench --scenario office-dense-ab --browser chrome --out-dir ./out-dense-ab

# Faster smoke twin
./target/debug/bpb bench --scenario office-dense-ab-quick --browser chrome --out-dir ./out-dense-ab-quick
```

### Fixture smoke (no live browser required)

```bash
cargo build --release
./target/release/bpb classify --input examples/sample-fixture.json --out /tmp/bpb-classified.json
./target/release/bpb report --input /tmp/bpb-classified.json --out-dir /tmp/bpb-out --redact
# or: ./scripts/smoke-fixture.sh
```

### Live Linux smoke (optional)

```bash
# if Chrome is running:
bpb sample --browser chrome --out /tmp/sample.json
bpb classify --input /tmp/sample.json --out /tmp/classified.json
bpb report --input /tmp/classified.json --out-dir /tmp/bpb-live --redact
```

## Layout

```
src/           Rust CLI (sample / classify / report / bench+profiles / model)
docs/en|zh/    Architecture, security, roadmap (bilingual)
scripts/       Bench scenarios (office-basic, office-basic-ab, office-dense-ab, …)
examples/      Sample fixtures (redacted-friendly)
```

## Target environment

- Primary: Windows RDS + Google Chrome
- Secondary: Microsoft Edge
- Linux sample path: smoke / pipeline testing only
- Docs: Chinese + English

## License

MIT
