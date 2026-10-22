# browser-process-budget (`bpb`)

Measure and attribute Chromium-family browser process overhead on constrained Windows hosts — especially **RDS / Remote Desktop** sessions.

中文说明见 [README.zh-CN.md](README.zh-CN.md)。

## Why

Site Isolation improves security but multiplies renderer processes. On RDS, thin clients, and low-end office PCs, process count and working set often dominate latency. This project turns that cost into **reproducible numbers** and policy comparisons — it is not a browser fork, and it does not recommend turning off isolation by default.

## Status

Scaffold / v0.1 design. Sampler implementation is next.

## CLI (planned)

```bash
bpb sample --browser chrome --out sample.json
bpb classify --input sample.json --out classified.json
bpb report --input classified.json --out-dir ./out --redact
bpb bench --scenario office-basic --browser chrome --out-dir ./out
```

## Layout

```
src/           Rust CLI (sample / classify / report / bench)
docs/en|zh/    Architecture & guides (bilingual)
scripts/       Bench scenarios
examples/      Sample reports (redacted)
```

## Target environment

- Primary: Windows RDS + Google Chrome
- Secondary: Microsoft Edge
- Docs: Chinese + English

## License

MIT
