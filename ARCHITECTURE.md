# 《天下策》项目技术架构（基线 v1.0）

> 游戏名称：**天下策**（仓库代号 `rh-slg`，crate 前缀 `slg-`）
> 状态：**已评审基线 v1.4**（2026-08-02）
> 范围：三国题材、类《率土之滨》桌面单机 SLG，大地图 UI 与地图编辑器共基座，数据驱动 + Mod 预留
>
> **变更记录**
> - v1.4（2026-08-02）：M0-T3 架构补充——§6.8 新增 ECS 数据映射表（每种游戏对象的存储方式）；§6.1 补充地图三形态转换接口伪代码签名；§7.1 补充编辑器同步策略（MapDocument / Runtime World / Save 三向转换流程）
> - v1.3（2026-08-02）：架构评估确认 10 项决策——D4 地图默认 256²/512² 可选；D10 JPS V1 不实现；§2.3 AI 错峰改为每局随机分配；§10 delta 超 30% 合并；T6 剧本占比调至 10%；跨 agent 协作协议；qa-engineer 增加代码审查职责；content-designer 增加设计方法论
> - v1.2（2026-07-29）：首发剧本确认为**"三国鼎立"**（Q8）：魏/蜀/吴 + 公孙渊·辽东、孟获·南中 共 5 个 AI 势力；**玩家为独立势力**（自建君主、一州起步、不附属于任何阵营）
> - v1.1（2026-07-29）：产品决策确认——三国题材（Q1）；网格改为**六边形**（Q2，D3/D10 联动变更）；**Steam 发行**（Q3，D18 变更）；主平台 Win10（Q4）；V1 存档纯本地（Q5）
> - v1.0（2026-07-29）：初始基线
> 本文档是后续长期维护与研发方向的唯一架构基准。任何架构级变更须更新本文档并提升版本号。

---

## 目录

1. [项目定位与玩法分析](#1-项目定位与玩法分析)
2. [单机化减法决策](#2-单机化减法决策)
3. [总体技术架构](#3-总体技术架构)
4. [核心技术决策清单](#4-核心技术决策清单)
5. [Workspace 与模块划分](#5-workspace-与模块划分)
6. [核心子系统设计](#6-核心子系统设计)
7. [地图编辑器设计](#7-地图编辑器设计)
8. [程序化生成](#8-程序化生成)
9. [数据驱动与 Mod 预留](#9-数据驱动与-mod-预留)
10. [存档系统](#10-存档系统)
11. [工程化与质量保障](#11-工程化与质量保障)
12. [性能预算](#12-性能预算)
13. [研发路线图（V1 → V2 → V3）](#13-研发路线图v1--v2--v3)
14. [风险登记册](#14-风险登记册)
15. [待决问题](#15-待决问题)

---

## 1. 项目定位与玩法分析

### 1.1 一句话定位

**《天下策》——一个"大地图即核心、编辑器即内容"的单机率土 like SLG**：玩家在格子大地图上铺路占地、配将战法、与有人格的 AI 势力博弈；玩完之后，用同一张地图视图做编辑器自建剧本，形成"游玩 → 创作 → 分享"的长期内容循环。"天下策"取"逐鹿天下、谋定而后动"之意，呼应游戏的战略博弈内核。

### 1.2 《率土之滨》玩法拆解（结论摘要）

**核心循环**：`铺路占地 → 资源产出 → 征兵养成 → 行军作战 → 夺取更多土地`，正反馈滚雪球 + 空间零和博弈。

| 机制 | 说明 | 本项目态度 |
|------|------|-----------|
| 铺路占地（相邻占领） | 只能占与己方领地相邻的格子，领土形状即防线 | **灵魂机制，保留** |
| 战法搭配 | 主将+2副将，主动/被动/指挥/突击四类战法，拆将得战法点 | **灵魂机制，保留** |
| 兵种克制 | 骑 > 弓 > 步 > 骑（原文为骑→步→弓→骑三角），约 ±15% 伤害 | 保留 |
| 真实时间行军 | 行军不可操作、可被截击，"援军能否赶到"是战略核心 | 保留为**可暂停实时制** |
| 回合制自动战斗 | 最多 8 回合，速度定序，战法概率发动，配将大于操作 | 保留，做成纯函数模拟 |
| 土地等级 1~9 | 越高级守军越强产出越高，中心富饶边缘贫瘠 | 保留 |
| 迷雾/视野 | 领地与部队提供视野，信息不对称 | 保留 |
| 同盟国战/赛季制 | 真人社交高潮 + 周期性重置 | **替换**：AI 势力外交 + 剧本/沙盒模式 |
| 抽卡付费 | 商业化核心 | **砍掉**：在野招募/俘获/登用/事件获取 |
| 服务器权威/反作弊 | 网游信任基础设施 | **砍掉**：本地即权威 |

### 1.3 率土 like 的"灵魂"清单（不可裁剪项）

1. 铺路占地 —— 空间即战略
2. 战法搭配 —— 配将深度
3. 大地图宏观博弈 —— 资源与要地争夺
4. 兵种/地形克制 —— 战斗基本策略维度
5. 资源产出/消耗循环 —— 扩张的经济引擎

裁剪任何一条，游戏就不再是率土 like。

---

## 2. 单机化减法决策

> 原则：**砍掉一切"为网游存在"的机制，保留一切"为乐趣存在"的机制。**

### 2.1 裁剪总表

| 机制 | 网游中的角色 | 单机决策 | 替代方案 |
|------|-------------|---------|---------|
| 网络层/协议同步 | 多人通信 | 砍掉 | 本地单机运行 |
| 服务器权威 | 数据一致/防作弊 | 砍掉 | 本地计算即权威 |
| 反作弊 | 防外挂 | 砍掉 | 单机修改器 = 玩家自由（甚至提供调试面板） |
| 账号/登录 | 身份认证 | 砍掉 | 本地存档 |
| 付费抽卡 | 商业化 | 砍掉 | 多途径获取武将（招募/俘获/登用/事件） |
| 真人同盟博弈 | 社交核心 | **替换** | 4~8 个有人格的 AI 势力 + 外交系统 |
| 赛季制 | 周期重置 | **改造** | 剧本模式（结构化）+ 沙盒模式（无限重玩） |
| 邮件/聊天 | 社交沟通 | 简化 | 事件通知 + AI 外交对话 |
| 集市交易 | 经济流通 | 保留简化 | 本地交易面板 |

### 2.2 时间制决策：可暂停实时制

**决策**：Tick 驱动的**可暂停实时制**，速度档 ×1 / ×2 / ×3（实现上限 ×10）。

| 方案 | 结论 |
|------|------|
| 纯回合制 | ✗ 失去"行军时间"这一战略维度，率土味道大减 |
| 纯实时制 | ✗ 无法暂停思考，新手不友好 |
| **可暂停实时** | ✅ 兼顾两者 |

规则要点：
- 逻辑时钟与渲染帧完全分离（固定 100 ms/tick，10 tick/s @ ×1）
- 暂停时：渲染与 UI 照常运行，可查看地图/战报/调配视图，指令进入 `CommandQueue`，恢复后注入
- 高倍速下战斗跳过演出直接出结果

### 2.3 AI 势力替代真人同盟

- 每局 4~8 个 AI 势力，每个势力有**名字、性格模板（好战/保守/商业/外交/阴谋）、战略目标、关系网络**
- AI 决策错峰：每局开始时**随机分配势力到决策槽位**（slot 0~9），每 tick 只有当 `current_tick % 10 == slot` 的势力执行决策，避免编号小的势力总是先手
- AI 与玩家受**完全相同规则**约束（同资源公式、同行军速度、同迷雾限制），难度差异只来自"信息延迟/决策间隔"等显式参数，不做隐形作弊
- 简化外交：好感度 -100~100，动作集 = 结盟/宣战/停战/送礼/威胁/贸易；盟约共享视野、互不攻击
- 关键体验目标：玩家能感知到"曹操在扩张、孙权在观望、刘备在求援"——**AI 有人格是单机弥补社交的唯一可行解**

### 2.4 模式设计

| 模式 | 定位 | 内容 |
|------|------|------|
| 剧本模式 | 结构化体验（V1 主线） | 固定开局、预设事件链、明确胜利条件（占洛阳/统一/存活 N 天），单局 4~12h |
| 沙盒模式 | 无限重玩（V2 起） | 程序化生成或玩家自建地图，动态事件，追求分数/版图 |

---

## 3. 总体技术架构

### 3.1 分层架构图

```
┌────────────────────────────────────────────────────────────────────┐
│                        slg-app（应用壳）                            │
│          窗口/入口 · 模式切换(游玩⇄编辑) · 插件组装 · 崩溃日志      │
├──────────────────────┬─────────────────────────────────────────────┤
│   slg-ui (egui)      │           slg-editor                        │
│   HUD/面板/弹窗      │   工具/笔刷/命令栈/校验/图层                │
│   小地图/战报        │   （编辑器 = 游戏视图的超集，复用渲染）      │
├──────────────────────┴─────────────────────────────────────────────┤
│                    slg-engine（Bevy 渲染与输入层）                  │
│   Chunk 网格渲染 · LOD · 相机平移缩放 · 格子拾取 · 迷雾渲染         │
│   tick_dispatcher（逻辑时钟）· ECS↔存档快照同步                     │
├────────────────────────────────────────────────────────────────────┤
│                    slg-core（纯逻辑，零渲染依赖）★                  │
│   map/（网格·寻路·领地UnionFind）  rule/（战斗·移动·经济）          │
│   entity/（武将·部队·城池·势力）   ai/（效用AI·外交）               │
│   gen/（程序化生成管线）           event/（游戏事件）               │
├──────────────────────┬──────────────────────┬──────────────────────┤
│   slg-data           │   slg-save           │   slg-assets         │
│   共享数据结构        │   容器读写·版本迁移   │   RON 数据表加载      │
│   配置/存档/地图定义  │   .slgmap/.slgsave   │   Mod 合并·热重载     │
└──────────────────────┴──────────────────────┴──────────────────────┘
            依赖方向：严格自上而下，下层绝不依赖上层
            slg-core 不依赖 Bevy —— 可单测、可换渲染层
```

### 3.2 核心架构原则

1. **核心逻辑与渲染解耦**：`slg-core` 是纯 Rust 库，不出现任何 Bevy 类型。战斗、寻路、领地、生成都可脱离引擎单测与基准测试。即使未来 Bevy 发生重大变更或更换渲染层，核心资产不受影响。
2. **编辑器与游戏共基座**：编辑器不是独立程序，而是游戏视图的**超集模式**——同一套 Chunk 渲染、同一套相机/拾取、同一份地图数据，只是叠加了工具层与命令栈。这保证"所见即所得"，且编辑器天然跟上游戏所有渲染升级。
3. **一切内容皆数据**：武将/战法/兵种/建筑/地形/事件全部 RON 数据表驱动，代码中没有硬编码内容。这是 Mod 生态与长期内容供给的地基。
4. **确定性优先**：种子驱动 RNG、纯函数战斗模拟、禁止生成管线使用 HashMap 迭代序——同输入同输出，支撑战报回放、地图分享、回归测试。
5. **存档 = 地图引用 + 增量**：存档不复制地图，而是引用地图文件内容哈希 + 存储运行状态与地形变更增量，保证地图可独立分享、存档轻量。

---

## 4. 核心技术决策清单

> 这是全项目的决策单一事实源。有分叉的地方只保留一个结论。

| # | 决策项 | 结论 | 理由（一句话） |
|---|--------|------|----------------|
| D1 | 渲染/GUI 框架 | **Bevy（2D）+ bevy_egui** 混合 | Bevy 负责大地图渲染/ECS/相机，egui 负责面板与编辑器表单；纯 egui 渲染 10 万格无 batching 不可行；Iced/Dioxus/Slint 是应用 UI 框架，不适合游戏地图 |
| D2 | 时间制 | 可暂停实时制，100 ms/tick | 保留行军时间的战略感，暂停兼顾思考友好 |
| D3 | 网格类型 | **六边形网格**（axial 坐标 `(q,r)`、pointy-top、6 邻域，计算用 cube 坐标） | 产品决策（Q2）；六边形邻接等距，行军与战线呈现更自然，贴近文明/三国志等经典 SLG 手感；"相邻占领"铺路规则完全保留；算法参照 Red Blob Games |
| D4 | 地图规模 | V1 默认 **256×256**（6.5 万格），可选 512×512（26 万格），架构支持到 2048×2048 | Chunk 32×32 分块设计对 420 万格仍有 10 倍内存余量；V1 默认 256² 降低渲染/生成/调试成本，512² 作为大地图可选项 |
| D5 | 地图运行时存储 | **Chunk 实体 + 32×32 定长数组**（非逐格 ECS 实体） | 逐格实体 420 万格 archetype 开销 400MB+；Chunk 聚合后全图 ~20MB 且缓存友好 |
| D6 | 地图文件格式 | 自定义二进制容器 `.slgmap`（bincode + zstd 分节 + 内嵌预览图），提供 RON 导出 | 大地图纯 RON 体积/速度不可接受；容器可嵌预览图与校验和；RON 导出服务 Mod 与调试 |
| D7 | 存档格式 | `.slgsave` 同构容器 = 地图哈希引用 + 状态/增量节 | 地图独立分享、存档轻量；哈希校验防地图被改导致坏档 |
| D8 | 配置数据格式 | **RON**（武将/战法/兵种/地形/事件/预设表） | 人类可读、支持注释与枚举、Git diff 友好、serde 原生 |
| D9 | 随机数 | 统一 **ChaCha12Rng**（`rand_chacha`） | 种子确定、跨平台一致、`jump()` 支持并行分块生成；战斗种子 = hash(双方, 格子, tick) |
| D10 | 寻路 | **Hex A\***（cube 坐标距离启发式），LRU 路径缓存，异步线程池；hex-JPS 保留描述但 V1 不实现 | 六边形 JPS 约束多、收益有限，A* + 缓存对 SLG 行军频次已足够；多部队并发入队限流（32 请求/tick） |
| D11 | 战斗 | **纯函数确定性模拟** `simulate(input) → CombatReport`，零 ECS 依赖 | 可独立单测/回放/并行；ECS 侧只做快照输入与结果写回 |
| D12 | 领地/铺路校验 | 每势力 **Union-Find** + 全局 owner map + 断连 BFS 脏标记 | 相邻占领判定 O(α(n))；断连分裂只在小连通块内 BFS |
| D13 | AI 架构 | **效用 AI（连续评分）+ 硬规则兜底**，分层：战略(50tick)/战术(10tick)/执行(每tick) | 行为树难表达"权衡"；效用评分自然涌现扩张/防守/外交；硬规则防蠢 |
| D14 | ECS 与全局状态 | 运行时状态**全在 Bevy World**（实体 + Resource）；存档 = World 子集快照 | 避免 ECS 与 GameState 双份状态同步地狱；序列化由快照系统完成 |
| D15 | 错误处理 | 库 crate 用 `thiserror`，应用层用 `anyhow`；禁止非测试代码 `unwrap()` | 标准实践 |
| D16 | 日志 | `tracing` + `tracing-appender` 文件轮转 + panic hook 崩溃落盘 | 结构化、可检索 |
| D17 | i18n | `fluent`，文案全走 ftl，首语言 zh-CN | Mozilla 标准，复数/变量支持完整 |
| D18 | 发行与打包 | **Steam** 为主渠道（`steamworks` crate，optional feature `steam`），`cargo-bundle` 独立安装包兜底；GitHub Actions 初期仅 Win 构建矩阵 | 产品决策（Q3/Q4）；Steam 创意工坊是 V3 地图/Mod UGC 的分发主渠道；成就/云存档作为 Steam 特性后续接入 |
| D19 | Mod 脚本 | V1 **只做数据 Mod**；枚举中预留 `Scripted` 变体，V3 评估 rhai | 数据 Mod 覆盖 80% 内容需求且零安全风险；脚本化推迟到数据格式稳定后 |
| D20 | Bevy 版本策略 | 开工日 pin 最新稳定版（设计按 0.15+ API），`slg-core` 隔离引擎依赖 | Bevy 每个 minor 有破坏性变更；核心层隔离使升级成本局部化 |

---

## 5. Workspace 与模块划分

### 5.1 Crate 结构

```
rh-slg/
├── Cargo.toml                     # workspace 根
├── ARCHITECTURE.md                # 本文档（架构基线）
├── assets/                        # 运行时资源
│   ├── textures/  audio/  fonts/
│   ├── data/                      # 内置数据表（RON）
│   │   ├── generals.ron  skills.ron  unit_types.ron
│   │   ├── terrain_types.ron  buildings.ron  events.ron
│   │   └── presets/*.ron          # 生成预设
│   ├── i18n/zh-CN/*.ftl
│   └── maps/                      # 内置地图/剧本
├── crates/
│   ├── slg-data/                  # 共享数据结构（仅依赖 serde）
│   │   └── config.rs / map_doc.rs / save.rs / ids.rs
│   ├── slg-core/                  # ★纯逻辑（不依赖 Bevy）
│   │   └── map/（grid, tile, pathfinding, territory, fog）
│   │       rule/（combat, economy, movement）
│   │       entity/（general, army, city, faction）
│   │       ai/（utility, diplomacy, persona）
│   │       gen/（terrain, resource, spawn, validate）
│   │       event.rs  clock.rs
│   ├── slg-engine/                # Bevy 渲染/输入/时钟
│   │   └── render/（chunk_mesh, lod, fog, atlas）
│   │       camera/（pan_zoom, picking）
│   │       systems/（tick_dispatcher, sync, march_viz）
│   ├── slg-ui/                    # egui 游玩 HUD
│   │   └── panels/（top_bar, minimap, general_card, battle_report, diplomacy）
│   ├── slg-editor/                # 编辑器逻辑 + egui 面板
│   │   └── tool/（brush, fill, select, stamp, entity_place）
│   │       command.rs（命令栈/撤销重做）  validate.rs  ui.rs
│   ├── slg-save/                  # 容器格式、压缩、版本迁移
│   ├── slg-assets/                # 数据表加载、Mod 合并、热重载
│   └── slg-app/                   # 入口、模式切换、插件组装
├── mods/                          # Mod 目录（玩家安装）
├── user/                          # 玩家数据（maps/ saves/ config.ron）
├── tests/                         # 跨 crate 集成测试
├── project/                       # 进度看板（PROGRESS.md，由 master-coordinator 维护）
└── .claude/agents/                # AI 开发团队定义（本项目全 AI 开发）
```

### 5.2 依赖方向（强约束，CI 检查）

```
slg-data ◄── slg-core ◄── slg-engine ◄── slg-ui ◄── slg-app
   ▲            ▲              ▲              ▲
   │            │              │              │
slg-save   slg-assets     slg-editor ─────────┘
```

- `slg-data`：零外部依赖（serde 除外），定义一切共享结构与 ID 类型
- `slg-core`：只依赖 `slg-data` + 纯算法 crate（noise/pathfinding/rand）
- `slg-engine`：把 `slg-core` 状态映射为 Bevy 实体与渲染
- `slg-editor`：依赖 core+engine，实现工具/命令/校验，UI 用 egui
- `slg-app`：唯一允许 `unwrap` 启动失败退出的地方
- **红线**：`grep -r "bevy" crates/slg-core crates/slg-data` 必须为空，纳入 CI

### 5.3 关键外部 crate 清单

| 领域 | 选型 | 备注 |
|------|------|------|
| 引擎 | `bevy`（2D features）+ `bevy_egui` | 版本开工日 pin，跟进 LTS 式升级纪律 |
| 调试 | `bevy_inspector_egui` | 运行时查看 ECS，仅 dev profile |
| 序列化 | `serde` + `ron` + `bincode` | RON=人类可读配置；bincode=容器内节 |
| 压缩 | `zstd` | 地图/存档容器分节压缩 |
| 随机/噪声 | `rand` + `rand_chacha` + `noise` | ChaCha12 统一 RNG；Simplex/fBm 生成 |
| 寻路 | 自研 JPS（基于 `slg-core` 网格）+ `pathfinding`（A* 回退） | JPS 针对方格优化 |
| 浮点确定性 | `libm` | 生成/战斗数学函数跨平台一致 |
| 错误 | `thiserror` / `anyhow` | 库/应用分层 |
| 日志 | `tracing` + `tracing-subscriber` + `tracing-appender` | JSON 文件轮转 |
| i18n | `fluent` + `i18n-embed` | zh-CN 首发 |
| 时间戳 | `chrono` | 存档元信息 |
| 测试 | `proptest`（属性）+ `insta`（快照）+ `criterion`(基准) | 战斗/生成是重点 |
| 打包 | `cargo-bundle` | 独立安装包（兜底渠道） |
| Steam | `steamworks`（optional feature `steam`） | 成就/云存档/创意工坊；V1 仅在启用 feature 时初始化，无 SDK 也能编译运行 |

---

## 6. 核心子系统设计

### 6.1 地图的三种形态（关键设计）

同一份地图在生命周期中有三种表达，**边界清晰、互不混用**：

```
        编辑器编辑/磁盘存储              运行时内存                 存档
┌──────────────────────┐      ┌──────────────────────┐   ┌─────────────────────┐
│  MapDocument         │ load │  Runtime World       │   │  .slgsave           │
│  (.slgmap 容器)      │ ───► │  Chunk Entity×N      │   │  map_hash: SHA-256  │
│  · meta/seed/preset  │      │  ├ TileTerrain[1024] │   │  turn/tick          │
│  · 地形层(RLE 密集)  │ ◄─── │  ├ TileOwner[1024]   │   │  faction_states[]   │
│  · 资源层(BTree 稀疏)│ save │  ├ TileLevel[1024]   │   │  entity 快照[]      │
│  · 实体层(BTree 稀疏)│      │  ├ TileResource[1024]│   │  tile_delta(变更格) │
│  · 规则层(区域/触发) │      │  └ ChunkDirty        │   │  event_log          │
│  · 预览图(PNG)       │      │  + 城池/部队/武将实体 │   └─────────────────────┘
└──────────────────────┘      └──────────────────────┘
```

- **MapDocument**（slg-data 定义）：面向磁盘与编辑器。地形层 100% 填充用 RLE 密集数组（压缩比 5~10×）；资源/实体层 <5% 填充用 BTreeMap 稀疏存储；规则层按区域存。
- **Runtime World**（slg-engine）：面向性能。每 32×32 格 = 1 个 Chunk Entity，内部定长数组，内存连续；只有城池/要塞等特殊格额外生成独立实体（全图数千个量级）。
- **Save**：面向轻量与分享。不复制地图，存 `map_hash` 引用 + 运行状态 + 被改变的格子增量。加载时校验哈希，不匹配则警告。

**形态转换接口（伪代码签名）**：

```rust
// ====== MapDocument → Runtime World ======
fn load_map_to_world(doc: &MapDocument, world: &mut World) -> Result<()>
// 流程：读 meta → 按 32×32 分 Chunk → RLE 解码地形 → 展开为 [TileData; 1024]
// → 生成 Chunk Entity + Components → 稀疏层(BTree)逐项生成独立 Entity

// ====== Runtime World → Save ======
fn world_to_save(world: &World, map_hash: [u8; 32]) -> Result<SaveFile>
// 流程：遍历 Chunk Entity → 计算 tile_delta（与 MapDocument 对比）
// → 快照武将/部队/城池/势力状态 → 组装 SaveFile

// ====== Save → Runtime World ======
fn load_save_to_world(save: &SaveFile, doc: &MapDocument, world: &mut World) -> Result<()>
// 流程：先 load_map_to_world(doc) → 再应用 save.tile_delta → 恢复实体/势力状态

// ====== MapDocument ↔ 磁盘 ======
fn save_map_to_file(doc: &MapDocument, path: &Path) -> Result<()>
fn load_map_from_file(path: &Path) -> Result<MapDocument>

// ====== Save ↔ 磁盘 ======
fn save_to_file(save: &SaveFile, path: &Path) -> Result<()>
fn load_save_from_file(path: &Path) -> Result<SaveFile>
```

> 以上函数均位于 `slg-data`（类型定义）与 `slg-save`（容器 IO）；`load_map_to_world` / `world_to_save` 桥接 `slg-data` ↔ `slg-engine`，由 `slg-app` 在模式切换时调用。

### 6.2 游戏循环与时间系统

```
Bevy Main Schedule
  └─ tick_dispatcher（每渲染帧）
       accumulator += frame_delta × speed_multiplier
       while accumulator ≥ 100ms:
           current_tick += 1
           run GameTickSchedule ──┐
                                  ▼
   ┌──────────── GameTickSchedule（逻辑 tick，与渲染帧解耦）────────────┐
   │ TickStart(指令注入) → ResourceProduction → BuildQueue →            │
   │ Recruitment → MarchAdvance → CombatResolution → TerritoryUpdate →  │
   │ AIDecision(错峰: faction i 在 tick%10==i 决策) → TickEnd(迷雾/事件)│
   └────────────────────────────────────────────────────────────────────┘
   单 tick 预算 < 10ms（100ms 间隔有 10 倍余量，×3 加速安全）
```

- 渲染插值：渲染侧读 `current_tick + accumulator/tick_duration` 平滑动画
- 事件总线：`GameEvent`（MarchArrived/CombatFinished/TileOccupied…）双缓冲，渲染层订阅播表现，逻辑层不依赖事件做决策

### 6.3 行军与寻路

- 坐标：`i32` 对，全局 key `u64 = (y << 32) | x`
- Hex A*（cube 坐标距离启发式）首选；地形移动代价数据表驱动（平原 1.0 / 山地 0.5 / 渡河 0.3，骑兵不可渡河）；河流跨越规则：仅渡口 hex 可渡
- 并发：路径请求进 `ComputeTaskPool` 队列，每 tick 限流 32 个；LRU 缓存（起终点+通行掩码为 key，命中率预计 >70%）
- 行军组件：预计算路径 + 预计算 `arrive_tick`，tick 内推进 path_index

### 6.4 战斗系统（纯函数确定性模拟）

```rust
// slg-core/src/rule/combat.rs —— 零 ECS 依赖
fn simulate(input: CombatInput) -> CombatReport
// CombatInput { seed, attacker: CombatSide, defender: CombatSide, terrain, weather }
// CombatSide  { generals: [GeneralSnapshot; 3], troops, formation, tech_bonuses }
// CombatReport{ rounds, final_troops, winner, exp_gained, loot }
```

- 流程：准备(阵法/克制系数) → 最多 8 回合(速度定序 → 战法概率发动 → 普攻 → 伤兵结算 → 撤退判定) → 战损
- 战法定义是**数据**（`SkillDef`：类型/发动率/目标策略/伤害公式枚举+参数），不是闭包
- 确定性：`seed = hash(攻方id, 守方id, tile_key, tick)`，同输入 → 同战报，可回放可验证
- 克制三角：骑→弓 / 弓→步 / 步→骑 各 ×1.15，反向 ×0.85
- ECS 衔接：`resolve_combats` 系统从 World 快照构建输入 → simulate → 写回兵力/经验 → 发事件 → 战报入 `CombatReportStore` 供 UI 查看

### 6.5 领地与铺路校验

- 每势力一个 Union-Find（路径压缩+按秩合并，根节点记连通块大小）
- 占地校验四步：目标格为空/敌 → **六邻（hex）**有己方格 → 该邻居与主城同连通分量 → `union` 合并
- 断连处理：格子被夺取时对该连通块做块内 BFS 分裂，不与主城相连的子块标记"飞地"，宽限 N tick 后自动丢失（块通常 <100 格，开销可忽略）

### 6.6 AI 势力系统

```
Layer 0 硬规则（每 tick 检查）: 主城被围→全军回防 / 兵力<阈值→停攻征兵 / 资源<7天消耗→停建
Layer 1 战略层（每 50 tick）: Region 价值评估 → 主攻方向；外交威胁评估 → 结盟/宣战
Layer 2 战术层（每 10 tick）: 候选动作效用评分（占地/攻城/增援/侦察），取 Top-N
Layer 3 执行层（每 tick）  : 战术指令 → 具体行军命令入队
```

效用评分示例：`score(占地) = 资源价值 × 距离衰减 × (1 - 敌方密度×0.5) × 性格攻击倾向`

反作弊纪律：AI 无隐形成就加成；难度阶梯只调三个显式参数——决策间隔倍率、资源倍率（困难 ≤1.2 且向玩家明示）、信息延迟 tick 数。

### 6.7 迷雾/视野

- 逻辑层：`FogOfWar` Resource，每 Chunk 一个 `[u8; 1024]`（0 未探索 / 1 已探索不可见 / 2 当前可见），TickEnd 更新；视野范围用 hex cube ring，遮挡用 cube 视线（line-of-sight）算法
- 渲染层：单通道 R8 纹理上传 GPU，fragment shader 采样混合（黑/半暗/透明三态）
- 大范围重算（迁城）分帧：每 tick 处理 64 格，跨多 tick 完成

### 6.8 ECS 数据映射表

> 对应 D5（Chunk 定长数组）与 D14（运行时状态全在 Bevy World）。以下列出每种游戏对象在 Runtime World 中的存储方式。

| 游戏对象 | 存储方式 | 说明 |
|----------|----------|------|
| Tile 地形 | Chunk Entity 上的 Component `TileTerrain([TerrainType; 1024])` | 每 32×32 格一个 Chunk Entity；TerrainType 为 u8 枚举，内存连续 |
| Tile 归属 | Chunk Entity 上的 Component `TileOwner([FactionId; 1024])` | FactionId 为 u8，0 表示无主 |
| Tile 等级 | Chunk Entity 上的 Component `TileLevel([u8; 1024])` | 1~9，对应资源产出与守军强度 |
| Tile 资源 | Chunk Entity 上的 Component `TileResource([ResourceType; 1024])` | 稀疏赋值，无资源格为 None；运行时展开为定长数组以保持缓存友好 |
| Chunk dirty 标记 | Chunk Entity 上的 Component `ChunkDirty(bool)` | 每 tick 有地形/归属变更时置 true，渲染系统增量更新 mesh |
| 武将 | ECS Entity + Components：`GeneralStats`（五维/等级/经验）、`GeneralSkills`（战法列表）、`GeneralTroopType`、`OwnerFaction` | 全图数百~数千；由招募/俘获/事件创建销毁 |
| 部队 | ECS Entity + Components：`ArmyTroops`（兵种/数量/士气）、`MarchPath`（预计算路径 + arrive_tick）、`Position`（当前 hex 坐标）、`OwnerFaction` | 行军推进每 tick 更新 path_index；战斗后兵力归零则 Entity 销毁 |
| 城池 | ECS Entity + Components：`CityLevel`（等级 1~10）、`CityGarrison`（守军配置）、`CityBuildQueue`（建造队列）、`Position`、`OwnerFaction`；静态配置（建筑列表/升级消耗）在 Resource `BuildingDefs` | 全图数十~数百；由地图编辑器/生成管线放置，运行时不创建新城 |
| 势力状态 | Resource `FactionStore`（`HashMap<FactionId, FactionState>`） | FactionState 含资源/外交关系/科技/Union-Find 根节点；4~8 个势力 |
| 游戏参数 | Resource `GlobalParams` | 经济/军事/外交倍率，从 `global_params.ron` 加载，剧本可覆盖 |
| 迷雾 | Resource `FogOfWar`（每 Chunk 一个 `[u8; 1024]`，0/1/2 三态） | TickEnd 由视野系统更新；渲染层单通道 R8 纹理上传 GPU |
| 命令队列 | Resource `CommandQueue`（`VecDeque<PlayerCommand>`） | 暂停时玩家指令入队，恢复时按序注入 TickStart 阶段 |
| 时钟 | Resource `GameClock`（`current_tick: u64`, `speed: Speed`, `accumulator: f64`） | tick_dispatcher 每渲染帧维护；Speed 枚举 = Paused / x1 / x2 / x3 |
| Union-Find 领地 | Resource `TerritoryGraph`（每势力一个 Union-Find 实例 + 全局 owner map） | 占地/断连 BFS 依赖此数据结构；§6.5 详述 |
| 寻路缓存 | Resource `PathCache`（LRU，key = 起终点 + 通行掩码） | 容量上限 4096 条，命中率预计 >70%；§6.3 详述 |
| 战报存储 | Resource `CombatReportStore`（`Vec<CombatReport>`） | 战斗模拟结果写入，UI 面板消费；§6.4 详述 |
| AI 决策槽 | Resource `AISlotAssignments`（`[FactionId; 10]`） | 开局随机分配，`tick % 10 == slot` 时执行；§2.3 / §6.6 详述 |

> **设计原则**：密集数据（地形/归属/等级/资源）走 Chunk 定长数组，保证缓存行连续性；稀疏实体（武将/部队/城池）走独立 ECS Entity，支持灵活增删；全局状态走 Resource，避免实体 archetype 碎片化。

---

## 7. 地图编辑器设计

### 7.1 架构定位

**编辑器 = 游戏视图的超集**。`slg-app` 通过 `EditorMode` Resource 切换模式；渲染管线、相机、拾取、Chunk 数据全部复用，编辑器只叠加：工具状态机、命令栈、校验器、egui 工具面板。

```
EditorMode {
  active, current_tool, active_layers(bitflag: 地形|归属|资源|实体|规则),
  brush_size(1/3/5/10), snap_to_grid, selected_faction, ghost_preview
}
EditorTool = Paint | FloodFill | Stamp(预设模板) | Select | Eyedropper | PlaceEntity
```

格子拾取：屏幕坐标 → 相机射线 → 与地面平面求交 → **cube 坐标 hex rounding**（取最近六边形中心，Red Blob Games cube rounding 算法）。

**编辑器 ↔ 游玩模式同步策略**：

编辑器与游玩模式共享同一份地图数据，但数据形态不同（MapDocument vs Runtime World）。切换流程：

| 切换方向 | 调用 | 流程 |
|----------|------|------|
| 编辑器 → 游玩 | `load_map_to_world(doc, world)` | 编辑器修改已保存到 MapDocument 内存；切换时将 MapDocument 完整展开为 Runtime World（§6.1 接口） |
| 游玩 → 编辑器 | `world_to_save(world, map_hash)` → 合并 delta 回 MapDocument | 提取运行时状态快照得到 tile_delta；将 delta 合并回 MapDocument 对应层（地形/归属/等级/资源），丢弃运行时临时实体（部队/战斗状态） |

> **V1 简化策略**：切换时全量转换，不保留游玩状态；编辑器始终以 MapDocument 为唯一事实源。V2 可优化为**实时双缓冲**——编辑器维护独立 MapDocument 副本，游玩模式读 Runtime World 副本，切换时仅同步 diff，降低大地图切换延迟。

### 7.2 编辑操作：命令模式 + 撤销重做

```rust
trait EditorCommand {
    fn execute(&self, doc: &mut MapDocument) -> Result<()>;
    fn undo(&self, doc: &mut MapDocument) -> Result<()>;
    fn merge_hint(&self) -> Option<MergeHint>;  // 连续笔刷合并为一次 stroke
}
// CommandHistory { undo_stack, redo_stack, max_depth: 200 }
```

命令集：PaintBrush / AreaFill / PlaceEntity / RemoveEntity / ModifyEntity / EditRiver / EditZoneRule / Composite（宏命令）。

### 7.3 实时校验系统（编辑器质量的关键）

| 校验项 | 级别 | 时机 |
|--------|------|------|
| 实体重叠、地形合法性 | Error | 每笔操作后（<5ms 轻量） |
| 全图连通性（Union-Find 飞地检测） | Error | 编辑间歇异步 |
| 各出生点资源包均衡度 | Warning | 编辑间歇异步 |
| 河流连续性、边界封闭性 | Warning | 编辑间歇异步 |
| 保存前全量校验 | Error 阻止保存 | 保存时 |

校验失败不只报错，还给**修复建议**（开凿通道/添加渡口/移动出生点）。

### 7.4 图层与暴露维度

五层可编辑：地形层（尺寸/类型/效果）、资源层（土地等级 1~9/资源倾向/特殊资源点）、建筑层（主城位/关隘城池/中立建筑）、势力层（数量/初始领土/属性/关系/目标）、规则层（胜利条件/事件触发器/全局参数倍率/特殊规则）。

### 7.5 编辑器用户流

```
新建空白 | 选预设模板 | 程序化生成(调参→生成→微调，推荐主路径) | 导入已有地图
   → 配置势力/规则/事件 → 校验 → 保存 .slgmap(内嵌预览图) → 沙盒加载游玩 / 文件分享给社区
```

**编辑器与生成的互补原则：生成是起点不是终点**——所有程序化生成结果都可在编辑器中修改。

---

## 8. 程序化生成

### 8.1 生成管线

```
主种子 → ChaCha12 派生子种子(地形/资源/出生点/天气)
→ 高程图(Simplex fBm 6-8 octave + Domain Warping)
→ 湿度图(独立通道 + 距水源衰减) + 温度图(纬度梯度 + 海拔衰减)
→ 地形分类([高程8档 × 湿度6档] 查找表 → TerrainType)
→ 河流后处理(山脊源头 → 最陡梯度下降 → 汇水累加定宽度 → 洼地成湖)
→ 率土要素投放
```

### 8.2 率土式要素投放

| 要素 | 算法 |
|------|------|
| 土地等级 | **圈层梯度 + 噪声扰动**：由外到内 4 圈 (1-3)/(3-5)/(5-7)/(7-9)，Simplex ±1~2 级扰动 → 中心富饶但有贫瘠缝隙，边缘偶有飞地 |
| 资源点 | **约束泊松盘采样**：密度图加权采样 → 地形掩码过滤（铁在山地、粮在平原）→ 等级范围过滤 |
| 关隘 | 6 方向可通行邻居扫描（恰 2 个近对向可通行邻居 + 两侧高程差大 = 隘口）→ 评分取 Top-K |
| 城池 | 多因子评分（中心度/防御性/连通度/周边资源）→ 贪心去重 → 连通性校验 |
| 出生点 | 泊松盘候选池(3~5 倍数量) → **模拟退火**优化公平性（能量 = -最小两两距离 + 资源方差），目标：各出生点资源/防御/扩展潜力评分两两差 < 0.1 |

### 8.3 连通性与铺路可达性双重校验

1. Union-Find 全图陆地块连通分量检查 → >1 个分量即存在飞地死局
2. 从每个出生点 BFS：互相可达？关隘/核心资源可达？
3. 不通过则自动修复建议（降山脉/加渡口/移出生点）

### 8.4 确定性纪律

- 统一 ChaCha12Rng，同种子跨平台同地图（种子可分享）
- 生成管线禁用 HashMap（迭代序不定）→ BTreeMap/IndexMap
- 浮点用 `libm` 确定性函数
- 并行分块生成用 `jump()` 分配独立随机流
- 生成预设（GenerationPreset：尺寸/富饶度/势力数/地形风格/自定义覆盖）序列化进地图 meta，可复现可分享

---

## 9. 数据驱动与 Mod 预留

### 9.1 数据表清单（RON）

| 表 | 关键字段 |
|----|---------|
| generals.ron | id/名称/稀有度/五维基础+成长/自带战法/可学习战法/可带兵种/立绘 |
| skills.ron | id/类型(主动/被动/指挥/突击)/发动率/目标策略/伤害公式(枚举+参数)/效果列表/来源武将 |
| unit_types.ron | id/兵种分类/攻防血速/征兵资源消耗/克制目标/地形适性表 |
| terrain_types.ron | id/移动代价/防御加成/可通行/可建造/美术集 |
| buildings.ron | id/类别/各级属性与消耗/地形需求/提供效果 |
| events.ron | id/触发条件/效果/脚本钩子(V1 留空) |
| global_params.ron | 经济/军事/地图/外交四组全局倍率；剧本可 `params_override` 局部覆盖 |

### 9.2 加载与合并

```
data/（内置） → mods/*/data/（按 mod.toml priority 排序） → user/（最高优先级）
合并规则：同 ID 覆盖整条记录；"+" 后缀文件为追加模式；__delete 标记删除；每次覆盖写冲突日志
存档只存数据 ID 不存内容 → Mod 改数值后旧存档自动生效
```

### 9.3 V1 即做的 Mod 预留（零成本）

- 关键枚举预留 `Scripted(String)` 变体（战法效果/事件条件/AI 行为）
- 所有数据表记录带 `custom_props: BTreeMap<String, PropValue>` 扩展字段
- 事件定义带 `script_hook: Option<String>`
- V3 脚本引擎候选：**rhai**（Rust 原生、类 JS、安全沙箱）> mlua > wasmtime

---

## 10. 存档系统

### 10.1 容器格式（.slgmap / .slgsave 同构）

```
┌─────────────────────────────┐
│ Magic "SLGM" │ Version u32  │  文件头
│ TOC Offset u64              │  → 指向末尾目录
├─────────────────────────────┤
│ Section: Meta        (bincode)          │
│ Section: TerrainLayer (bincode + zstd)  │
│ Section: ResourceLayer / EntityLayer …  │
│ Section: Preview PNG (256×256 缩略图)   │
├─────────────────────────────┤
│ TOC: 各节 offset/size/crc32 │
└─────────────────────────────┘
```

### 10.2 版本迁移纪律

- 每节带独立版本号；`slg-save` 维护 `migrate_vN_to_vN+1` 函数链，加载时自动迁移到最新
- 小版本新增字段用 `#[serde(default)]`；大版本保留旧结构体定义
- 迁移函数用 `insta` 快照测试锁定行为

### 10.3 存档 = 地图引用 + 增量

```
.slgsave {
  map_ref: { path, content_hash: SHA-256 }   # 加载时校验，不匹配→警告+选项
  tick, faction_states[], entity_snapshots[] # 势力资源/外交/武将/部队状态
  tile_delta: [(tile_key, old, new)]         # 相对地图原始状态的地形/归属变更
  event_log[]                                # 已触发事件，防重放
}
```

自动存档：每日（游戏内日）+ 关键事件（大会战/城池易手）前；手动存档槽位 ≥10。

**Delta 合并策略**：当 tile_delta 变更格数量超过全图 30% 时，自动触发一次全量快照重置 delta，防止后期存档膨胀。

---

## 11. 工程化与质量保障

### 11.1 测试策略（按 crate 分层）

| 层 | 手段 | 重点 |
|----|------|------|
| slg-core | 单测 + `proptest` 属性测试 | 战斗确定性（同种子 1000 次同结果）、铺路连通性、经济结算守恒 |
| slg-core/gen | `insta` 快照 | 同种子地图逐格快照，防生成回归 |
| slg-core | `criterion` 基准 | tick 耗时、JPS 路径耗时、战斗模拟耗时，CI 性能回归门禁 |
| slg-engine | 渲染冒烟（headless） | 启动不崩、Chunk 数正确 |
| 集成 | tests/ | 存档往返（save→load→diff 为空）、地图加载→开局→100 tick 推演 |

### 11.2 日志/崩溃/调试

- `tracing` 结构化日志：控制台 + 按日轮转 JSON 文件；级别按模块过滤
- panic hook 捕获 backtrace 落盘 `crash_*.log`
- dev profile 启用 `bevy_inspector_egui`（运行时 ECS 检视）+ 内置调试面板（改资源/传送/强制战斗结果——单机游戏调试即作弊器，大方提供）

### 11.3 窗口/分辨率/本地化/热重载

- 最小窗口 1280×720，egui 相对布局，高 DPI 自动缩放
- 文案 100% 走 Fluent ftl，首发 zh-CN，结构上支持 en-US
- 开发模式 `AssetPlugin::watch_for_changes`，改 RON 数据表即时生效（存档不受影响，因为存档存 ID）

### 11.4 发行、打包与 CI

- 发行渠道（产品决策 Q3）：**Steam 为主**——`steamworks` crate 置于 optional feature `steam` 之后，无 SDK 的开发构建可独立编译运行；`cargo-bundle`（NSIS/MSI 安装包）作兜底渠道
- 目标平台（产品决策 Q4）：**Windows 10+（64 位）为 V1 唯一主平台**；macOS/Linux 延后至 V1 之后视情况补充（CI 矩阵初期仅 Windows）
- GitHub Actions：tag 触发 → cargo build --release + bundle + Steam depot 上传脚本（steamcmd）
- Steam 特性规划：成就（绑定剧本胜利条件，V1 末接入）、云存档（V1 后，本地存档格式不变、同步为附加层）、**创意工坊 = V3 地图/Mod UGC 分发主渠道**
- CI 流水线：fmt + clippy(-D warnings) + test + **依赖方向检查**（core/data 禁止出现 bevy）+ 基准回归

### 11.5 维护纪律（长期）

1. **架构基线制度**：本文档的决策清单是唯一事实源；变更决策 = 改文档 + 升版本 + PR 评审
2. **Bevy 升级纪律**：每季度评估一次 minor 升级；升级只影响 slg-engine/slg-ui/slg-editor，core 层零改动即为合格
3. **数据格式兼容**：地图/存档/数据表三类格式的版本迁移链只增不改，旧测试夹具永久保留
4. **KPI 门禁**：性能预算（第 12 节）进 CI 基准测试，回归 >10% 阻断合并

---

## 12. 性能预算

### 12.1 内存预算（以 2048×2048 上限计，V1 实际 512² 约 1/16）

| 数据 | 估算 |
|------|------|
| Chunk 地形/归属/等级/资源数组 | ~20 MB |
| 迷雾数据 | ~4 MB |
| 寻路通行图 | ~4 MB |
| Union-Find × 50 势力 | ~5 MB |
| 武将/城池/部队实体 | ~8 MB |
| **逻辑层合计** | **~42 MB**（含纹理 < 200 MB） |

### 12.2 帧率 KPI

| 场景 | 目标 |
|------|------|
| 正常游玩 ×1 | 60 FPS，单 tick < 10 ms |
| ×3 加速 | 60 FPS，单帧逻辑 < 14 ms |
| 大会战（10+ 部队） | 战斗模拟分帧，每 tick ≤20 场，单场 < 0.25 ms |
| 全图迷雾重算 | 分帧总耗时 < 200 ms，无感知卡顿 |
| 编辑器 Flood Fill（全图） | < 500 ms（多线程） |
| 地图生成（256²） | < 5 s（并行分块 + 进度条 + 低清预览先行；512² 可选，目标 <15 s） |

### 12.3 已识别风险点对策

| 风险 | 对策 |
|------|------|
| LOD 切换集中触发 mesh 重建风暴 | 每帧最多重建 16 个 Chunk，排队 |
| AI 全势力同 tick 决策 | 错峰：势力 i 在 tick%10==i 决策 |
| 大量行军同时到达 | 战斗模拟限流分帧 |
| 断连 BFS 大图 | 块内 BFS 上限 500 格/次，超出延后 |

---

## 13. 研发路线图（V1 → V2 → V3）

### V1 —— 能玩（核心循环闭环）

**里程碑：内置预设一键生成地图 → 完整玩通一局 → 基本编辑能力**

| 模块 | 范围 | 占比 |
|------|------|------|
| 工程骨架 | workspace 8 crate、CI、日志、i18n 框架 | 10% |
| 大地图 | Chunk 渲染/LOD/相机/拾取/迷雾 | 15% |
| 核心玩法 | 铺路占地 + 资源循环 + 征兵建造 | 25% |
| 战斗 | 武将/战法/兵种 + 纯函数结算 + 战报 UI | 20% |
| AI | 4~8 势力效用 AI + 基础外交 | 15% |
| 剧本 | 首发剧本"三国鼎立"（5 个 AI 势力 + 玩家独立势力）+ 完整事件链 + 胜利条件 + 外交台词 | 10% |
| 编辑器雏形 | 地形笔刷/实体放置/撤销重做/基础校验 | 5% |

**V1 不做**：脚本 Mod、地形过渡美术、沙盒动态事件、多人、云存档。

### V2 —— 编辑器完整（UGC 起飞）

**里程碑：玩家可设计完整剧本（规则+事件+胜利条件）并分享，开箱即玩**

- 高级编辑器：河流编辑、规则层编辑、选区操作、图层管理、Stamp 模板库
- 全量校验 + 修复建议系统
- 剧本系统完整：事件链、区域规则、自定义胜利条件
- 沙盒模式：动态事件（天灾/叛乱/名将出世）
- 自定义生成预设导入导出
- 地形过渡美术（六边形 autotiling / 过渡规则表）
- 内置地图画廊（浏览/标签/预览图）

### V3 —— Mod 生态（社区驱动）

**里程碑：社区可创造新机制并互相组合**

- 完整 Mod 加载器（依赖解析/冲突检测/覆盖日志）
- rhai 脚本引擎（自定义战法/事件/AI），安全沙箱
- 资源 Mod（美术/立绘/UI 主题）
- Mod 工具链（数据表校验 CLI、模板、开发者文档）
- **Steam 创意工坊**对接（地图文件/Mod 一键订阅，产品决策 Q3）
- 生成管线读取 Mod 扩展数据（新地形/新资源类型）

### 路线图依赖链

```
V1 生成管线 ──► V2 全量校验（依赖生成稳定）──► V3 扩展生成（依赖 Mod 数据表）
V1 命令模式 ──► V2 规则层编辑 ──► V2 剧本系统 ──► V3 脚本事件
V1 数据表结构 ──► V2 自定义预设 ──► V3 Mod 合并/脚本（依赖格式稳定）
V1 地图容器 ──► V2 地图画廊（依赖预览图+格式稳定）──► V3 工坊分享
```

### 长期研发方向（V3 之后的北极星）

1. **编辑器深度 = 游戏寿命**：参考文明世界编辑器/CK3/魔兽编辑器的演进路径，规则编辑（不止地图编辑）是终极形态
2. **内容供给去中心化**：官方做工具与范式，社区做内容；数据格式稳定性高于一切新特性
3. **AI 人格化持续投入**：AI 势力剧本化、事件化、语音化，是单机 SLG 差异化的长期护城河

---

## 14. 风险登记册

| # | 风险 | 影响 | 缓解 |
|---|------|------|------|
| R1 | Bevy 0.x 破坏性升级 | 中 | core 层零引擎依赖；季度升级纪律；engine 层适配层封装 |
| R2 | 大地图渲染帧率不达标 | 高 | Chunk+LOD 架构已预留 16 倍规模余量；基准测试 CI 门禁 |
| R3 | 战斗数值平衡黑洞 | 中 | 纯函数 + proptest + 战报回放工具；数值全参数化可热调 |
| R4 | 同种子跨平台不一致 | 中 | ChaCha12 + libm + 禁 HashMap 迭代序 + 逐格快照测试 |
| R5 | 编辑器体验笨重导致 UGC 目标落空 | 高 | 校验+修复建议前置；"生成→微调"主路径降低创作门槛 |
| R6 | AI 势力呆板/作弊感 | 中 | 人格模板 + 效用 AI + 反作弊纪律（显式难度参数） |
| R7 | 范围蔓延（率土系统极多） | 高 | V1 严格闭环裁剪；路线图门禁：V1 不满不开 V2 |
| R8 | 单人/小团队产能 | 高 | 美术走 atlas 占位 + 社区；优先机制深度而非内容量 |

---

## 15. 产品决策记录

> 2026-07-29 确认。架构条款（D3/D10/D18 等）已同步更新至 v1.1。

| # | 问题 | 结论 | 对架构的影响 |
|---|------|------|-------------|
| Q1 | 题材/IP | **三国题材** | V1 内容数据表为三国内容（武将/剧本/事件/文案）；架构保持题材无关，换题材 = 换数据表/Mod |
| Q2 | 地图网格 | **六边形（hex）** | D3/D10 变更：axial 坐标 + cube 计算、Hex A* 寻路、hex rounding 拾取、六边形 tile atlas；铺路规则改为 6 邻域 |
| Q3 | 发行渠道 | **Steam 发行** | D18 变更：`steamworks` optional feature；创意工坊为 V3 UGC 主渠道；成就/云存档后续接入 |
| Q4 | 最低支持 OS | **Windows 10+（64 位）** | V1 仅 Windows 构建矩阵；macOS/Linux 延后至 V1 之后可选 |
| Q5 | 数据保存 | **本地保存（V1）** | 与 §10 现有设计一致：`user/saves/` + `.slgsave` 容器；Steam 云同步为发行后附加层，存档格式不变 |
| Q8 | 三国题材首批剧本 | **"三国鼎立"**：魏/蜀/吴三大 AI 势力 + 2 个群雄残部 AI 势力（公孙渊·辽东、孟获·南中），共 5 个 AI 势力；**玩家为独立势力**（自建君主名、一州起步、不附属任何阵营，与各势力外交地位平等） | 剧本数据落于 `assets/data/scenarios/sanguo_dl/`，由 content-designer 制作；5 个 AI 恰好落在 §2.3 推荐区间 |

### 剩余待决问题（不阻塞 V1 启动）

| # | 问题 | 默认值（未确认则按此执行） |
|---|------|--------------------------|
| Q6 | 排期基线 | 本项目全 AI 开发，按 agent 产能排期；路线图占比见 §13，由 master-coordinator 维护迭代 |
| Q7 | 美术风格（写实/水墨风/低多边形/像素） | V1 占位 atlas，风格决策不晚于 V1 中期 |

---

*本文档由架构评审整合生成（玩法策划 / 系统架构 / 引擎工程 / 程序化生成四个视角），当前基线 v1.4。*
