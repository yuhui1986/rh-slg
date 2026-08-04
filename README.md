# 天下策（rh-slg）

《天下策》——类《率土之滨》的**桌面单机 SLG**，Rust 实现。核心体验是格子大地图上的铺路占地、配将战法与 AI 势力博弈；大地图视图同时是**地图编辑器**，以"游玩 → 创作 → 分享"支撑长期可玩性。

## 技术栈速览

- **语言**：Rust（edition 2021+，workspace 多 crate）
- **渲染**：Bevy（2D）+ **UI**：bevy_egui（面板/编辑器）
- **逻辑核心**：`slg-core` 纯 Rust 库，零引擎依赖，可单测/可换渲染层
- **数据**：RON 数据表驱动（武将/战法/兵种/地形/事件），Mod 预留
- **存档**：自定义二进制容器（bincode + zstd），地图引用 + 增量存档

## 文档

| 文档 | 内容 |
|------|------|
| [ARCHITECTURE.md](./ARCHITECTURE.md) | **项目技术架构基线 v1.0**：玩法分析、单机减法决策、20 项技术决策清单、模块划分、子系统设计、性能预算、V1→V3 路线图、风险登记册 |

> 任何架构级变更须先更新 ARCHITECTURE.md 并升版本号。

## 仓库规划

```
crates/         slg-data / slg-core / slg-engine / slg-ui / slg-editor / slg-save / slg-assets / slg-app
assets/         textures / audio / fonts / data(RON 数据表) / i18n / maps
mods/           Mod 目录（玩家安装）
user/           玩家自制地图与存档
tests/          跨 crate 集成测试
project/        进度看板 PROGRESS.md（由 master-coordinator 维护）
.claude/agents/ AI 开发团队定义（本项目全 AI 开发）
```

## AI 开发团队

本项目完全由 AI agent 开发，团队定义在 `.claude/agents/`：

| Agent | 角色 |
|-------|------|
| `master-coordinator` | **总排协调员**：里程碑管理、任务分解、派发、验收门禁（唯一调度入口） |
| `arch-guardian` | 架构守护者：基线评审、红线巡检、争议裁决 |
| `core-engineer` | 核心逻辑：hex 网格 / 战斗 / 经济 / AI / 程序化生成（slg-core · slg-data） |
| `render-engineer` | 引擎渲染：Chunk 渲染 / 相机 / 迷雾 / HUD（slg-engine · slg-ui） |
| `editor-engineer` | 地图编辑器：工具 / 命令栈 / 容器格式（slg-editor · slg-save） |
| `content-designer` | 数据与内容：三国数据表 / 三国鼎立剧本 / 数值 / i18n |
| `qa-engineer` | 质量保障：测试 / CI / 基准 / 性能门禁 |

使用方式：交给 `master-coordinator` 统筹派发，或直接指名任一专项 agent 执行。

当前状态：**架构基线 v1.2 已建立、agent 团队就位**。下一步：master-coordinator 启动 M0（workspace 骨架与 CI）。
