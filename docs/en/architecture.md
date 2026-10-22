# Architecture

## Pipeline

1. **sample** — enumerate Chrome (then Edge) processes; capture PID, parent, session id, working set, command line.
2. **classify** — map into Browser / Renderer / GPU / Utility / Extension / Other.
3. **report** — JSON + Markdown with optional redaction.
4. **bench** — run a named office scenario on RDS, then sample.

## Principles

- Read-only by default.
- RDS multi-session comparison is a first-class goal.
- Never silently weaken Site Isolation.
