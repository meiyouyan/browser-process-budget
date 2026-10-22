# browser-process-budget (`bpb`)

Measure and attribute Chromium-family browser process overhead on constrained Windows hosts — especially **RDS / Remote Desktop** sessions.

中文说明见 [README.zh-CN.md](README.zh-CN.md)。

## Why

Site Isolation improves security but multiplies renderer processes. On RDS, thin clients, and low-end office PCs, process count and working set often dominate latency. This project turns that cost into **reproducible numbers** and policy comparisons — it is not a browser fork, and it does not recommend turning off isolation by default.

## Status

**v0.1 — usable pipeline on Linux for classify/report; Windows sampler compiles for RDS hosts.**

| Command | Linux (this box / CI) | Windows RDS (primary) |
| --- | --- | --- |
| `bpb classify` | Works | Works |
| `bpb report` | Works (JSON + Markdown, optional redact) | Works |
| `bpb sample` | **Smoke only** via `/proc` (google-chrome / chromium) | Native Toolhelp + working set + session id |
| `bpb bench` | Loads `scripts/*.toml`, idle sleep, then sample→classify→report (URL launch skipped) | Same |

Windows APIs are behind `cfg(windows)` and are not exercised on Linux CI. Run `bpb sample` on the RDS host for production numbers.

## CLI

```bash
bpb sample --browser chrome --out sample.json
bpb classify --input sample.json --out classified.json
bpb report --input classified.json --out-dir ./out --redact
bpb bench --scenario office-basic --browser chrome --out-dir ./out
```

### Fixture smoke (no live browser required)

```bash
cargo build --release
./target/release/bpb classify --input examples/sample-fixture.json --out /tmp/bpb-classified.json
./target/release/bpb report --input /tmp/bpb-classified.json --out-dir /tmp/bpb-out --redact
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
src/           Rust CLI (sample / classify / report / bench / model)
docs/en|zh/    Architecture & guides (bilingual)
scripts/       Bench scenarios
examples/      Sample fixtures (redacted-friendly)
```

## Target environment

- Primary: Windows RDS + Google Chrome
- Secondary: Microsoft Edge
- Linux sample path: smoke / pipeline testing only
- Docs: Chinese + English

## License

MIT
