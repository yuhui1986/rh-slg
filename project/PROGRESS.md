# 《天下策》项目进度看板

> 本文件由 `master-coordinator` 维护，请勿手动修改状态字段。
> 架构基线：[../ARCHITECTURE.md](../ARCHITECTURE.md)（当前 v1.4）

## 里程碑总览

| 里程碑 | 状态 | 说明 |
|--------|------|------|
| M0 立项 | ✅ 完成 | 架构基线 ✅ / agent 团队 ✅ / workspace 骨架 ✅ / CI ✅ |
| M1 V1 核心循环 | ✅ 完成 | 铺路占地 + 资源 + 战斗 + AI + 渲染 + 编辑器（170 个测试全通过） |
| M2 编辑器完整 | ✅ 完成 | 事件链/胜利条件/区域规则 + 高级编辑器 + 沙盒模式 + 地图画廊（313 个测试全通过） |
| M3 V1 完整可玩 | ✅ 完成 | "三国鼎立"剧本通关 + 编辑器创建剧本 + Steam 构建（392+ 个测试全通过） |
| M4 V3 Mod 生态 | 未开始 | rhai 脚本 / Steam 创意工坊 / Mod 工具链 |

## V1 任务组（对应 ARCHITECTURE §13）

- [x] T1 工程骨架：workspace 8 crate / CI / tracing 日志 / fluent i18n 框架（10%）✅ M0 已完成
- [x] T2 大地图：Chunk 渲染 / LOD / 相机 / hex 拾取 / 迷雾（15%）✅ M1 已完成
- [x] T3 核心玩法：铺路占地 + 资源循环 + 征兵建造（25%）✅ M1 已完成
- [x] T4 战斗：武将/战法/兵种 + 纯函数结算 + 战报 UI（20%）✅ M1 已完成
- [x] T5 AI：5 势力效用 AI + 基础外交（15%）✅ M1 已完成
- [x] T6 剧本：三国鼎立（5 AI 势力 + 玩家独立势力）+ 完整事件链 + 胜利条件 + 外交台词（10%）✅ M1 已完成
- [x] T7 编辑器雏形：地形笔刷 / 实体放置 / 撤销重做 / 基础校验（5%）✅ M1 已完成

## 当前迭代：M1 V1 核心循环 ✅ 已完成

> 执行计划：[M1_EXECUTION_PLAN.md](./M1_EXECUTION_PLAN.md)
> 状态：✅ 全部完成（2026-08-02）

### M1 验收结果

| 检查项 | 结果 |
|--------|------|
| `cargo fmt --check` | ✅ PASS |
| `cargo clippy -- -D warnings` | ✅ PASS |
| `cargo test --workspace` | ✅ 170 个测试全通过 |
| `cargo check --workspace` | ✅ PASS |
| `grep bevy slg-core/slg-data` | ✅ 为空（零引擎依赖） |
| slg-core 测试 | ✅ 140 个（hex/领地/经济/行军/战斗/AI/生成/加载） |
| slg-app 集成测试 | ✅ 10 个（全流程/确定性/势力/领地/时钟） |
| slg-save 测试 | ✅ 8 个（往返/CRC/压缩） |
| slg-editor 测试 | ✅ 8 个（命令栈/工具/校验） |
| slg-assets 测试 | ✅ 4 个（加载/校验/剧本） |

### M1 任务卡完成情况

| 卡号 | 标题 | 负责 | 状态 |
|------|------|------|------|
| M1-T01 | ECS 组件与 Resource 定义 | core | ✅ |
| M1-T02 | 数据表扩展与三国内容 | content | ✅ |
| M1-T03 | slg-assets 数据加载 | core | ✅ |
| M1-T04 | 程序化地图生成管线 | core | ✅ |
| M1-T05 | MapDoc↔World 转换 | core | ✅ |
| M1-T06 | 存档容器读写 | editor | ✅ |
| M1-T07 | 时钟与 tick_dispatcher | render | ✅ |
| M1-T08 | 领地与铺路系统 | core | ✅ |
| M1-T09 | 经济与资源系统 | core | ✅ |
| M1-T10 | 行军与寻路系统 | core | ✅ |
| M1-T11 | 战斗模拟系统 | core | ✅ |
| M1-T12 | Chunk 渲染与 LOD | render | ✅ |
| M1-T13 | 相机与 hex 拾取 | render | ✅ |
| M1-T14 | 迷雾渲染系统 | render | ✅ |
| M1-T15 | HUD 面板 | render | ✅ |
| M1-T16 | AI 决策系统 | core | ✅ |
| M1-T17 | 编辑器基础工具 | editor | ✅ |
| M1-T18 | 全链路集成 | render+core | ✅ |
| M1-T19 | 集成测试与验收 | qa | ✅ |

**M1 完成，进入 M2。**

### M1 任务卡总表

| 卡号 | 标题 | 负责 | 依赖 | 状态 |
|------|------|------|------|------|
| M1-T01 | ECS 组件与 Resource 定义 | core-engineer | 无 | 待启动 |
| M1-T02 | 数据表扩展与三国内容 | content-designer | 无 | 待启动 |
| M1-T03 | slg-assets 加载实现 | core-engineer | 无 | 待启动 |
| M1-T04 | 程序化地图生成管线 | core-engineer | T01 | 待启动 |
| M1-T05 | MapDoc<->World 转换 | core-engineer | T01, T04 | 待启动 |
| M1-T06 | 存档容器读写 | editor-engineer | 无 | 待启动 |
| M1-T07 | 时钟与 tick_dispatcher | core + render | T01 | 待启动 |
| M1-T08 | 领地与铺路系统 | core-engineer | T01 | 待启动 |
| M1-T09 | 经济与资源系统 | core-engineer | T01 | 待启动 |
| M1-T10 | 行军与寻路系统 | core-engineer | T01, T09 | 待启动 |
| M1-T11 | 战斗模拟系统 | core-engineer | T01 | 待启动 |
| M1-T12 | Chunk 渲染与 LOD | render-engineer | T01, T04 | 待启动 |
| M1-T13 | 相机与 hex 拾取 | render-engineer | T12 | 待启动 |
| M1-T14 | 迷雾渲染 | render-engineer | T01, T12 | 待启动 |
| M1-T15 | HUD 面板 | render-engineer | T01 | 待启动 |
| M1-T16 | AI 决策系统 | core-engineer | T08, T09, T10, T11 | 待启动 |
| M1-T17 | 编辑器基础工具 | editor-engineer | T01, T12 | 待启动 |
| M1-T18 | 全链路集成 | render + core | T01~T17 | 待启动 |
| M1-T19 | 集成测试与验收 | qa-engineer | T18 | 待启动 |

### 上一迭代：M0 workspace 骨架与 CI ✅ 已完成

> 执行计划：[M0_EXECUTION_PLAN.md](./M0_EXECUTION_PLAN.md)
> 状态：✅ 全部完成（2026-08-02）

| 卡号 | 标题 | 负责 | 依赖 | 状态 |
|------|------|------|------|------|
| M0-T1 | workspace 骨架创建 | qa-engineer | 无 | ✅ 完成 |
| M0-T2 | CI 流水线配置 | qa-engineer | T1 | ✅ 完成 |
| M0-T3 | ECS 数据映射表与转换接口 | arch-guardian | 无 | ✅ 完成 |
| M0-T4 | slg-data 共享数据结构骨架 | core-engineer | T3 | ✅ 完成 |
| M0-T5 | slg-core hex 网格数学模块 | core-engineer | T3 | ✅ 完成 |
| M0-T6 | 内容数据骨架 | content-designer | T4 | ✅ 完成 |
| M0-T7 | 渲染/编辑器 crate 骨架 | render + editor | T1 | ✅ 完成 |
| M0-T8 | slg-app 入口 + slg-assets | render + core | T7 | ✅ 完成 |
| M0-T9 | 集成验证与红线巡检 | qa + arch | T1-T8 | ✅ 完成 |

### M0 验收结果

| 检查项 | 结果 |
|--------|------|
| `cargo fmt --check` | ✅ PASS |
| `cargo clippy -- -D warnings` | ✅ PASS |
| `cargo test --workspace` | ✅ 13 个测试全部通过 |
| `cargo build --release` | ✅ PASS |
| `grep bevy slg-core/slg-data` | ✅ 为空（零引擎依赖） |
| 非测试代码无 unwrap | ✅ PASS |
| ARCHITECTURE.md v1.4 | ✅ ECS 映射表 + 转换接口已补充 |
| CI yml | ✅ Windows 矩阵，4 个 job |
| assets/data/*.ron | ✅ 7 个 RON 文件 |
| assets/i18n/zh-CN/main.ftl | ✅ 14 个 key |

## 阻塞项

（无）

## 决策日志

| 日期 | 决策 | 来源 |
|------|------|------|
| 2026-07-29 | 架构基线 v1.0 建立（Bevy+egui、Chunk 地图、纯函数战斗、效用 AI、编辑器共基座） | 架构评审 |
| 2026-07-29 | 游戏定名《天下策》 | 用户 |
| 2026-07-29 | Q1–Q5 确认：三国题材 / 六边形网格 / Steam 发行 / Win10 / 本地存档 → 基线 v1.1 | 用户 |
| 2026-07-29 | Q8 确认：首发剧本"三国鼎立"，玩家独立势力 → 基线 v1.2 | 用户 |
| 2026-07-29 | AI agent 开发团队组建（1 总排 + 6 专项） | 用户 |
| 2026-08-02 | 架构评估确认：C1–C10 共 10 项决策（详见下方） | 用户 |
| 2026-08-02 | C1: V1 默认 256×256，512² 可选（D4 更新） | 用户 |
| 2026-08-02 | C2: JPS 保留描述，V1 不实现（D10 更新） | 用户 |
| 2026-08-02 | C3: AI 错峰改为每局随机分配决策槽位（§2.3 更新） | 用户 |
| 2026-08-02 | C4: 存档 delta 超 30% 时做全量快照合并（§10 更新） | 用户 |
| 2026-08-02 | C5: 256² 生成 <5s 目标维持，先 benchmark 验证 | 用户 |
| 2026-08-02 | C6: T6 剧本占比调至 10%，首发做完整剧本体验 | 用户 |
| 2026-08-02 | C7: Agent 可直接通信，须抄送 master-coordinator | 用户 |
| 2026-08-02 | C8: ACR 流程先不定义，遇到问题再建立 | 用户 |
| 2026-08-02 | C9: qa-engineer 承担代码审查职责 | 用户 |
| 2026-08-02 | C10: 补充 content-designer 设计方法论 | 用户 |
