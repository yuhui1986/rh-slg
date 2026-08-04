# M3 执行计划：V1 完整可玩

> 版本：v1.0（2026-08-02）
> 基线：ARCHITECTURE.md v1.4
> 前置：M2 已完成（编辑器完整 + UGC 能力，313 个测试全通过）

---

## 1. M3 总体目标与验收标准

### 1.1 一句话目标

**从能创作到能发行**：玩家可完整通关"三国鼎立"剧本（从开局到胜利/失败），游戏流程完整无断点，性能稳定 60 FPS，数值经平衡调整可玩，编辑器可创建自定义剧本并分享，Steam 构建可选接入。

### 1.2 验收标准

| # | 标准 | 验证方法 |
|---|------|----------|
| AC-1 | 主菜单可选择"新游戏"/"继续游戏"/"编辑器"/"设置"，各入口正常工作 | 试玩验证 |
| AC-2 | 新游戏可选择剧本、自定义势力名称、选择难度，进入游戏后初始状态正确 | 试玩验证 |
| AC-3 | 游戏中可随时保存/加载存档，加载后状态完整恢复 | 试玩验证 |
| AC-4 | "三国鼎立"剧本可完整通关：占领洛阳 / 统一 / 存活 365 天任一条件达成后显示胜利画面 | 端到端验证 |
| AC-5 | 失败条件触发后显示失败画面（势力覆灭），含统计数据 | 试玩验证 |
| AC-6 | 5 个 AI 势力行为差异化明显（魏扩张、蜀外交、吴防御、辽东投机、南中保守），500 tick 推演无 panic | 试玩验证 + 集成测试 |
| AC-7 | 事件链在游戏过程中正确触发（黄巾余党/天命/名将投靠/势力覆灭），玩家可感知 | 试玩验证 |
| AC-8 | 外交系统完整可用：结盟/宣战/停战/送礼/威胁，盟约效果正确（共享视野/互不攻击） | 试玩验证 |
| AC-9 | 单 tick 耗时 < 10ms（256x256，正常游戏状态），×3 加速下 60 FPS 不掉 | 基准测试 |
| AC-10 | 512x512 地图可正常游玩，单 tick < 20ms | 基准测试 |
| AC-11 | 数值平衡：AI 不会过早碾压玩家，玩家有合理的扩张窗口期 | 试玩验证（3 局以上） |
| AC-12 | 编辑器可创建完整自定义剧本（地形+势力+事件+胜利条件），保存后可在"新游戏"中加载游玩 | 端到端验证 |
| AC-13 | 地图文件可导出为 .slgmap、可导入加载，预览图正确显示 | 试玩验证 |
| AC-14 | 设置菜单可调节：音量、窗口分辨率、游戏速度默认值、语言 | 试玩验证 |
| AC-15 | Steam 构建可选：`cargo build --features steam` 可编译（无 SDK 时降级为本地模式） | CI |
| AC-16 | `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test --workspace` 全绿 | CI |
| AC-17 | `grep -r "bevy" crates/slg-core crates/slg-data` 为空 | CI |
| AC-18 | 新增测试覆盖所有 M3 新系统（存档/游戏流程/平衡/Steam） | `cargo test` |

### 1.3 "完整可玩"定义

玩家可以：启动程序 -> 主菜单 -> 选择"三国鼎立"剧本 -> 自定义势力名 -> 进入游戏 -> 占地扩张 -> 征兵作战 -> 外交博弈 -> 事件触发 -> 达成胜利条件 -> 胜利画面（含统计）-> 返回主菜单。全程可存档/读档，60 FPS 流畅，AI 有挑战性。

---

## 2. 参与 Agent 列表及职责

| Agent | 职责范围 | M3 主要交付 |
|-------|---------|------------|
| **core-engineer** | slg-core + slg-data（纯逻辑，零 Bevy 依赖） | 游戏流程状态机、存档/读档逻辑、AI 行为调优、战斗/经济数值调整、性能热点优化 |
| **render-engineer** | slg-engine + slg-ui（Bevy 渲染 + egui HUD） | 主菜单 UI、新游戏设置 UI、胜利/失败画面、设置菜单、Steam 构建集成、性能渲染优化 |
| **editor-engineer** | slg-editor + slg-save（编辑器 + 存档容器） | 存档容器完善、编辑器创建剧本完整流程、地图导入导出打磨、编辑器 UI 优化 |
| **content-designer** | assets/data + assets/i18n（RON 数据表 + 文案） | 数值平衡调整、剧本内容完善、i18n 文案补充、难度配置 |
| **qa-engineer** | tests/ + CI + 基准（质量门禁） | M3 集成测试、性能基准、端到端通关测试、试玩验收 |
| **arch-guardian** | ARCHITECTURE.md 维护 + 红线巡检 | Steam 架构审查、存档格式审查、性能预算审查 |

### 跨 crate 边界红线

- core-engineer **禁止**引入 bevy/egui 依赖
- render-engineer **禁止**在渲染层写游戏规则
- editor-engineer **禁止**私改 slg-data 字段结构（新字段需经 arch-guardian 评审）
- content-designer **禁止**改逻辑或引擎代码

---

## 3. 任务卡清单

### 阶段 A：游戏流程完整性（无外部依赖，可并行启动）

---

#### M3-T01: 主菜单与游戏模式选择

- **负责**: render-engineer
- **目标**: 实现完整的主菜单界面，支持新游戏、继续游戏、编辑器、设置四个入口
- **具体内容**:
  - 在 `slg-ui/src/panels/` 新建 `main_menu.rs`
  - 主菜单 UI（egui 全屏面板）：
    - 游戏标题"天下策"居中显示
    - 四个按钮：新游戏 / 继续游戏 / 编辑器 / 设置
    - 背景：渲染引擎提供静态地图缩略图或渐变背景
    - 版本号显示（右下角）
  - "新游戏"入口：
    - 跳转到新游戏设置面板（M3-T02）
  - "继续游戏"入口：
    - 扫描 `user/saves/` 目录，列出最近存档（按时间排序）
    - 显示存档信息：剧本名、游戏天数、势力名、保存时间
    - 选择后加载存档
  - "编辑器"入口：
    - 跳转到编辑器模式（M2 已实现）
  - "设置"入口：
    - 跳转到设置面板（M3-T05）
  - `GamePhase` 扩展：
    - `Menu` / `NewGameSetup` / `Playing` / `Paused` / `Editor` / `GameOver`
  - 模式切换系统：
    - 主菜单 <-> 新游戏 <-> 游玩 <-> 编辑器 <-> 游戏结束
    - 切换时正确清理/初始化状态
  - i18n：所有菜单文案走 fluent（zh-CN）
- **验收标准**:
  - [ ] 主菜单正确显示四个入口按钮
  - [ ] "继续游戏"正确列出存档文件
  - [ ] 各入口跳转正常，无 panic
  - [ ] 所有文案走 ftl，无硬编码
  - [ ] `cargo test -p slg-ui` 通过
- **依赖**: 无（使用 M2 已有的 GamePhase 框架扩展）
- **复杂度**: 中
- **预估工时**: 1~2 个会话

---

#### M3-T02: 新游戏设置界面

- **负责**: render-engineer
- **目标**: 实现新游戏创建流程，支持剧本选择、势力自定义、难度设置
- **具体内容**:
  - 在 `slg-ui/src/panels/` 新建 `new_game.rs`
  - 剧本选择面板：
    - 列出所有可用剧本（从 `assets/data/scenarios/` 扫描）
    - 每个剧本显示：名称、描述、势力数量、地图尺寸
    - 选中后高亮，点击"下一步"
  - 势力自定义面板：
    - 显示玩家势力信息（来自剧本定义）
    - 可编辑：势力名称（默认"玩家"）、君主名
    - 显示初始资源、初始武将、出生位置预览
    - 势力颜色选择（预设 6 色）
  - 难度选择面板：
    - 简单 / 普通 / 困难 / 噩梦
    - 难度影响：AI 决策间隔倍率、资源倍率（向玩家明示）
    - 简单：AI 决策间隔 x1.5，资源 x0.8
    - 普通：AI 决策间隔 x1.0，资源 x1.0
    - 困难：AI 决策间隔 x0.8，资源 x1.2
    - 噩梦：AI 决策间隔 x0.6，资源 x1.5
  - 确认面板：
    - 显示所有设置摘要
    - "开始游戏"按钮 -> 调用 `setup_game` 加载剧本
    - "返回"按钮 -> 回到主菜单
  - `GameSetupConfig` 结构：
    - `scenario_id: String`
    - `player_faction_name: String`
    - `player_lord_name: String`
    - `difficulty: Difficulty`
    - `player_color: [f32; 3]`
  - i18n：所有文案走 fluent（zh-CN）
- **验收标准**:
  - [ ] 剧本列表正确显示所有可用剧本
  - [ ] 势力名称可自定义
  - [ ] 难度选择正确影响游戏参数
  - [ ] "开始游戏"正确初始化游戏状态
  - [ ] 所有文案走 ftl，无硬编码
  - [ ] `cargo test -p slg-ui` 通过
- **依赖**: M3-T01（主菜单入口）
- **复杂度**: 中
- **预估工时**: 1~2 个会话

---

#### M3-T03: 游戏内存档/读档系统

- **负责**: editor-engineer + core-engineer（协作）
- **目标**: 实现游戏中随时保存/加载存档的功能，存档包含完整游戏状态
- **具体内容**:
  - **core-engineer 交付**（slg-core）：
    - `GameStateSnapshot` 结构：完整游戏状态快照
      - `tick: u64`
      - `factions: BTreeMap<FactionId, FactionState>`
      - `territory: TerritorySnapshot`（owner_map + union-find 状态）
      - `entities: Vec<EntitySnapshot>`（武将/部队/城池状态）
      - `fog: FogSnapshot`
      - `event_log: Vec<EventLogEntry>`（已触发事件，防重放）
      - `diplomacy: DiplomacySnapshot`
    - `snapshot_game(world) -> GameStateSnapshot`：从当前状态提取快照
    - `restore_game(snapshot, world) -> Result<()>`：从快照恢复状态
    - 快照序列化：使用 bincode + zstd 压缩
  - **editor-engineer 交付**（slg-save）：
    - `SaveGameMeta` 结构：
      - `scenario_id: String`
      - `scenario_name: String`
      - `player_faction_name: String`
      - `difficulty: String`
      - `game_tick: u64`
      - `game_days: u32`
      - `saved_at: DateTime<Utc>`
      - `play_time_seconds: u64`
      - `preview_data: Vec<u8>`（缩略图 PNG）
    - `save_game(meta, snapshot, map_hash, path) -> Result<()>`：保存完整存档
    - `load_game(path) -> Result<(SaveGameMeta, GameStateSnapshot, PathBuf)>`：加载存档
    - 存档格式：`.slgsave` 容器（复用 M1 容器格式，扩展 Meta section）
    - 自动存档：每 100 tick（游戏内约 10 天）自动保存到 `user/saves/auto/`
    - 手动存档：玩家可随时按 F5 快速保存，或通过菜单保存到指定槽位
    - 存档槽位：至少 10 个手动槽位 + 无限自动存档（保留最近 20 个）
    - 存档列表：`list_saves() -> Vec<SaveGameMeta>`，按时间排序
  - **render-engineer 交付**（slg-ui）：
    - 保存/加载 UI：
      - 快捷键 F5 = 快速保存，F9 = 快速加载
      - 菜单中"保存游戏"按钮 -> 弹出保存面板（槽位选择 + 备注输入）
      - 菜单中"加载游戏"按钮 -> 弹出加载面板（存档列表 + 预览）
    - 自动存档提示：自动存档时右下角短暂提示"已自动保存"
  - `GamePhase` 扩展：`Saving` / `Loading` 状态（短暂过渡）
  - i18n：存档相关文案走 fluent
- **验收标准**:
  - [ ] F5 快速保存成功，F9 快速加载恢复完整状态
  - [ ] 保存后关闭程序重新启动，加载存档后游戏状态完全恢复
  - [ ] 自动存档每 100 tick 触发一次
  - [ ] 存档列表正确显示所有存档信息
  - [ ] 存档文件体积合理（256x256 < 5MB）
  - [ ] `cargo test -p slg-save -p slg-core` 通过
- **依赖**: 无（使用 M1 已有的存档容器框架扩展）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M3-T04: 胜利/失败画面与游戏统计

- **负责**: render-engineer + core-engineer（协作）
- **目标**: 实现游戏结束时的胜利/失败画面，显示统计数据与成就
- **具体内容**:
  - **core-engineer 交付**（slg-core）：
    - `GameStatistics` 结构：
      - `total_ticks: u64`（游戏总时长）
      - `tiles_occupied: u32`（占领格数）
      - `tiles_lost: u32`（丢失格数）
      - `battles_fought: u32`（战斗次数）
      - `battles_won: u32`（胜利次数）
      - `generals_recruited: u32`（招募武将数）
      - `generals_lost: u32`（损失武将数）
      - `alliances_formed: u32`（结盟次数）
      - `alliances_broken: u32`（背盟次数）
      - `peak_territory: u32`（最大领地数）
      - `peak_armies: u32`（最大部队数）
      - `events_triggered: u32`（触发事件数）
      - `resource_stats: ResourceStats`（总产出/总消耗）
    - `StatisticsTracker` Resource：游戏过程中持续追踪统计
    - 集成到 `GameTickSchedule` 各阶段：每 tick 更新相关统计
    - `VictoryResult` 扩展：包含胜利/失败原因详细描述
  - **render-engineer 交付**（slg-ui）：
    - 在 `slg-ui/src/panels/` 新建 `game_over.rs`
    - 胜利画面：
      - 全屏面板，金色主题
      - 大标题"天下归心"或对应胜利条件的文案
      - 胜利原因描述
      - 统计数据网格（战斗/领地/武将/外交各维度）
      - 游戏时长（现实时间 + 游戏内天数）
      - 按钮："再来一局" / "返回主菜单" / "查看最终地图"
    - 失败画面：
      - 全屏面板，暗色主题
      - 大标题"壮志未酬"
      - 失败原因描述
      - 统计数据（同胜利画面）
      - 按钮："读取存档" / "再来一局" / "返回主菜单"
    - 胜利/失败判定集成：
      - 每 tick 检查胜利条件（M2 已有 VictoryCondition 引擎）
      - 触发时暂停游戏，显示结束画面
    - i18n：所有文案走 fluent
- **验收标准**:
  - [ ] 占领洛阳后正确显示胜利画面
  - [ ] 势力覆灭后正确显示失败画面
  - [ ] 统计数据正确（战斗次数/占领格数等）
  - [ ] "再来一局"正确重置游戏状态
  - [ ] "读取存档"正确加载最近存档
  - [ ] 所有文案走 ftl，无硬编码
  - [ ] `cargo test -p slg-core -p slg-ui` 通过
- **依赖**: M2-T03（胜利条件引擎）、M3-T01（主菜单入口）
- **复杂度**: 中
- **预估工时**: 1~2 个会话

---

#### M3-T05: 设置菜单

- **负责**: render-engineer
- **目标**: 实现游戏设置面板，支持音量、分辨率、游戏速度、语言等配置
- **具体内容**:
  - 在 `slg-ui/src/panels/` 新建 `settings.rs`
  - `GameSettings` 结构（RON 可序列化）：
    - `audio: AudioSettings` — `master_volume: f32`, `music_volume: f32`, `sfx_volume: f32`
    - `video: VideoSettings` — `resolution: (u32, u32)`, `fullscreen: bool`, `vsync: bool`
    - `gameplay: GameplaySettings` — `default_speed: Speed`, `auto_save_interval: u32`, `show_tutorial: bool`
    - `language: String` — 语言代码（`zh-CN` / `en-US`）
  - 设置面板 UI（egui）：
    - 分页标签：音频 / 视频 / 游戏 / 语言
    - 音频页：三个音量滑块（0~100）
    - 视频页：分辨率下拉、全屏开关、垂直同步开关
    - 游戏页：默认速度选择、自动存档间隔、新手教程开关
    - 语言页：语言选择下拉（zh-CN / en-US）
    - "应用"按钮 -> 保存设置到 `user/config.ron`
    - "恢复默认"按钮 -> 重置所有设置
    - "返回"按钮 -> 回到主菜单
  - 设置持久化：
    - 保存到 `user/config.ron`
    - 启动时自动加载
    - 缺失文件时使用默认值
  - 设置应用：
    - 音量设置即时生效（预留音频系统接口）
    - 分辨率设置需要重启提示
    - 语言设置切换 i18n 语言
  - i18n：设置面板文案走 fluent
- **验收标准**:
  - [ ] 设置面板四个分页正确显示
  - [ ] 音量滑块可调节
  - [ ] 分辨率下拉可选择
  - [ ] 设置保存到文件，重启后恢复
  - [ ] "恢复默认"正确重置
  - [ ] 所有文案走 ftl，无硬编码
- **依赖**: M3-T01（主菜单入口）
- **复杂度**: 低
- **预估工时**: 1 个会话

---

### 阶段 B：剧本通关验证与调优（依赖阶段 A 部分）

---

#### M3-T06: setup_game 完整实现（剧本加载流程）

- **负责**: core-engineer
- **目标**: 将 `setup_game` 从硬编码实现改为完整的剧本加载流程，支持任意剧本初始化
- **具体内容**:
  - 重写 `slg-app/src/lib.rs` 的 `setup_game`：
    - 接收 `GameSetupConfig`（来自 M3-T02）
    - 加载指定剧本 RON 文件（`assets/data/scenarios/{id}/scenario.ron`）
    - 按剧本定义初始化势力：
      - 创建势力状态（资源/外交/人格）
      - 放置初始城池/部队/武将
      - 设置初始领地（按 `initial_territory_radius` 生成圆形领地）
      - 设置初始外交关系
    - 加载事件链定义（`event_chains.ron`）
    - 加载胜利条件定义（`victory_conditions.ron`）
    - 加载区域规则定义（`zone_rules.ron`）
    - 注册 AI 槽位（随机分配到 0~9）
    - 初始化迷雾（玩家势力视野）
    - 初始化统计追踪器
  - `ScenarioLoader` 结构：
    - `load_scenario(path) -> ScenarioData`：加载剧本所有数据
    - `validate_scenario(data) -> Result<()>`：校验剧本完整性
    - `apply_scenario(data, world) -> Result<()>`：应用剧本到游戏状态
  - 难度参数应用：
    - `DifficultyConfig` 结构：AI 决策间隔倍率、资源倍率
    - 从 `GameSetupConfig.difficulty` 映射到 `DifficultyConfig`
    - 应用到 `GlobalParams`
  - 玩家势力配置：
    - 使用 `GameSetupConfig.player_faction_name` 覆盖默认名
    - 使用 `GameSetupConfig.player_color` 覆盖默认色
  - i18n：剧本名称/描述走 fluent
- **验收标准**:
  - [ ] 选择"三国鼎立"剧本后正确初始化 6 个势力
  - [ ] 各势力初始位置/资源/武将/领地正确
  - [ ] 外交关系按剧本定义初始化
  - [ ] 事件链/胜利条件/区域规则正确加载
  - [ ] 难度设置正确影响游戏参数
  - [ ] 玩家势力名称/颜色正确应用
  - [ ] `cargo test -p slg-core -p slg-app` 通过
- **依赖**: M3-T02（新游戏设置界面提供配置）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M3-T07: AI 势力行为全面调优

- **负责**: core-engineer
- **目标**: 调优 5 个 AI 势力的行为，使其差异化明显、有挑战性但不作弊
- **具体内容**:
  - 重写 `slg-core/src/ai/` 各模块：
    - `persona.rs`：细化人格模板
      - 魏（曹操）：高扩张高攻击，优先占地和征兵，外交上强势威胁
      - 蜀（刘备）：中等扩张，重视外交结盟，偏好防守反击
      - 吴（孙权）：低扩张高防御，重视水路和关隘，外交上游刃有余
      - 辽东（公孙渊）：低扩张低攻击，偏好偏安一隅，偶尔投机
      - 南中（孟获）：中等攻击低扩张，偏好游击战，不善外交
    - `utility.rs`：完善效用评分
      - 占地评分：资源价值 x 距离衰减 x (1 - 敌方密度 x 0.5) x personality.expansion
      - 攻击评分：目标兵力比 x 距离衰减 x personality.aggression x 关系衰减
      - 防御评分：威胁度 x personality.caution
      - 外交评分：关系值 x personality.diplomacy x 共同敌人
      - 征兵评分：兵力缺口 x 资源充裕度
      - 建造评分：建筑优先级 x 资源充裕度
    - `strategy.rs`：完善战略层
      - 区域价值评估：资源密度 x 防御性 x 连通性
      - 主攻方向选择：综合威胁与机会
      - 外交策略：结盟条件/宣战条件/停战条件
    - `tactics.rs`：完善战术层
      - 部队调度：多部队协同进攻
      - 增援逻辑：前线部队不足时派援军
      - 侦察逻辑：派小部队探索未知区域
  - 硬规则兜底完善：
    - 主城被围 -> 全军回防（已有）
    - 兵力 < 阈值 -> 停攻征兵（已有）
    - 资源 < 7 天消耗 -> 停建（已有）
    - 新增：盟友被攻击 -> 考虑援军
    - 新增：发现弱敌 -> 集中兵力进攻
  - AI 决策日志：
    - 每次 AI 决策记录原因（用于调试和玩家理解）
    - `AIDecisionLog` 结构：`faction_id`, `action`, `reason`, `tick`
  - 性能优化：
    - AI 决策缓存：相同状态不重复计算
    - 候选动作数量限制：每层最多评估 20 个候选
  - 确定性：AI 决策使用 ChaCha12Rng，种子 = hash(faction_id, tick)
- **验收标准**:
  - [ ] 魏势力扩张速度明显快于其他势力
  - [ ] 蜀势力结盟频率明显高于其他势力
  - [ ] 吴势力领地增长缓慢但防御坚固
  - [ ] 辽东势力偏安一隅，不主动扩张
  - [ ] 南中势力行为模式与其他势力不同
  - [ ] AI 不会做出违反规则的操作
  - [ ] 500 tick 推演无 panic
  - [ ] AI 决策日志可查看
  - [ ] `cargo test -p slg-core` 通过
- **依赖**: M3-T06（需要完整剧本加载才能测试 AI 行为）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M3-T08: 事件链完整触发验证与补充

- **负责**: core-engineer + content-designer（协作）
- **目标**: 验证所有事件链在游戏过程中正确触发，补充缺失的事件链
- **具体内容**:
  - **core-engineer 交付**：
    - 事件链引擎完善（`slg-core/src/event/chain.rs`）：
      - 确保所有触发条件类型正确实现
      - 确保所有效果类型正确执行
      - 事件链持久化：已触发事件记录到存档，加载后不重复触发
      - 事件链分支：支持玩家选择分支（通过 UI 弹窗）
    - 事件 UI 集成：
      - 事件触发时发送 `GameEvent::EventTriggered`
      - 渲染层订阅并显示事件通知面板（M2-T15 已有框架）
      - 分支选择：弹出选择面板，玩家选择后继续事件链
  - **content-designer 交付**：
    - 验证 `assets/data/scenarios/sanguo_dl/event_chains.ron`：
      - 黄巾余党事件链：确保触发条件正确（tick 范围/势力状态）
      - 天命事件链：确保天命值计算正确
      - 名将投靠事件链：确保武将出现位置合理
      - 势力覆灭事件链：确保连锁反应正确
    - 补充缺失事件链：
      - 开局事件：玩家势力建立时的引导事件
      - 季节事件：每 90 tick 触发的季节变化（影响资源/战斗）
      - 随机天灾：旱灾/洪水/蝗灾（触发条件/效果/持续时间）
      - 外交事件：势力间自动结盟/宣战的触发事件
    - 事件文案完善：
      - 所有事件描述/分支选项/结果描述走 i18n
      - 事件触发时的通知文案
    - 事件数值校验：
      - 确保事件效果数值合理（不破坏平衡）
      - 确保事件触发概率合理（不太频繁/太稀少）
  - 集成测试：
    - 测试所有事件链的触发条件
    - 测试事件效果的正确执行
    - 测试事件链的分支逻辑
- **验收标准**:
  - [ ] 黄巾余党事件链在游戏过程中至少触发一次
  - [ ] 名将投靠事件正确生成可招募武将
  - [ ] 势力覆灭事件正确触发连锁反应
  - [ ] 事件分支选择正确影响后续事件
  - [ ] 事件触发不导致 panic
  - [ ] 所有事件文案走 ftl
  - [ ] `cargo test -p slg-core` 通过
- **依赖**: M3-T06（需要完整剧本加载）、M2-T01（事件链引擎基础）
- **复杂度**: 中
- **预估工时**: 2 个会话

---

#### M3-T09: 外交系统完整流程

- **负责**: core-engineer
- **目标**: 完善外交系统，使结盟/宣战/停战/送礼/威胁全流程可用且有战略意义
- **具体内容**:
  - 完善 `slg-core/src/ai/diplomacy.rs`：
    - `DiplomacyAction` 枚举完善：
      - `ProposeAlliance` — 提议结盟（需要关系 >= 50）
      - `DeclareWar` — 宣战（关系降至 -100）
      - `ProposeCeasefire` — 提议停战（需要关系 >= -20）
      - `SendGift` — 送礼（消耗资源，关系 +10~30）
      - `Threaten` — 威胁（兵力优势时可用，关系 -10~20，对方可能屈服）
      - `TradeAgreement` — 贸易协定（双方资源产出 +5%）
    - 外交动作执行：
      - AI 发起外交动作 -> 玩家收到通知 -> 玩家选择接受/拒绝
      - 玩家发起外交动作 -> AI 根据关系/人格/局势决定接受/拒绝
    - 盟约效果：
      - 共享视野（盟友领地/部队可见）
      - 互不攻击（AI 不会攻击盟友）
      - 援军请求（盟友可请求援军）
    - 宣战效果：
      - 关系降至 -100
      - 解除所有盟约
      - AI 攻击策略调整
    - 停战效果：
      - 关系恢复至 0
      - 停止军事行动 N tick
    - 关系衰减：
      - 每 tick 关系向 0 微调（±0.1）
      - 领土接壤时关系 -0.05（扩张压力）
      - 共同敌人时关系 +0.02
  - 外交 UI 完善（`slg-ui/src/panels/diplomacy.rs`）：
    - 外交面板：显示所有势力关系值
    - 外交动作按钮：结盟/宣战/停战/送礼/威胁
    - 外交通知：收到外交提议时弹窗
    - 外交历史：最近 N 次外交动作记录
  - 外交事件：
    - 势力自动结盟/宣战的触发事件（M3-T08 的外交事件）
    - 第三方势力调停事件
  - i18n：外交文案走 fluent
- **验收标准**:
  - [ ] 玩家可向 AI 势力提议结盟，关系足够时 AI 接受
  - [ ] 宣战后关系降至 -100，AI 开始攻击
  - [ ] 结盟后共享视野，AI 不攻击盟友
  - [ ] 送礼正确消耗资源并增加关系
  - [ ] 外交通知正确弹出
  - [ ] 500 tick 推演外交系统无 panic
  - [ ] `cargo test -p slg-core` 通过
- **依赖**: M3-T06（需要完整剧本加载）、M3-T07（AI 外交决策）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

### 阶段 C：性能优化与数值平衡（依赖阶段 B 部分）

---

#### M3-T10: 性能分析与优化

- **负责**: core-engineer + render-engineer（协作）
- **目标**: 分析游戏性能热点，优化关键路径，确保 60 FPS 和 tick 预算
- **具体内容**:
  - **core-engineer 交付**：
    - 性能分析：
      - 使用 `criterion` 基准测试各系统耗时
      - 识别热点：AI 决策、寻路、战斗、领地更新
    - 优化措施：
      - AI 决策缓存：相同状态不重复计算效用评分
      - 寻路缓存优化：LRU 命中率监控与调整
      - 战斗模拟优化：预计算常用值（克制系数/地形适性）
      - 领地更新优化：增量更新而非全量重算
      - 事件链评估优化：分帧评估（每 tick 最多评估 10 条事件链）
    - 512x512 支持验证：
      - 确保 512x512 地图生成 < 15s
      - 确保 512x512 地图单 tick < 20ms
      - 确保 512x512 地图内存 < 400MB
  - **render-engineer 交付**：
    - 渲染性能分析：
      - 使用 Bevy 内置诊断工具分析帧耗时
      - 识别渲染热点：Chunk mesh 重建、迷雾更新、UI 绘制
    - 优化措施：
      - Chunk mesh 重建限流：每帧最多 16 个（已有，验证生效）
      - 迷雾纹理更新优化：仅更新变化的 Chunk
      - UI 绘制优化：egui 面板仅在可见时绘制
      - LOD 切换优化：平滑过渡而非突然切换
    - 内存优化：
      - 纹理图集压缩：使用 BC7/ASTC 压缩
      - Chunk 数据对齐：确保缓存行友好
      - 不可见 Chunk 卸载：远处 Chunk 释放 mesh 数据
  - 性能基准更新：
    - 更新 CI 基准回归检查
    - 新增 512x512 基准
    - 新增 AI 决策基准
    - 新增战斗模拟基准（10+ 部队并发）
- **验收标准**:
  - [ ] 256x256 正常游玩单 tick < 10ms
  - [ ] 256x256 ×3 加速下 60 FPS
  - [ ] 512x512 正常游玩单 tick < 20ms
  - [ ] 512x512 地图生成 < 15s
  - [ ] 内存使用 < 200MB（256x256）/ < 400MB（512x512）
  - [ ] 性能基准 CI 全绿
  - [ ] `cargo test --workspace` 通过
- **依赖**: M3-T06（需要完整游戏流程才能测试性能）、M3-T07（需要完整 AI 才能测试 AI 性能）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M3-T11: 数值平衡调整

- **负责**: content-designer + core-engineer（协作）
- **目标**: 调整战斗/经济/AI 数值，使游戏有挑战性但公平，玩家有合理的策略空间
- **具体内容**:
  - **content-designer 交付**：
    - 更新 `project/numbers.md`：
      - 基于试玩数据调整数值基准
      - 标注需要平衡调整的具体数值
    - 更新 `assets/data/global_params.ron`：
      - 经济倍率调整：确保早期扩张有压力，后期不至于资源溢出
      - 军事倍率调整：确保战斗有来有回，不至于一方碾压
      - 外交倍率调整：确保外交有战略意义
    - 更新 `assets/data/generals.ron`：
      - 调整武将五维数值，确保 S/A/B/C 级武将差距合理
      - 调整武将自带战法，确保搭配有深度
    - 更新 `assets/data/skills.ron`：
      - 调整战法发动率/伤害/效果，确保主动/被动/指挥/突击四类战法各有用途
      - 确保战法组合有协同效应
    - 更新 `assets/data/unit_types.ron`：
      - 调整兵种属性，确保克制三角 ±15% 合理
      - 调整地形适性，确保地形有战略意义
    - 更新 `assets/data/buildings.ron`：
      - 调整建筑效果/费用/建造时间，确保建造决策有意义
    - 更新 `assets/data/scenarios/sanguo_dl/scenario.ron`：
      - 调整各势力初始资源/武将/领地，确保开局公平
      - 调整初始外交关系，确保开局有外交空间
    - 难度配置：
      - 创建 `assets/data/difficulty.ron`：各难度的具体参数
      - 简单：AI 决策间隔 x1.5，资源 x0.8，AI 攻击性 -20%
      - 普通：AI 决策间隔 x1.0，资源 x1.0，AI 攻击性 0%
      - 困难：AI 决策间隔 x0.8，资源 x1.2，AI 攻击性 +20%
      - 噩梦：AI 决策间隔 x0.6，资源 x1.5，AI 攻击性 +40%
  - **core-engineer 交付**：
    - 难度参数应用：
      - `DifficultyConfig` 从 `difficulty.ron` 加载
      - 应用到 AI 决策间隔、资源产出、AI 攻击性权重
    - 平衡测试框架：
      - 自动化平衡测试：AI vs AI 100 局，统计胜率/平均时长/领地分布
      - 确保各势力胜率在 15%~25%（6 势力时）
  - 试玩迭代：
    - 至少 3 局完整试玩（简单/普通/困难各一局）
    - 记录每局关键数据：扩张速度/战斗胜率/外交行为/游戏时长
    - 基于试玩数据调整数值
- **验收标准**:
  - [ ] 普通难度下玩家有合理的扩张窗口期（前 100 tick 不被碾压）
  - [ ] 各势力胜率在 15%~25%（AI vs AI 测试）
  - [ ] 战斗有来有回（不存在一方碾压的情况）
  - [ ] 外交有战略意义（结盟/宣战时机影响局势）
  - [ ] 难度设置明显影响游戏体验
  - [ ] 所有 RON 文件可被正确解析
  - [ ] `cargo test --workspace` 通过
- **依赖**: M3-T07（需要完整 AI 才能测试平衡）、M3-T09（需要完整外交才能测试外交平衡）
- **复杂度**: 高
- **预估工时**: 2~3 个会话（含试玩迭代）

---

### 阶段 D：编辑器打磨（依赖阶段 A 部分，可与阶段 B/C 并行）

---

#### M3-T12: 编辑器创建完整剧本流程

- **负责**: editor-engineer
- **目标**: 验证并完善编辑器创建完整自定义剧本的端到端流程
- **具体内容**:
  - 完善编辑器剧本创建流程：
    - 新建剧本向导：
      - 步骤 1：选择地图（空白/生成/导入）
      - 步骤 2：配置势力（数量/名称/颜色/初始位置/初始资源）
      - 步骤 3：配置胜利条件（从模板选择/自定义）
      - 步骤 4：配置事件链（从模板选择/自定义/无）
      - 步骤 5：配置区域规则（可选）
      - 步骤 6：校验并保存
    - 势力配置面板：
      - 势力列表（添加/删除/编辑）
      - 每个势力：名称、颜色、初始城池位置（点击地图选择）、初始资源、初始武将
      - 玩家势力标记（is_player）
    - 胜利条件配置面板：
      - 条件列表（添加/删除/编辑）
      - 条件类型下拉：占领区域/占领城池/存活天数/消灭势力/资源阈值
      - 参数配置（根据类型动态显示）
    - 事件链配置面板：
      - 事件链列表（从模板选择/导入/手动创建）
      - 模板库：预置 5~10 个常用事件链模板
      - 事件链预览：节点+连线图
  - 剧本保存：
    - 保存为 `assets/data/scenarios/{name}/` 目录结构：
      - `scenario.ron` — 剧本定义
      - `event_chains.ron` — 事件链定义
      - `victory_conditions.ron` — 胜利条件定义
      - `zone_rules.ron` — 区域规则定义
      - `map.slgmap` — 地图文件
    - 保存时自动校验所有引用 ID 有效性
    - 保存后自动注册到剧本列表
  - 剧本加载：
    - 从"新游戏"界面可看到自定义剧本
    - 加载后正确初始化所有配置
  - i18n：向导文案走 fluent
- **验收标准**:
  - [ ] 可通过向导创建完整自定义剧本
  - [ ] 势力配置正确保存到 scenario.ron
  - [ ] 胜利条件配置正确保存
  - [ ] 事件链配置正确保存
  - [ ] 保存后可在"新游戏"中看到并加载
  - [ ] 加载后游戏正确初始化
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: M3-T01（主菜单入口）、M2-T10（规则层编辑器基础）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M3-T13: 地图文件分享与导入打磨

- **负责**: editor-engineer
- **目标**: 完善地图文件的导出/导入流程，确保文件可分享且加载正确
- **具体内容**:
  - 地图导出：
    - "导出地图"按钮 -> 选择保存位置 -> 生成 `.slgmap` 文件
    - 导出内容：地形层 + 资源层 + 实体层 + 规则层 + 预览图 + 元数据
    - 导出时自动校验（M2-T11 全量校验）
    - 导出时自动生成预览图（256x256 PNG）
  - 地图导入：
    - "导入地图"按钮 -> 选择 .slgmap 文件 -> 校验 -> 加载到编辑器
    - 导入校验：文件格式/版本/完整性
    - 导入后自动注册到画廊
  - 剧本导出/导入：
    - "导出剧本"按钮 -> 选择保存位置 -> 生成 `.slgscenario` 压缩包（含地图+配置）
    - "导入剧本"按钮 -> 选择 .slgscenario 文件 -> 解压 -> 校验 -> 注册
  - 文件格式版本管理：
    - `.slgmap` 容器版本号（已有，验证兼容性）
    - `.slgscenario` 容器版本号（新增）
    - 旧版本自动迁移（M1 已有迁移框架）
  - 预览图生成完善：
    - 预览图包含：地形 + 势力颜色 + 城池标记
    - 预览图尺寸：256x256
    - 预览图嵌入 .slgmap 容器
  - 画廊集成：
    - 导入的地图/剧本自动出现在画廊
    - 画廊显示预览图/名称/描述/标签
- **验收标准**:
  - [ ] 导出的 .slgmap 文件可在另一台机器上加载
  - [ ] 导入的 .slgmap 文件正确加载到编辑器
  - [ ] 导出的 .slgscenario 文件包含完整剧本数据
  - [ ] 导入的 .slgscenario 文件正确注册到剧本列表
  - [ ] 预览图正确生成并嵌入
  - [ ] 画廊正确显示导入的地图
  - [ ] `cargo test -p slg-editor -p slg-save` 通过
- **依赖**: 无（使用 M2 已有的文件格式框架）
- **复杂度**: 中
- **预估工时**: 1~2 个会话

---

### 阶段 E：Steam 构建（可选，可推迟到 M4）

---

#### M3-T14: Steam 构建配置

- **负责**: render-engineer + qa-engineer（协作）
- **目标**: 实现 Steam 可选构建，`cargo build --features steam` 可编译，无 SDK 时降级为本地模式
- **具体内容**:
  - **render-engineer 交付**：
    - `slg-app/Cargo.toml` 添加 optional feature：
      ```toml
      [features]
      steam = ["steamworks"]
      ```
    - `steamworks` crate 集成：
      - 仅在 `steam` feature 启用时初始化
      - 无 SDK 时编译通过（steamworks crate 支持）
      - 初始化失败时降级为本地模式（日志警告）
    - Steam API 封装（`slg-app/src/steam.rs`）：
      - `SteamClient` 结构：optional，仅在 feature 启用时存在
      - `init_steam() -> Option<SteamClient>`：初始化 Steam，失败返回 None
      - `is_steam_enabled() -> bool`：检查 Steam 是否启用
    - Steam 特性（基础版）：
      - Steam Overlay 支持（自动，无需额外代码）
      - Steam 成就接口（预留，不实现具体成就）
      - Steam 云存档接口（预留，不实现）
  - **qa-engineer 交付**：
    - CI 更新：
      - 新增 `cargo build --features steam` job（可选，允许失败）
      - 确保 `cargo build`（无 steam feature）仍然正常
    - 构建脚本：
      - `scripts/build_release.ps1`：Windows 发布构建脚本
      - `scripts/build_steam.ps1`：Steam 构建脚本（需要 SDK）
    - 文档：
      - `project/STEAM_BUILD.md`：Steam 构建说明文档
  - arch-guardian 审查：
    - 审查 Steam 集成架构设计
    - 确保 optional feature 不影响无 SDK 构建
- **验收标准**:
  - [ ] `cargo build`（无 steam feature）正常编译
  - [ ] `cargo build --features steam` 可编译（有 SDK 时）
  - [ ] 无 SDK 时 `cargo build --features steam` 编译通过但运行时降级
  - [ ] Steam Overlay 可用（有 Steam 时）
  - [ ] CI 全绿
  - [ ] `cargo test --workspace` 通过
- **依赖**: 无（独立任务）
- **复杂度**: 中
- **预估工时**: 1~2 个会话

---

### 阶段 F：集成与验证（依赖全部前置任务）

---

#### M3-T15: 全链路集成与端到端测试

- **负责**: render-engineer + core-engineer + editor-engineer（协作）
- **目标**: 将 M3 所有子系统串联，实现从主菜单到胜利/失败的完整游戏流程
- **具体内容**:
  - **主菜单 -> 新游戏 -> 游玩 -> 胜利/失败 -> 主菜单**完整流程：
    - render-engineer：主菜单/新游戏设置/游戏结束画面/设置菜单
    - core-engineer：剧本加载/AI 行为/事件链/外交/胜利条件
    - editor-engineer：存档/读档
  - **编辑器 -> 创建剧本 -> 新游戏加载 -> 游玩**完整流程：
    - editor-engineer：编辑器创建剧本
    - core-engineer：剧本加载
    - render-engineer：新游戏界面显示自定义剧本
  - **存档/读档完整性**：
    - 游戏中保存 -> 关闭程序 -> 重新启动 -> 加载存档 -> 状态完全恢复
    - 自动存档 -> 手动加载 -> 状态正确
  - **i18n 完整覆盖**：
    - 所有 M3 新增 UI 文案走 fluent
    - 所有新增事件/外交文案走 fluent
  - **模式切换完整性**：
    - 主菜单 <-> 新游戏 <-> 游玩 <-> 编辑器 <-> 游戏结束
    - 切换时正确清理/初始化状态
  - 端到端验证：
    - 完整通关一局"三国鼎立"剧本
    - 完整通关一局自定义剧本
    - 存档/读档验证
    - 性能验证
- **验收标准**:
  - [ ] 主菜单 -> 新游戏 -> 游玩 -> 胜利 -> 主菜单完整流程无 panic
  - [ ] 编辑器创建剧本 -> 新游戏加载 -> 游玩完整流程无 panic
  - [ ] 存档/读档后状态完全恢复
  - [ ] 所有 UI 文案走 ftl
  - [ ] 模式切换无状态泄漏
  - [ ] `cargo test --workspace` 通过
- **依赖**: M3-T01 ~ M3-T13 全部
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M3-T16: 集成测试与 M3 验收

- **负责**: qa-engineer
- **目标**: 编写 M3 集成测试、运行性能基准、试玩验证全部 M3 验收标准
- **具体内容**:
  - 集成测试（tests/）：
    - `test_main_menu_flow`：主菜单各入口正常工作
    - `test_new_game_setup`：新游戏设置流程完整
    - `test_save_load_roundtrip`：存档保存->加载->状态一致
    - `test_auto_save`：自动存档正确触发
    - `test_victory_condition_trigger`：胜利条件触发后显示胜利画面
    - `test_defeat_condition_trigger`：失败条件触发后显示失败画面
    - `test_campaign_playthrough`：三国鼎立剧本 500 tick 推演无 panic
    - `test_custom_scenario_playthrough`：自定义剧本完整游玩
    - `test_diplomacy_full_flow`：外交全流程（结盟/宣战/停战）
    - `test_event_chain_trigger`：事件链在正确 tick 触发
    - `test_ai_behavior_difference`：5 个 AI 势力行为有差异
    - `test_difficulty_impact`：难度设置影响游戏参数
    - `test_editor_create_scenario`：编辑器创建剧本完整流程
    - `test_scenario_export_import`：剧本导出->导入完整性
    - `test_performance_256`：256x256 性能基准
    - `test_performance_512`：512x512 性能基准
  - 性能基准（criterion）：
    - `bench_tick_256`：256x256 单 tick 耗时（目标 < 10ms）
    - `bench_tick_512`：512x512 单 tick 耗时（目标 < 20ms）
    - `bench_ai_decision`：AI 决策耗时
    - `bench_save_game`：存档保存耗时
    - `bench_load_game`：存档加载耗时
    - `bench_generation_512`：512x512 地图生成（目标 < 15s）
  - CI 更新：
    - 新增 M3 测试到 CI 矩阵
    - 基准回归检查更新
    - 512x512 基准可选（CI 时间考虑）
  - 代码审查：
    - 随机抽查 5 个 M3 新增文件
    - 检查注释完整性、命名规范、错误处理
  - 试玩验收：
    - 按 AC-1 ~ AC-18 逐项验证
    - 输出缺陷清单（Blocker / Warning / Suggestion）
    - 输出 M3 KPI 报告
  - 端到端通关测试：
    - 至少 1 局完整通关（普通难度）
    - 记录关键数据：游戏时长/扩张曲线/战斗次数/外交行为
- **验收标准**:
  - [ ] 所有集成测试通过
  - [ ] 性能基准基线入库
  - [ ] CI 全绿
  - [ ] 代码审查无 Blocker
  - [ ] 试玩验收 AC-1 ~ AC-18 全部通过
  - [ ] 输出完整 M3 验收报告
- **依赖**: M3-T01 ~ M3-T15 全部
- **复杂度**: 中
- **预估工时**: 2 个会话

---

## 4. 执行顺序

### 4.1 可并行的任务组

```
并行组 1（立即启动，无依赖）:
  M3-T01  主菜单与游戏模式选择    [render-engineer]
  M3-T05  设置菜单                [render-engineer]  （与 T01 同 agent，可交替）
  M3-T14  Steam 构建配置          [render + qa]      （独立，可选）

并行组 2（依赖 T01 完成后）:
  M3-T02  新游戏设置界面          [render-engineer]   （依赖 T01）
  M3-T03  存档/读档系统           [editor + core]     （独立，可与 T02 并行）

并行组 3（依赖 T02 完成后）:
  M3-T04  胜利/失败画面           [render + core]     （依赖 T01）
  M3-T06  setup_game 完整实现     [core-engineer]     （依赖 T02）

并行组 4（依赖 T06 完成后）:
  M3-T07  AI 行为调优             [core-engineer]     （依赖 T06）
  M3-T08  事件链验证与补充        [core + content]    （依赖 T06）
  M3-T09  外交系统完整流程        [core-engineer]     （依赖 T06）

并行组 5（依赖 T07/T08/T09 部分完成）:
  M3-T10  性能优化                [core + render]     （依赖 T06, T07）
  M3-T11  数值平衡调整            [content + core]    （依赖 T07, T09）
  M3-T12  编辑器创建剧本流程      [editor-engineer]   （依赖 T01）
  M3-T13  地图文件分享打磨        [editor-engineer]   （独立）

并行组 6（依赖全部前置）:
  M3-T15  全链路集成              [render + core + editor]
  M3-T16  集成测试与验收          [qa-engineer]       （依赖 T15）
```

### 4.2 必须串行的依赖链

**链 1：主菜单 -> 新游戏 -> 剧本加载 -> AI 调优 -> 性能优化**（关键路径）

```
T01 (主菜单)
 -> T02 (新游戏设置)
 -> T06 (剧本加载)
 -> T07 (AI 调优)
 -> T10 (性能优化)
 -> T15 (集成)
 -> T16 (验收)
```

**链 2：剧本加载 -> 事件链 -> 平衡调整**（内容主线）

```
T06 (剧本加载)
 -> T08 (事件链验证)
 -> T11 (数值平衡)
 -> T15 (集成)
```

**链 3：剧本加载 -> 外交 -> 平衡调整**（外交主线）

```
T06 (剧本加载)
 -> T09 (外交系统)
 -> T11 (数值平衡)
 -> T15 (集成)
```

**链 4：主菜单 -> 胜利/失败画面**（UI 链）

```
T01 (主菜单)
 -> T04 (胜利/失败画面)
 -> T15 (集成)
```

**链 5：存档系统**（独立链）

```
T03 (存档/读档)
 -> T15 (集成)
```

**链 6：编辑器链**（独立链）

```
T12 (编辑器创建剧本) + T13 (地图文件分享)
 -> T15 (集成)
```

### 4.3 关键路径分析

**关键路径**（决定 M3 最短完成时间）：

```
T01 (主菜单，1~2 会话)
 -> T02 (新游戏设置，1~2 会话)
 -> T06 (剧本加载，2 会话)
 -> T07 (AI 调优，2~3 会话)
 -> T10 (性能优化，2~3 会话)
 -> T15 (集成，2~3 会话)
 -> T16 (验收，2 会话)

总计：12~16 个会话
```

**并行路径 A**（内容链，可与关键路径并行）：

```
T06 -> T08 (事件链，2 会话) + T09 (外交，2 会话)
 -> T11 (数值平衡，2~3 会话)
总计：6~7 会话（并行完成后汇入 T15）
```

**并行路径 B**（编辑器链，可与关键路径并行）：

```
T12 (编辑器创建剧本，2~3 会话) + T13 (地图文件分享，1~2 会话)
总计：3~5 会话（并行完成后汇入 T15）
```

**并行路径 C**（存档链，可与关键路径并行）：

```
T03 (存档/读档，2~3 会话)
 -> T15 (汇入集成)
```

### 4.4 core-engineer 任务排序

core-engineer 负担较重（T03/T04/T06/T07/T08/T09/T10/T11 共 8 张卡），建议排序：

```
Session 1:  T06 (setup_game 完整实现) - 开始
Session 2:  T06 续
Session 3:  T07 (AI 行为调优)
Session 4:  T07 续
Session 5:  T07 续 + T08 (事件链验证)
Session 6:  T08 续 + T09 (外交系统)
Session 7:  T09 续
Session 8:  T10 (性能优化)
Session 9:  T10 续
Session 10: T11 (数值平衡)
Session 11: T11 续
Session 12: T03 (存档/读档) + T04 (胜利/失败)
```

### 4.5 render-engineer 任务排序

```
Session 1:  T01 (主菜单) + T05 (设置菜单)
Session 2:  T01 续 + T02 (新游戏设置)
Session 3:  T02 续 + T04 (胜利/失败画面)
Session 4:  T10 (渲染性能优化)
Session 5:  T14 (Steam 构建，可选)
Session 6:  T15 (集成)
```

### 4.6 editor-engineer 任务排序

```
Session 1:  T03 (存档/读档) - 开始
Session 2:  T03 续
Session 3:  T12 (编辑器创建剧本)
Session 4:  T12 续
Session 5:  T13 (地图文件分享)
Session 6:  T15 (集成)
```

### 4.7 content-designer 任务排序

```
Session 1:  T08 (事件链内容) - 等待 core T06 完成
Session 2:  T08 续
Session 3:  T11 (数值平衡)
Session 4:  T11 续
Session 5:  T11 续（试玩迭代）
```

### 4.8 qa-engineer 任务排序

```
Session 1:  T14 (Steam CI，可选)
Session 2:  T16 (集成测试编写)
Session 3:  T16 续（试玩验收)
Session 4:  T16 续（性能基准 + 报告)
```

---

## 5. 跨 agent 协作节点

### 5.1 协作节点清单

| # | 节点 | 参与方 | 协作内容 | 触发时机 |
|---|------|--------|---------|---------|
| C1 | GameSetupConfig 接口对齐 | render -> core | T02 定义的 GameSetupConfig 结构影响 T06 setup_game 实现 | T02 完成后 |
| C2 | 存档快照接口对齐 | core -> editor | T03 的 GameStateSnapshot 结构影响 slg-save 容器格式 | T03 期间 |
| C3 | 胜利/失败画面数据流 | core -> render | T04 的 GameStatistics 结构影响游戏结束面板设计 | T04 期间 |
| C4 | AI 行为与外交接口 | core -> content | T07/T09 的 AI 决策逻辑影响 T08/T11 的内容设计 | T07/T09 完成后 |
| C5 | 事件链分支 UI 数据流 | core -> render | T08 的事件分支选择需要 render 显示选择面板 | T08 期间 |
| C6 | 数值平衡与 AI 调优 | content -> core | T11 的数值调整影响 T07 的 AI 效用评分 | T11 期间 |
| C7 | 编辑器剧本格式 | editor -> core | T12 的剧本保存格式需 core 剧本加载器正确解析 | T12 期间 |
| C8 | Steam 架构审查 | render -> arch-guardian | T14 的 Steam 集成设计需 arch-guardian 评审 | T14 期间 |
| C9 | 性能优化与数值调整 | core <-> content | T10 的性能优化可能影响 T11 的数值设计 | T10 + T11 期间 |
| C10 | 端到端集成 | render + core + editor | 主菜单 -> 新游戏 -> 游玩 -> 胜利/失败 -> 主菜单完整流程 | T15 期间 |

### 5.2 协作协议

- 每个协作节点，发起方在完成报告中明确标注"下游影响"与"接口签名"
- 接收方在开始任务前先读取上游的接口定义
- 如有接口分歧，由 arch-guardian 依据 ARCHITECTURE.md 裁决
- master-coordinator 在协作节点完成后更新 PROGRESS.md

---

## 6. 风险与缓解

### 6.1 M3 层面风险

| # | 风险 | 影响 | 概率 | 缓解措施 |
|---|------|------|------|---------|
| MR1 | AI 调优耗时超预期（行为差异化难以实现） | 关键路径延长 | 高 | 先实现基础差异化（攻击性/扩张性权重），复杂行为后置；AI 决策日志辅助调试 |
| MR2 | 数值平衡黑洞（试玩迭代无限循环） | M3 延期 | 中 | 设定迭代上限（最多 3 轮试玩调整）；先用保守数值，后续热更新 |
| MR3 | 存档系统复杂度超预期（状态恢复不完整） | 存档功能降级 | 中 | T03 先实现核心状态（势力/领地/武将），边缘状态（事件日志/统计）后置 |
| MR4 | 性能优化效果不明显 | 60 FPS 不保 | 中 | T10 先 profile 热点，针对性优化；512x512 可降级为"可玩但不保证 60 FPS" |
| MR5 | 编辑器创建剧本流程复杂度超预期 | T12 延期 | 中 | T12 先实现最小可用（仅势力+胜利条件配置），事件链/区域规则可手动编辑 RON |
| MR6 | Steam 构建依赖 SDK 导致 CI 不稳定 | CI 阻塞 | 低 | Steam 构建设为可选 job，允许失败；无 SDK 时本地模式正常工作 |
| MR7 | 事件链分支 UI 实现困难 | 事件体验降级 | 中 | 分支选择用简单 egui 弹窗，不需要复杂 UI；可降级为自动选择（无分支） |
| MR8 | 外交系统 AI 决策过于复杂 | AI 调优困难 | 中 | 先实现简单外交（仅结盟/宣战），复杂外交（威胁/贸易）后置 |

### 6.2 性能预算检查点

| 检查点 | 时机 | 指标 | 不达标处理 |
|--------|------|------|-----------|
| 单 tick 耗时 | T10 完成 | 256x256 < 10ms | profile 热点，针对性优化 |
| 512x512 单 tick | T10 完成 | < 20ms | 可接受但标注"大地图性能警告" |
| 512x512 生成 | T10 完成 | < 15s | 降 octave 数 / 简化后处理 |
| 存档保存 | T03 完成 | < 2s | 压缩级别调整 / 异步保存 |
| 存档加载 | T03 完成 | < 3s | 异步加载 / 进度条 |
| AI 决策 | T07 完成 | < 1ms/势力 | 候选动作数量限制 / 缓存 |
| 内存使用 | T10 完成 | < 200MB (256x256) | 纹理压缩 / Chunk 卸载 |

### 6.3 回退策略

如果 M3 某个子系统延期导致无法在计划内完成全部验收标准：

1. **AI 调优延期**：M3 仅验证基础 AI 行为（占地/征兵/行军），复杂行为（外交/协同）推迟到 M3.5
2. **数值平衡延期**：使用 M2 的保守数值，标注"待平衡调整"，后续热更新
3. **性能优化延期**：M3 仅保证 256x256 可玩，512x512 标注"实验性"
4. **编辑器创建剧本延期**：M3 仅验证内置剧本可通关，自定义剧本推迟到 M3.5
5. **Steam 构建延期**：推迟到 M4（用户已确认可选）
6. **外交系统延期**：M3 仅实现基础外交（结盟/宣战），复杂外交推迟到 M3.5
7. **胜利/失败画面延期**：用简单文本面板替代，后续美化

**降级优先级**（按对"完整可玩"目标的影响排序，不可降级项标 *）：

1. *主菜单 + 新游戏设置（T01/T02）— 游戏入口，不可降级
2. *剧本加载（T06）— 游戏启动，不可降级
3. *胜利/失败条件（T04）— 游戏结束，不可降级
4. *AI 行为调优（T07）— 游戏挑战性，不可降级但可简化
5. 存档/读档（T03）— 可降级为仅手动保存
6. 事件链验证（T08）— 可降级为部分事件不触发
7. 外交系统（T09）— 可降级为基础外交
8. 性能优化（T10）— 可降级为仅 256x256
9. 数值平衡（T11）— 可使用保守数值
10. 编辑器创建剧本（T12）— 可推迟
11. Steam 构建（T14）— 可推迟到 M4

---

## 附录 A：任务卡总表

| 卡号 | 标题 | 负责 | 依赖 | 复杂度 | 阶段 |
|------|------|------|------|--------|------|
| M3-T01 | 主菜单与游戏模式选择 | render-engineer | 无 | 中 | A |
| M3-T02 | 新游戏设置界面 | render-engineer | T01 | 中 | A |
| M3-T03 | 游戏内存档/读档系统 | editor + core | 无 | 高 | A |
| M3-T04 | 胜利/失败画面与游戏统计 | render + core | T01 | 中 | A |
| M3-T05 | 设置菜单 | render-engineer | T01 | 低 | A |
| M3-T06 | setup_game 完整实现 | core-engineer | T02 | 高 | B |
| M3-T07 | AI 势力行为全面调优 | core-engineer | T06 | 高 | B |
| M3-T08 | 事件链完整触发验证与补充 | core + content | T06 | 中 | B |
| M3-T09 | 外交系统完整流程 | core-engineer | T06 | 高 | B |
| M3-T10 | 性能分析与优化 | core + render | T06, T07 | 高 | C |
| M3-T11 | 数值平衡调整 | content + core | T07, T09 | 高 | C |
| M3-T12 | 编辑器创建完整剧本流程 | editor-engineer | T01 | 高 | D |
| M3-T13 | 地图文件分享与导入打磨 | editor-engineer | 无 | 中 | D |
| M3-T14 | Steam 构建配置 | render + qa | 无 | 中 | E |
| M3-T15 | 全链路集成与端到端测试 | render + core + editor | T01~T14 | 高 | F |
| M3-T16 | 集成测试与 M3 验收 | qa-engineer | T15 | 中 | F |

**统计**：
- 总任务卡：16 张
- core-engineer：8 张（T03/T04/T06/T07/T08/T09/T10/T11）
- render-engineer：6 张（T01/T02/T04/T05/T10/T14）
- editor-engineer：4 张（T03/T12/T13/T15）
- content-designer：3 张（T08/T11/T15）
- qa-engineer：2 张（T14/T16）
- arch-guardian：审查角色（在 T14 节点参与 Steam 架构评审）

---

## 附录 B：M3 新增数据结构快速参考

| 数据结构 | 定义位置 | 消费方 |
|----------|----------|--------|
| GameSetupConfig | slg-ui/src/panels/new_game.rs | render 新游戏 UI, core setup_game |
| GameStatistics | slg-core/src/resource.rs | core 统计追踪, render 游戏结束画面 |
| StatisticsTracker | slg-core/src/resource.rs | core tick 各阶段 |
| SaveGameMeta | slg-save/src/container.rs | editor 存档, render 存档列表 |
| GameStateSnapshot | slg-core/src/resource.rs | core 快照, editor 存档容器 |
| DifficultyConfig | assets/data/difficulty.ron | core AI, content 难度配置 |
| DiplomacyAction (扩展) | slg-core/src/ai/diplomacy.rs | core 外交, render 外交 UI |
| AIDecisionLog | slg-core/src/ai/utility.rs | core AI 调试, render AI 日志面板 |
| GameSettings | user/config.ron | render 设置菜单 |
| TransitionRuleTable (扩展) | assets/data/terrain_transitions.ron | render autotiling, content 过渡数据 |

---

## 附录 C：M3 验收标准与任务卡映射

| AC | 验收标准 | 主要负责任务卡 |
|----|----------|---------------|
| AC-1 | 主菜单四个入口正常工作 | T01 |
| AC-2 | 新游戏可选择剧本/自定义势力/选择难度 | T02, T06 |
| AC-3 | 游戏中可保存/加载存档 | T03 |
| AC-4 | 三国鼎立剧本可完整通关 | T06, T07, T08, T09, T11 |
| AC-5 | 失败条件触发后显示失败画面 | T04 |
| AC-6 | 5 个 AI 势力行为差异化明显 | T07 |
| AC-7 | 事件链正确触发 | T08 |
| AC-8 | 外交系统完整可用 | T09 |
| AC-9 | 单 tick < 10ms，×3 加速 60 FPS | T10 |
| AC-10 | 512x512 可正常游玩 | T10 |
| AC-11 | 数值平衡：AI 不碾压玩家 | T11 |
| AC-12 | 编辑器可创建完整自定义剧本 | T12 |
| AC-13 | 地图文件可导出/导入 | T13 |
| AC-14 | 设置菜单可调节配置 | T05 |
| AC-15 | Steam 构建可选 | T14 |
| AC-16 | cargo fmt/clippy/test 全绿 | T16 |
| AC-17 | bevy 依赖检查通过 | T16 |
| AC-18 | 新增测试覆盖 M3 新系统 | T16 |
