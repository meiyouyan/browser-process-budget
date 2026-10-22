# 架构说明

## 流水线

1. **sample** — 枚举 Chrome（随后 Edge）进程；采集 PID、父进程、会话 ID、工作集、命令行。
2. **classify** — 归类为 Browser / Renderer / GPU / Utility / Extension / Other。
3. **report** — 输出 JSON + Markdown，支持脱敏。
4. **bench** — 在 RDS 跑命名办公场景后再采样。

## 原则

- 默认只读。
- RDS 多会话对比是一等目标。
- 不静默削弱站点隔离。
