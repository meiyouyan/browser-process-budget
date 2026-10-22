# Security Policy / 安全策略

## English

`browser-process-budget` is a **read-only measurement** tool by default.
It must not change browser or OS security policies unless the operator
explicitly opts into a documented, risk-labeled workflow.

- Do **not** disable Site Isolation on shared multi-user hosts (RDS) without a written risk acceptance.
- Reports intended for public release must redact hostnames, usernames, and internal URLs.
- Never use this project to scan or modify systems you do not administer.

## 中文

本工具默认只做**只读度量**，不应擅自改浏览器或系统安全策略。

- 在 RDS 等多用户共享主机上，未经书面风险确认，不要关闭站点隔离。
- 公开报告必须脱敏：主机名、用户名、内网 URL。
- 不得用于你无权管理的系统。
