# Security: what we measure vs what we never recommend

bpb is a **measurement lab**, not a policy hammer. This page states the boundary in plain language. Top-level operator policy also lives in [`SECURITY.md`](../../SECURITY.md).

## What we measure

Under **Site Isolation kept on**, we measure and compare:

| Knob / signal | Why it matters on RDS |
| --- | --- |
| Process count & role mix | Renderer explosion vs. browser/GPU/utility baselines |
| Working set (and later private/PSS-like) | Memory pressure across sessions |
| Launch profiles (`[[profiles]]` args) | Process-reuse density (`--process-per-site`), soft renderer caps (`--renderer-process-limit`), future Memory Saver / extension sets |
| Session id (Windows) | Per-user trees on shared hosts (aggregation planned) |

Profile markers record `args` and `security_notes` so a run is auditable: what was launched, and that isolation was not silently dropped.

## Acceptable measurement trade-offs (not “defaults to ship blindly”)

These may appear in **explicit** A/B profiles for measurement:

- `--process-per-site` — consolidates same-site renderers; increases **fate-sharing within a site**; does **not** disable Site Isolation.
- `--renderer-process-limit=N` — soft cap; can raise tab contention / fate-sharing under load.
- Future: Memory Saver / discard-related and managed-extension profiles — always documented with trade-offs.

Operators must still **measure on their RDS image** before adopting any density knob.

## What we never recommend as defaults

Do **not** treat the following as recommended defaults for shared multi-user hosts (RDS):

| Flag / practice | Why |
| --- | --- |
| `--disable-site-isolation-trials` / disabling Site Isolation | Cross-site compromise blast radius |
| `--no-sandbox` | Removes a core process sandbox |
| `--single-process` | Collapses isolation boundaries |
| Silent policy changes by the tool | bpb must not weaken security without an explicit, risk-labeled operator choice |

If a scenario somehow includes such flags for a controlled lab experiment, markers emit a **WARNING** and docs still refuse them as defaults.

## Data handling

- Reports meant for public release should **redact** hostnames, usernames, and internal URLs (`bpb report --redact`).
- Use only on systems you administer.
- Read-only sampling is the default posture.

## Related

- [Architecture](architecture.md)
- [Roadmap](roadmap.md)
