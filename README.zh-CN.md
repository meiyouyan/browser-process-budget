# browser-process-budget (`bpb`)

在受限 Windows 主机（尤其是 **RDS / 远程桌面**）上，度量并归因 Chromium 系浏览器的进程与内存开销。

English: [README.md](README.md).

## 为什么做

站点隔离提升安全，但会放大渲染进程数量。在 RDS、瘦客户机、低性能办公终端上，进程数和工作集往往先拖垮体验。本项目把开销变成**可复现数据**和策略对照——不是另做浏览器，也默认不建议关闭隔离。

## 状态

仓库骨架 / 设计中。下一步实现 Windows 采样器。

## CLI（规划）

```bash
bpb sample --browser chrome --out sample.json
bpb classify --input sample.json --out classified.json
bpb report --input classified.json --out-dir ./out --redact
bpb bench --scenario office-basic --browser chrome --out-dir ./out
```

## 目录

```
src/           Rust CLI
docs/zh|en/    架构与指南（中英）
scripts/       基准场景
examples/      脱敏示例报告
```

## 目标环境

- 主：Windows RDS + Chrome
- 次：Edge
- 文档：中英双语

## 许可证

MIT
