# M2 执行计划：编辑器完整（UGC 起飞）

> 版本：v1.0（2026-08-02）
> 基线：ARCHITECTURE.md v1.4
> 前置：M1 已完成（核心循环闭环，170 个测试全通过）

---

## 1. M2 总体目标与验收标准

### 1.1 一句话目标

**从能玩到能创作**：玩家可在编辑器中设计完整剧本（规则+事件+胜利条件），使用高级编辑工具（河流/选区/Stamp/规则层），通过全量校验保证质量，分享后开箱即玩；同时沙盒模式支持动态事件，地形过渡美术提升视觉品质，地图画廊支持浏览与发现。

### 1.2 验收标准

| # | 标准 | 验证方法 |
|---|------|----------|
| AC-1 | 编辑器可绘制河流，连续性校验正确报错 | 试玩验证 |
| AC-2 | 编辑器支持框选/套索选区，可对选区批量操作 | 试玩验证 |
| AC-3 | 编辑器图层管理：可见性切换、锁定、活跃图层切换 | 试玩验证 |
| AC-4 | Stamp 模板库：保存区域为模板、从库中选择放置 | 试玩验证 |
| AC-5 | 规则层编辑：可定义区域规则（资源倍率/通行限制/特殊效果） | 试玩验证 |
| AC-6 | 自定义胜利条件：可配置占领目标/存活天数/势力覆灭等条件 | 试玩验证 |
| AC-7 | 事件链系统：支持触发条件/效果/分支，事件可按时间/状态/行为触发 | 试玩验证 + 单测 |
| AC-8 | 沙盒模式：动态事件（天灾/叛乱/名将出世）在游戏过程中随机触发 | 试玩验证 |
| AC-9 | 全量校验：保存前检查连通性/资源平衡/河流连续/实体重叠，失败附修复建议 | 试玩验证 |
| AC-10 | 生成预设可导出为文件、可导入使用，同预设同结果 | 单测 + 试玩 |
| AC-11 | 地形过渡平滑，相邻不同类型地形有过渡纹理，无硬切割 | 试玩验证 |
| AC-12 | 地图画廊：可浏览内置地图、按标签筛选、查看预览图 | 试玩验证 |
| AC-13 | 玩家可创建完整剧本（自定义事件链+胜利条件+区域规则），保存后可直接加载游玩 | 端到端验证 |
| AC-14 | `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test --workspace` 全绿 | CI |
| AC-15 | `grep -r "bevy" crates/slg-core crates/slg-data` 为空 | CI |
| AC-16 | 新增测试覆盖所有新系统（事件链/胜利条件/区域规则/动态事件/校验器） | `cargo test` |

### 1.3 "UGC 起飞"定义

玩家可以：打开编辑器 -> 新建或生成地图 -> 用高级工具编辑地形与河流 -> 定义区域规则 -> 配置事件链与胜利条件 -> 全量校验通过 -> 保存为 .slgmap -> 在地图画廊中看到 -> 加载后直接游玩自定义剧本。沙盒模式下，动态事件让每局体验不同。

---

## 2. 参与 Agent 列表及职责

| Agent | 职责范围 | M2 主要交付 |
|-------|---------|------------|
| **core-engineer** | slg-core + slg-data（纯逻辑，零 Bevy 依赖） | 事件链引擎、区域规则引擎、胜利条件引擎、动态事件系统、生成预设序列化 |
| **render-engineer** | slg-engine + slg-ui（Bevy 渲染 + egui HUD） | 地形 autotiling、地图画廊 UI、沙盒模式 UI、地形过渡渲染 |
| **editor-engineer** | slg-editor + slg-save（编辑器 + 存档容器） | 河流编辑、选区操作、图层管理、Stamp 模板、规则层编辑、全量校验+修复建议、地图画廊后端 |
| **content-designer** | assets/data + assets/i18n（RON 数据表 + 文案） | 沙盒事件内容、剧本事件链/胜利条件/区域规则、地形过渡美术数据、i18n 文案 |
| **qa-engineer** | tests/ + CI + 基准（质量门禁） | M2 集成测试、确定性验证、性能基准、试玩验收 |
| **arch-guardian** | ARCHITECTURE.md 维护 + 红线巡检 | 架构审查（事件链/区域规则/胜利条件的数据结构设计评审） |

### 跨 crate 边界红线

- core-engineer **禁止**引入 bevy/egui 依赖
- render-engineer **禁止**在渲染层写游戏规则
- editor-engineer **禁止**私改 slg-data 字段结构（新字段需经 arch-guardian 评审）
- content-designer **禁止**改逻辑或引擎代码

---

## 3. 任务卡清单

### 阶段 A：核心引擎扩展（无外部依赖，可并行启动）

---

#### M2-T01: 事件链引擎

- **负责**: core-engineer
- **目标**: 在 slg-core 中实现事件链系统，支持触发条件评估、效果执行、分支选择
- **具体内容**:
  - 在 `slg-core/src/event/` 扩展现有 `event.rs`，新建 `chain.rs`、`trigger.rs`、`effect.rs`
  - `EventChain` 结构：事件链定义，包含有序事件节点列表，每个节点有触发条件、效果列表、可选分支
  - `TriggerCondition` 枚举：
    - `TimeReached { tick: u64 }` — 游戏时间到达
    - `FactionState { faction: FactionId, condition: StateCondition }` — 势力状态（资源/兵力/领地数阈值）
    - `TileOccupied { coord: HexCoord, by: FactionId }` — 特定格子被占领
    - `FactionEliminated { faction: FactionId }` — 势力覆灭
    - `DiplomacyChanged { a: FactionId, b: FactionId, relation: RelationType }` — 外交关系变化
    - `RandomChance { probability: f64, cooldown_ticks: u64 }` — 随机触发（带冷却）
    - `And(Vec<TriggerCondition>)` / `Or(Vec<TriggerCondition>)` / `Not(Box<TriggerCondition>)` — 逻辑组合
  - `EventEffect` 枚举：
    - `GrantResources { faction: FactionId, resources: ResourceDelta }` — 授予资源
    - `SpawnArmy { faction: FactionId, coord: HexCoord, army: ArmyConfig }` — 生成部队
    - `SpawnGeneral { faction: FactionId, general_id: String }` — 生成武将
    - `ChangeDiplomacy { a: FactionId, b: FactionId, delta: i32 }` — 改变外交好感
    - `ModifyTerrain { coord: HexCoord, terrain: TerrainType }` — 修改地形
    - `ShowMessage { key: String, params: BTreeMap<String, String> }` — 显示消息（i18n key）
    - `SetBranchIndex { index: usize }` — 跳转到事件链指定节点
  - `EventChainStore` Resource：管理所有活跃事件链实例，每 tick 评估触发条件
  - 事件链评估集成到 `GameTickSchedule` 的 TickEnd 阶段
  - 确定性：随机触发使用 ChaCha12Rng，种子 = hash(chain_id, tick)
  - 在 slg-data 中定义 `EventChainDef`（RON 可序列化的事件链定义结构）
- **验收标准**:
  - [ ] TimeReached 触发：设置 tick=10 的事件在第 10 tick 正确触发
  - [ ] FactionState 触发：势力资源低于阈值时触发
  - [ ] 逻辑组合：And/Or/Not 条件正确求值
  - [ ] 效果执行：GrantResources 正确增减资源
  - [ ] 分支跳转：SetBranchIndex 正确跳转到指定节点
  - [ ] 确定性：同种子同事件链触发序列完全相同（proptest）
  - [ ] `cargo test -p slg-core` 通过
  - [ ] 无 bevy 依赖
- **依赖**: 无（M1 的 event.rs 基础已就绪）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M2-T02: 区域规则引擎

- **负责**: core-engineer
- **目标**: 实现基于地图区域的规则系统，支持区域内资源倍率、通行限制、特殊效果
- **具体内容**:
  - 在 `slg-core/src/rule/` 新建 `zone_rule.rs`
  - `ZoneRule` 结构：
    - `zone_id: String` — 区域标识
    - `tiles: BTreeSet<TileKey>` — 区域包含的格子集合（或几何定义：中心+半径/多边形）
    - `effects: Vec<ZoneEffect>` — 区域效果列表
    - `active: bool` — 是否激活
  - `ZoneEffect` 枚举：
    - `ResourceMultiplier { resource: ResourceType, factor: f64 }` — 资源产出倍率
    - `MovementCostMultiplier { factor: f64 }` — 移动代价倍率
    - `DefenseBonus { factor: f64 }` — 防御加成
    - `RestrictAccess { allowed_factions: Vec<FactionId> }` — 通行限制
    - `SpecialBuilding { building_id: String }` — 允许特殊建筑
  - `ZoneRuleStore` Resource：管理所有区域规则
  - 集成到经济系统：`tick_resources` 读取区域倍率
  - 集成到寻路系统：移动代价计算考虑区域倍率
  - 集成到战斗系统：防御计算考虑区域加成
  - 在 slg-data 中定义 `ZoneRuleDef`（RON 可序列化）
- **验收标准**:
  - [ ] 区域资源倍率：设置 2x 金产出区域内，金产出正确翻倍
  - [ ] 区域移动代价：设置 0.5x 移动代价区域内，行军速度加快
  - [ ] 区域防御加成：区域内战斗防御方获得加成
  - [ ] 通行限制：非允许势力无法通过限制区域
  - [ ] proptest：区域规则不影响区域外的计算
  - [ ] `cargo test -p slg-core` 通过
  - [ ] 无 bevy 依赖
- **依赖**: 无（与 M2-T01 并行，使用 M1 已有的经济/寻路/战斗接口）
- **复杂度**: 中

---

#### M2-T03: 自定义胜利条件引擎

- **负责**: core-engineer
- **目标**: 实现可配置的胜利条件评估系统，支持多种条件类型与组合
- **具体内容**:
  - 在 `slg-core/src/rule/` 新建 `victory.rs`
  - `VictoryCondition` 枚举：
    - `OccupyRegion { tiles: BTreeSet<TileKey>, label: String }` — 占领指定区域（如"占领洛阳"）
    - `OccupyCount { min_tiles: usize }` — 占领至少 N 格
    - `EliminateFaction { faction: FactionId }` — 消灭指定势力
    - `SurviveTicks { ticks: u64 }` — 存活至少 N tick
    - `ResourceThreshold { resource: ResourceType, amount: u64 }` — 资源达到阈值
    - `ControlAllCities { region: Option<String> }` — 控制所有城池（可限定区域）
    - `AllianceWith { factions: Vec<FactionId> }` — 与指定势力建立同盟
    - `And(Vec<VictoryCondition>)` / `Or(Vec<VictoryCondition>)` — 逻辑组合
  - `VictoryState` 结构：跟踪每个势力的胜利条件进度
  - `check_victory(state, faction_store, territory, tick) -> Option<VictoryResult>`：每 tick 评估
  - `VictoryResult { faction: FactionId, condition: VictoryCondition, tick: u64 }`
  - 集成到 `GameTickSchedule` 的 TickEnd 阶段
  - 支持多胜利条件（任一满足即胜利）与多失败条件（任一满足即失败）
  - 在 slg-data 中定义 `VictoryConfig`（RON 可序列化）
- **验收标准**:
  - [ ] OccupyRegion：占领指定格子集合后判定胜利
  - [ ] SurviveTicks：存活 N tick 后判定胜利
  - [ ] EliminateFaction：目标势力覆灭后判定胜利
  - [ ] And/Or 组合：复合条件正确求值
  - [ ] 多势力独立评估：各势力胜利条件互不影响
  - [ ] `cargo test -p slg-core` 通过
  - [ ] 无 bevy 依赖
- **依赖**: 无（使用 M1 已有的 TerritoryGraph / FactionStore 接口）
- **复杂度**: 中

---

#### M2-T05: 生成预设导入导出

- **负责**: core-engineer
- **目标**: 实现 GenerationPreset 的文件序列化，支持导出为 .ron 文件与从文件导入
- **具体内容**:
  - 在 `slg-core/src/gen/` 扩展 `preset.rs`（或新建）
  - `GenerationPreset` 结构完善：
    - `name: String` — 预设名称
    - `description: String` — 描述
    - `width: u32, height: u32` — 地图尺寸
    - `seed: u64` — 主种子（0 = 随机）
    - `terrain_style: TerrainStyle` — 地形风格（大陆/群岛/平原/山地）
    - `richness: f64` — 富饶度（0.0~2.0）
    - `faction_count: u8` — 势力数量
    - `custom_overrides: BTreeMap<String, PropValue>` — 自定义覆盖参数
    - `tags: Vec<String>` — 标签（用于画廊筛选）
  - `export_preset(preset, path) -> Result<()>`：导出为 RON 文件
  - `import_preset(path) -> Result<GenerationPreset>`：从 RON 文件导入
  - `validate_preset(preset) -> Result<Vec<ValidationWarning>>`：校验预设参数合法性
  - 预设文件格式：RON，带版本号，向前兼容（`#[serde(default)]`）
  - 内置预设：`presets/` 目录下预置 3~5 个典型预设（大陆/群岛/平原激战/群雄割据/新手友好）
- **验收标准**:
  - [ ] 导出 -> 导入往返：预设内容一致
  - [ ] 内置预设可正确加载
  - [ ] 无效参数（如 faction_count=0）被校验拦截并给出警告
  - [ ] `cargo test -p slg-core` 通过
  - [ ] 无 bevy 依赖
- **依赖**: 无（GenerationPreset 结构已在 M1 定义）
- **复杂度**: 低

---

### 阶段 B：高级编辑器工具（依赖阶段 A 部分）

---

#### M2-T06: 河流编辑工具

- **负责**: editor-engineer
- **目标**: 实现编辑器中的河流绘制与编辑功能，包含连续性校验
- **具体内容**:
  - 在 `slg-editor/src/tool/` 新建 `river.rs`
  - `RiverPaint` 命令：
    - 笔刷模式：在 hex 上标记为河流格
    - 河流宽度参数：1/2/3 格宽
    - 渡口标记：在指定 hex 标记为渡口（可通行）
  - `RiverErase` 命令：清除 hex 的河流标记
  - 河流数据存储：MapDocument 的 ResourceLayer 扩展河流层（`BTreeMap<TileKey, RiverData>`）
    - `RiverData { width: u8, is_ford: bool, direction: Option<FlowDirection> }`
  - 河流连续性校验（集成到 validate.rs）：
    - 检查河流 hex 是否形成连续路径（每个河流 hex 至少有一个相邻河流 hex，端点除外）
    - 检查河流是否形成环路（不允许）
    - 检查渡口位置合理性（渡口两侧必须有可通行地形）
  - 修复建议：断开的河流建议"添加连接段"；环路建议"断开某处"
  - 命令栈集成：RiverPaint/RiverErase 实现 EditorCommand trait
  - Ghost 预览：鼠标移动时显示将影响的河流 hex 范围
- **验收标准**:
  - [ ] 可在地图上绘制河流，河流 hex 有视觉标识
  - [ ] 可设置渡口位置
  - [ ] 河流连续性校验：断开处标红警告
  - [ ] 撤销/重做正常
  - [ ] 保存后重新加载河流数据不丢失
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: 无（MapDocument 结构可扩展，使用 M1 已有的命令栈框架）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M2-T07: 选区与区域操作

- **负责**: editor-engineer
- **目标**: 实现编辑器的框选/套索选区工具，支持对选区批量操作
- **具体内容**:
  - 在 `slg-editor/src/tool/` 新建 `select.rs`
  - `SelectTool` 状态机：
    - `BoxSelect`：鼠标拖拽矩形区域，选中所有 hex（hex 坐标在矩形范围内）
    - `LassoSelect`：鼠标自由绘制路径，选中路径围成的 hex 集合（hex 填充算法）
    - `AddToSelection` / `RemoveFromSelection`：Shift/Ctrl 修饰键
    - `SelectAll`：全选当前图层
    - `Deselect`：取消选择
  - 选区高亮渲染：选中的 hex 显示半透明高亮边框
  - `SelectionRegion` 结构：`BTreeSet<TileKey>` + 边界信息
  - 选区批量操作命令：
    - `BatchPaint`：对选区内所有 hex 批量修改地形
    - `BatchSetOwner`：对选区内所有 hex 批量设置归属
    - `BatchSetLevel`：对选区内所有 hex 批量设置土地等级
    - `CopyRegion` / `PasteRegion`：复制粘贴选区内容（含地形/资源/实体）
    - `FillSelection`：用当前工具填充选区
  - 所有批量操作实现 EditorCommand trait，支持撤销重做
  - 性能：选区操作 1000 格 < 100ms
- **验收标准**:
  - [ ] 框选可选中矩形区域内的 hex
  - [ ] 套索可选中自由绘制区域内的 hex
  - [ ] Shift/Ctrl 修饰键正确添加/移除选区
  - [ ] 批量地形修改正确应用到选区内所有 hex
  - [ ] 复制粘贴保留地形/资源/实体数据
  - [ ] 撤销/重做正确
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: 无（使用 M1 已有的命令栈与 hex 坐标系统）
- **复杂度**: 中

---

#### M2-T08: 图层管理

- **负责**: editor-engineer
- **目标**: 实现编辑器的图层管理面板，支持可见性切换、锁定、活跃图层选择
- **具体内容**:
  - 在 `slg-editor/src/` 扩展 `ui.rs`，新建 `layer_panel.rs`
  - `LayerManager` 结构：
    - `layers: Vec<LayerState>` — 图层列表
    - `active_layer: LayerType` — 当前活跃图层
  - `LayerState` 结构：
    - `layer_type: LayerType` — 地形/资源/建筑/势力/规则/河流
    - `visible: bool` — 是否可见
    - `locked: bool` — 是否锁定（不可编辑）
    - `opacity: f32` — 显示透明度（0.0~1.0）
  - `LayerType` 枚举：Terrain / Resource / Building / Faction / Rule / River
  - 图层面板 UI（egui）：
    - 图层列表，每层有可见性图标（眼睛）、锁定图标（锁）、透明度滑块
    - 点击图层设为活跃（高亮显示）
    - 拖拽排序（决定渲染叠放顺序）
  - 渲染集成：
    - 锁定图层的 hex 不响应编辑操作
    - 隐藏图层的 hex 不渲染对应内容
    - 活跃图层决定当前工具的作用对象
  - 快捷键：1~6 切换活跃图层，V 切换可见性，L 切换锁定
- **验收标准**:
  - [ ] 图层面板显示所有 6 个图层
  - [ ] 可见性切换：隐藏图层后对应内容不渲染
  - [ ] 锁定切换：锁定图层后无法编辑
  - [ ] 活跃图层切换：工具操作只影响活跃图层
  - [ ] 透明度滑块影响渲染
  - [ ] 快捷键正常工作
- **依赖**: 无（使用 M1 已有的编辑器模式框架）
- **复杂度**: 低

---

#### M2-T09: Stamp 模板库

- **负责**: editor-engineer
- **目标**: 实现编辑器的 Stamp 模板系统，支持保存区域为模板、从库中选择放置
- **具体内容**:
  - 在 `slg-editor/src/tool/` 新建 `stamp.rs`
  - `StampTemplate` 结构：
    - `name: String` — 模板名称
    - `description: String` — 描述
    - `tiles: BTreeMap<HexCoord, StampTile>` — 模板内容（相对坐标）
    - `size: (i32, i32)` — 模板尺寸
    - `tags: Vec<String>` — 标签
    - `preview_data: Vec<u8>` — 预览图数据（PNG bytes）
  - `StampTile` 结构：地形类型 + 资源 + 等级 + 河流标记 + 建筑
  - `SaveAsStamp` 命令：将当前选区保存为 StampTemplate
  - `PlaceStamp` 命令：在鼠标位置放置模板内容
    - 支持旋转（60° 步进，六边形对称）
    - 支持镜像翻转
    - Ghost 预览：放置前显示半透明预览
  - Stamp 库管理：
    - `StampLibrary` 结构：管理所有模板
    - 保存到 `user/stamps/*.ron`
    - 内置模板：`assets/data/stamps/` 预置常用模板（渡口/关隘/城池布局/资源区）
  - Stamp 面板 UI（egui）：
    - 模板列表（缩略图+名称）
    - 搜索/标签筛选
    - 旋转/镜像按钮
    - 删除/重命名
  - 命令栈集成：PlaceStamp 实现 EditorCommand trait
- **验收标准**:
  - [ ] 选区可保存为 Stamp 模板
  - [ ] 模板列表显示缩略图与名称
  - [ ] 放置模板正确还原地形/资源/建筑
  - [ ] 旋转功能：60° 步进正确变换 hex 坐标
  - [ ] 撤销/重做正常
  - [ ] 内置模板可加载使用
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: M2-T07（需要选区功能来保存模板）
- **复杂度**: 中

---

#### M2-T10: 规则层编辑器

- **负责**: editor-engineer
- **目标**: 实现编辑器中规则层的可视化编辑界面，包含区域规则、事件触发器、胜利条件配置
- **具体内容**:
  - 在 `slg-editor/src/tool/` 新建 `rule_editor.rs`
  - 在 `slg-editor/src/ui/` 新建 `rule_panel.rs`
  - 区域规则编辑：
    - 选区 -> 右键"定义区域" -> 弹出区域规则配置面板
    - 配置面板：区域名称、效果列表（资源倍率/移动代价/防御加成/通行限制）
    - 区域可视化：不同区域用不同颜色半透明覆盖
    - 区域边界编辑：可拖拽调整区域范围
  - 事件触发器编辑：
    - 事件列表面板：显示当前剧本的所有事件
    - 事件编辑器：触发条件配置（下拉选择类型 + 参数填写）
    - 效果配置：效果类型 + 参数
    - 分支配置：条件分支连线
    - 事件链可视化：节点+连线图（简化版，egui 实现）
  - 胜利条件编辑：
    - 胜利条件面板：列表显示所有胜利条件
    - 添加/删除/编辑条件
    - 条件类型下拉选择 + 参数配置
    - 支持 And/Or 组合
  - 所有规则数据写入 MapDocument 的 RuleLayer
  - 命令栈集成：所有编辑操作实现 EditorCommand trait
- **验收标准**:
  - [ ] 可为选区定义区域规则（资源倍率/通行限制）
  - [ ] 区域在地图上用颜色区分显示
  - [ ] 可创建事件链：配置触发条件+效果
  - [ ] 可配置自定义胜利条件
  - [ ] 所有规则数据可保存到 .slgmap
  - [ ] 撤销/重做正常
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: M2-T01（事件链数据结构）、M2-T02（区域规则数据结构）、M2-T03（胜利条件数据结构）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M2-T11: 全量校验与修复建议系统

- **负责**: editor-engineer
- **目标**: 实现编辑器的全面校验系统，覆盖所有编辑层，校验失败时提供修复建议
- **具体内容**:
  - 在 `slg-editor/src/validate.rs` 大幅扩展
  - `ValidatorRegistry` 结构：注册所有校验器
  - 校验器清单：
    - **连通性校验**（已有，增强）：Union-Find 检查各势力领地连通性，检测飞地
    - **资源平衡校验**（新增）：检查各出生点周围资源分布均衡度（方差 < 阈值）
    - **河流连续性校验**（新增）：河流 hex 是否形成连续路径，渡口位置是否合理
    - **实体重叠校验**（已有）：同一位置不允许两个城池
    - **出生点校验**（新增）：出生点数量匹配势力数，距离合理
    - **胜利条件校验**（新增）：胜利条件引用的格子/势力是否存在
    - **事件链校验**（新增）：事件触发条件引用的 ID 是否有效，事件链是否有死节点
    - **区域规则校验**（新增）：区域是否为空，效果参数是否合法
    - **关隘可达性校验**（新增）：关隘是否可从至少两个方向通行
  - 校验级别：
    - `Error`：阻止保存（实体重叠、连通性断裂、胜利条件引用无效）
    - `Warning`：可选修复（资源不均衡、河流断开、出生点过近）
    - `Info`：提示信息（区域规则覆盖重叠）
  - `FixSuggestion` 结构：
    - `description: String` — 修复描述
    - `action: FixAction` — 自动修复动作
    - `auto_applicable: bool` — 是否可一键应用
  - `FixAction` 枚举：
    - `AddFord { coord: HexCoord }` — 在断开处添加渡口
    - `RemoveEnclave { tiles: BTreeSet<TileKey> }` — 移除飞地
    - `MoveSpawnPoint { faction: FactionId, new_coord: HexCoord }` — 移动出生点
    - `ConnectRiver { from: HexCoord, to: HexCoord }` — 连接断开的河流
    - `BalanceResources { adjustments: BTreeMap<TileKey, u8> }` — 调整资源分布
  - 校验面板 UI（egui）：
    - 校验结果列表（按级别分组）
    - 点击问题项跳转到对应位置（相机平移）
    - 一键修复按钮（对 auto_applicable 项）
    - 修复后自动重新校验
  - 性能：轻量校验每笔操作后 <5ms，全量校验 <2s（256x256）
- **验收标准**:
  - [ ] 连通性校验正确检测飞地
  - [ ] 河流连续性校验正确检测断开
  - [ ] 胜利条件校验正确检测无效引用
  - [ ] 修复建议描述清晰可理解
  - [ ] 一键修复可自动应用
  - [ ] 修复后重新校验通过
  - [ ] 点击问题项跳转到对应位置
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: M2-T06（河流校验）、M2-T10（规则层校验需要规则数据结构）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M2-T12: 地图画廊后端

- **负责**: editor-engineer
- **目标**: 实现地图画廊的后端逻辑，支持地图元数据管理、标签、搜索、预览图生成
- **具体内容**:
  - 在 `slg-editor/src/` 新建 `gallery.rs`
  - `MapGalleryEntry` 结构：
    - `path: PathBuf` — 地图文件路径
    - `name: String` — 地图名称（来自 MapDocument meta）
    - `description: String` — 描述
    - `author: String` — 作者
    - `tags: Vec<String>` — 标签
    - `preview_path: PathBuf` — 预览图路径
    - `created_at: DateTime<Utc>` — 创建时间
    - `modified_at: DateTime<Utc>` — 修改时间
    - `map_size: (u32, u32)` — 地图尺寸
    - `faction_count: u8` — 势力数量
    - `has_custom_events: bool` — 是否有自定义事件
    - `has_custom_victory: bool` — 是否有自定义胜利条件
  - `MapGallery` 结构：
    - `entries: Vec<MapGalleryEntry>` — 所有地图条目
    - `scan_directory(dir)` — 扫描目录下所有 .slgmap 文件
    - `filter_by_tag(tag) -> Vec<&MapGalleryEntry>` — 按标签筛选
    - `filter_by_size(size) -> Vec<&MapGalleryEntry>` — 按尺寸筛选
    - `search(query) -> Vec<&MapGalleryEntry>` — 搜索名称/描述
    - `sort_by(sort_key)` — 排序（名称/日期/尺寸）
  - 预览图生成：
    - 加载 MapDocument -> 渲染缩略图 -> 保存为 PNG
    - 预览图尺寸 256x256
    - 自动在保存地图时生成预览图
  - 地图元数据读取：从 .slgmap 文件的 Meta section 读取
  - 标签管理：
    - 内置标签：`campaign` / `sandbox` / `small` / `large` / `custom_events` / `custom_victory`
    - 用户自定义标签
  - 用户地图目录：`user/maps/`
  - 内置地图目录：`assets/maps/`
- **验收标准**:
  - [ ] 扫描目录后正确列出所有 .slgmap 文件
  - [ ] 按标签筛选正确返回结果
  - [ ] 搜索功能可按名称匹配
  - [ ] 预览图在保存地图时自动生成
  - [ ] 地图元数据正确读取
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: M2-T05（预设格式参考）、M1 已有的 .slgmap 容器读写
- **复杂度**: 中

---

### 阶段 C：渲染与 UI 扩展（与阶段 B 并行）

---

#### M2-T13: 地形 Autotiling 系统

- **负责**: render-engineer
- **目标**: 实现六边形地形的 autotiling 过渡系统，相邻不同地形类型之间有平滑过渡纹理
- **具体内容**:
  - 在 `slg-engine/src/render/` 新建 `autotile.rs`
  - 过渡规则表（`TransitionRuleTable`）：
    - 定义任意两种地形类型之间的过渡方式
    - 过渡类型：`Hard`（硬切割）/ `Blend`（渐变混合）/ `Edge`（边缘装饰）
    - 优先级：水域 > 山地 > 森林 > 平原（高优先级地形"侵蚀"低优先级）
  - 六边形 autotiling 算法：
    - 对每个 hex，检查 6 个邻居的地形类型
    - 根据邻居类型组合选择过渡纹理变体
    - 六边形 pointy-top 的 6 条边，每条边独立判断过渡
    - 过渡纹理变体数量：每种地形对最多 64 种（2^6 边组合），实际用规则合并到 ~10 种
  - 过渡纹理生成：
    - 基础地形纹理 + 过渡遮罩
    - 遮罩基于邻居方向生成（边缘渐变）
    - Fragment shader 混合两张地形纹理
  - Chunk mesh 更新：
    - autotiling 结果编码进 mesh 的 UV/颜色数据
    - ChunkDirty 驱动增量更新
    - 限流：每帧最多 16 个 chunk 重建（复用 M1 LOD 限流机制）
  - 性能：autotiling 不影响 60 FPS
- **验收标准**:
  - [ ] 相邻不同地形之间有可见过渡（无硬切割）
  - [ ] 水域边缘有正确的岸线过渡
  - [ ] 山地边缘有坡度过渡
  - [ ] 过渡纹理在 LOD 切换时正确处理
  - [ ] 60 FPS 不受影响
  - [ ] `cargo test -p slg-engine` 通过
- **依赖**: 无（使用 M1 已有的 Chunk 渲染与地形数据）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M2-T14: 地图画廊 UI

- **负责**: render-engineer
- **目标**: 实现地图画廊的 egui 面板，支持浏览、筛选、预览地图
- **具体内容**:
  - 在 `slg-ui/src/panels/` 新建 `gallery.rs`
  - 画廊主面板（egui）：
    - 网格布局：每行 3~4 个地图卡片
    - 每个卡片：预览图 + 名称 + 作者 + 标签 + 尺寸 + 势力数
    - 鼠标悬停高亮，点击选中
  - 筛选面板：
    - 标签筛选（多选复选框）
    - 尺寸筛选（下拉：全部/小/中/大）
    - 搜索框（按名称/描述搜索）
    - 排序选项（名称/日期/尺寸）
  - 地图详情面板：
    - 大预览图
    - 完整描述
    - 标签列表
    - 地图信息（尺寸/势力数/是否有自定义事件/胜利条件）
    - "加载"按钮 -> 进入编辑器或直接开始游戏
    - "编辑"按钮 -> 进入编辑器模式
  - 新建地图面板：
    - 空白地图 / 程序化生成 / 导入文件
    - 生成预设选择（使用 M2-T05 的预设库）
    - 尺寸/种子/势力数配置
  - i18n：所有画廊文案走 fluent（zh-CN）
- **验收标准**:
  - [ ] 画廊正确显示所有可用地图
  - [ ] 标签筛选功能正常
  - [ ] 搜索功能正常
  - [ ] 点击地图卡片显示详情
  - [ ] "加载"按钮正确加载地图
  - [ ] 新建地图流程完整（空白/生成/导入）
  - [ ] 所有文案走 ftl，无硬编码
- **依赖**: M2-T12（画廊后端数据）
- **复杂度**: 中

---

#### M2-T15: 沙盒模式 UI

- **负责**: render-engineer
- **目标**: 实现沙盒模式的 UI 扩展，包含动态事件通知、沙盒特有 HUD 元素
- **具体内容**:
  - 在 `slg-ui/src/panels/` 新建 `event_notification.rs`
  - 动态事件通知面板：
    - 事件弹窗：显示事件名称、描述、可选分支
    - 分支选择按钮（如果有分支）
    - 自动消失（可配置延迟）或手动关闭
    - 事件历史记录（可回看已触发事件）
  - 沙盒模式 HUD 扩展：
    - 分数/目标进度显示（沙盒模式的目标系统）
    - 动态事件冷却指示器
    - 天气/季节指示器（如果实现）
  - 沙盒模式启动面板：
    - 选择地图（从画廊）
    - 选择生成预设或已有地图
    - 配置沙盒参数（动态事件频率/难度/目标类型）
    - "开始沙盒"按钮
  - 游戏结束面板：
    - 胜利/失败原因
    - 统计数据（存活天数/占领格数/战斗次数）
    - "再来一局" / "返回主菜单"按钮
  - i18n：所有沙盒文案走 fluent（zh-CN）
- **验收标准**:
  - [ ] 动态事件触发时显示通知弹窗
  - [ ] 分支选择可影响后续事件
  - [ ] 事件历史可回看
  - [ ] 沙盒启动面板可配置参数
  - [ ] 游戏结束面板显示正确统计
  - [ ] 所有文案走 ftl，无硬编码
- **依赖**: M2-T04（动态事件数据）、M2-T16（事件内容）
- **复杂度**: 中

---

### 阶段 D：沙盒模式（依赖阶段 A 事件链）

---

#### M2-T04: 沙盒动态事件系统

- **负责**: core-engineer
- **目标**: 实现沙盒模式下的动态事件触发与执行系统
- **具体内容**:
  - 在 `slg-core/src/event/` 新建 `sandbox.rs`
  - `SandboxEventConfig` 结构：
    - `event_frequency: EventFrequency` — 事件频率（低/中/高）
    - `enabled_categories: BTreeSet<EventCategory>` — 启用的事件类别
    - `difficulty_modifier: f64` — 难度修正
  - `EventCategory` 枚举：
    - `Disaster` — 天灾（旱灾/洪水/蝗灾/瘟疫）
    - `Rebellion` — 叛乱（领地叛乱/军队哗变）
    - `FamousGeneral` — 名将出世（在野武将出现）
    - `TradeRoute` — 商路事件（贸易机会/商路中断）
    - `Weather` — 天气事件（影响战斗/行军）
    - `Diplomatic` — 外交事件（势力间自动结盟/宣战）
  - `SandboxEventScheduler` 结构：
    - 管理事件触发调度
    - 基于 tick 的事件队列
    - 随机事件使用 ChaCha12Rng，种子 = hash(world_seed, event_index)
    - 冷却机制：同类事件间隔至少 N tick
    - 条件过滤：事件只在满足前提条件时触发（如旱灾只在夏季触发）
  - 事件效果集成：
    - 天灾：指定区域资源产出降低 N%，持续 M tick
    - 叛乱：指定区域出现敌对部队
    - 名将出世：在野武将出现在指定城池，可招募
    - 商路：临时资源加成或减益
  - 沙盒模式目标系统：
    - `SandboxGoal` 枚举：MaxScore / MaxTerritory / SurviveNDays / Custom
    - 分数计算：领地数 x 10 + 城池数 x 50 + 武将数 x 20 + 资源总量 / 100
  - 集成到 GameTickSchedule 的 TickEnd 阶段
- **验收标准**:
  - [ ] 天灾事件正确触发并降低资源产出
  - [ ] 叛乱事件正确生成敌对部队
  - [ ] 名将出世事件正确生成可招募武将
  - [ ] 事件冷却机制正确（同类事件不连续触发）
  - [ ] 随机事件确定性：同种子同 tick 序列完全相同
  - [ ] 分数计算正确
  - [ ] `cargo test -p slg-core` 通过
  - [ ] 无 bevy 依赖
- **依赖**: M2-T01（使用事件链引擎的触发与效果框架）
- **复杂度**: 中

---

### 阶段 E：内容制作（可与阶段 B/C/D 并行）

---

#### M2-T16: 沙盒事件内容

- **负责**: content-designer
- **目标**: 制作沙盒模式的动态事件数据表，覆盖天灾/叛乱/名将出世/商路/天气/外交六大类
- **具体内容**:
  - 扩展 `assets/data/events.ron`，新增沙盒事件定义：
  - **天灾类**（8~10 个事件）：
    - 旱灾：平原区域资源产出 -30%，持续 50 tick
    - 洪水：河流附近区域通行困难，持续 30 tick
    - 蝗灾：农田产出 -50%，持续 40 tick
    - 瘟疫：城池人口减少，持续 60 tick
    - 地震：随机建筑损坏
    - 每个事件：触发条件（季节/区域类型）、效果、文案
  - **叛乱类**（5~8 个事件）：
    - 领地叛乱：占领区出现敌对部队（兵力与占领时间负相关）
    - 军队哗变：部队士气大幅下降
    - 流民起义：低等级区域出现中立部队
    - 每个事件：触发条件（民心/占领时间/资源状况）、效果
  - **名将出世类**（10~15 个事件）：
    - 三国名将出现在特定城池（诸葛亮/周瑜/吕布等）
    - 隐士出山：高属性武将在偏远地区出现
    - 降将归顺：被消灭势力的武将可被招募
    - 每个事件：出现位置/属性/可招募条件
  - **商路类**（5~8 个事件）：
    - 商路繁荣：资源产出 +20%，持续 30 tick
    - 商路中断：贸易收入归零，持续 20 tick
    - 外商来访：可购买特殊资源
  - **天气类**（5~8 个事件）：
    - 暴风雪：行军速度 -50%，战斗攻击力 -20%
    - 酷暑：部队消耗粮食 +50%
    - 大雾：视野范围 -50%
  - **外交类**（5~8 个事件）：
    - 势力联盟：两个 AI 势力自动结盟
    - 背叛：盟友突然宣战
    - 和平使者：第三方势力调停
  - 所有事件文案走 i18n（zh-CN）
  - 创建 `project/sandbox_events.md`：事件设计说明文档
- **验收标准**:
  - [ ] 每个类别至少 5 个事件定义
  - [ ] 所有事件 RON 可被 slg-assets 正确解析
  - [ ] 事件触发条件引用的 ID 全部有效
  - [ ] 事件文案走 ftl，零硬编码
  - [ ] 事件数值参考 `project/numbers.md` 基准
- **依赖**: M2-T01（事件链数据结构定义）、M2-T04（动态事件系统框架）
- **复杂度**: 中

---

#### M2-T17: 剧本扩展内容（事件链+胜利条件+区域规则）

- **负责**: content-designer
- **目标**: 为"三国鼎立"剧本补充完整的事件链、自定义胜利条件与区域规则
- **具体内容**:
  - 扩展 `assets/data/scenarios/sanguo_dl/events.ron`：
    - 黄巾余党事件链（5~8 个节点）：黄巾残部出现 -> 玩家/AI 剿灭 -> 奖励/惩罚
    - 天命事件链：各势力争夺天命值，影响外交与事件
    - 名将投靠事件链：特定条件下武将主动投靠
    - 势力覆灭事件链：某势力被消灭后的连锁反应（散兵/投降/复仇）
  - 扩展 `assets/data/scenarios/sanguo_dl/scenario.ron`：
    - 自定义胜利条件：
      - 占领洛阳（指定区域）
      - 统一全部州（占领所有城池）
      - 存活 365 天
      - 消灭所有敌对势力
    - 区域规则：
      - 中原地区：资源产出 1.2x，移动代价 0.9x（富饶平原）
      - 西南山区：资源产出 0.8x，移动代价 1.5x，防御 +20%
      - 江东水乡：河流密集，渡口关键
      - 关中地区：防御 +30%（函谷关等关隘集中）
  - 新增 `assets/data/scenarios/sanguo_dl/zones.ron`：区域规则定义
  - 新增 `assets/data/scenarios/sanguo_dl/victory.ron`：胜利条件定义
  - 扩展 i18n 文案：事件描述/分支选项/胜利条件文案
  - 更新 `project/numbers.md`：新增区域规则数值说明
- **验收标准**:
  - [ ] 至少 3 条完整事件链（5+ 节点）
  - [ ] 至少 4 种胜利条件
  - [ ] 至少 4 个区域规则定义
  - [ ] 所有 RON 文件可被正确解析
  - [ ] 引用 ID 全部有效
  - [ ] 文案走 ftl，零硬编码
- **依赖**: M2-T01（事件链结构）、M2-T02（区域规则结构）、M2-T03（胜利条件结构）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M2-T18: 地形过渡美术数据

- **负责**: content-designer
- **目标**: 定义地形过渡规则表与视觉配置数据
- **具体内容**:
  - 新建 `assets/data/terrain_transitions.ron`：
    - 过渡规则表：每对地形类型的过渡方式（Hard/Blend/Edge）
    - 优先级：水域 > 山地 > 森林 > 沙漠 > 沼泽 > 丘陵 > 平原 > 关隘
    - 过渡参数：渐变宽度、混合系数、边缘装饰类型
  - 扩展 `terrain_types.ron`：
    - 每种地形新增 `transition_color: [f32; 3]` — 过渡混合色
    - 每种地形新增 `edge_texture: String` — 边缘装饰纹理 ID
  - 地形视觉风格定义：
    - 色彩方案：每种地形的主色/辅色/高光色
    - 纹理变体：每种地形 2~3 种纹理变体（避免重复感）
  - 验证过渡规则完整性：所有地形对都有定义
  - i18n：地形名称文案
- **验收标准**:
  - [ ] 所有地形对（8x8=64）都有过渡规则定义
  - [ ] 过渡优先级无循环依赖
  - [ ] RON 文件可被正确解析
  - [ ] 色彩方案视觉协调
- **依赖**: 无（使用 M1 已有的 terrain_types.ron 结构）
- **复杂度**: 低

---

### 阶段 F：集成与验证（依赖全部前置任务）

---

#### M2-T19: 全链路集成

- **负责**: render-engineer + core-engineer + editor-engineer（协作）
- **目标**: 将 M2 所有子系统串联，实现从编辑到游玩的完整 UGC 流程
- **具体内容**:
  - **slg-app 模式扩展**（render-engineer）：
    - 新增 `SandboxMode` Resource：沙盒模式配置与状态
    - 沙盒模式启动流程：选择地图/预设 -> 配置参数 -> 初始化事件调度器 -> 开始
    - 模式切换：主菜单 <-> 画廊 <-> 编辑器 <-> 沙盒游玩 <-> 剧本游玩
  - **事件链集成**（core-engineer）：
    - 事件链引擎集成到 GameTickSchedule TickEnd 阶段
    - 事件效果执行：资源/部队/武将/外交/地形修改
    - 事件 UI 触发：事件发生时发送 GameEvent，渲染层订阅显示通知
  - **规则层集成**（core-engineer）：
    - 区域规则加载：从 MapDocument RuleLayer 读取区域定义
    - 区域规则集成到经济/寻路/战斗系统
    - 胜利条件评估：每 tick 检查，触发游戏结束
  - **编辑器完整流程**（editor-engineer）：
    - 编辑器全流程：新建/生成 -> 编辑地形/河流 -> 定义区域 -> 配置事件 -> 设置胜利条件 -> 校验 -> 保存
    - 保存时自动嵌入预览图
    - 保存后自动注册到画廊
  - **autotiling 集成**（render-engineer）：
    - 地形变更时自动触发 autotiling 重算
    - 过渡规则从数据表加载
  - **i18n 完整覆盖**：
    - 所有 M2 新增 UI 文案走 fluent
    - 事件描述/分支选项走 fluent
  - 端到端验证：
    - 创建自定义剧本（含事件链+胜利条件+区域规则）-> 保存 -> 在画廊中看到 -> 加载游玩 -> 事件触发 -> 达成胜利条件
- **验收标准**:
  - [ ] 沙盒模式可启动，动态事件正常触发
  - [ ] 编辑器可创建含规则层的完整地图
  - [ ] 事件链在游玩中正确触发与执行
  - [ ] 区域规则影响经济/寻路/战斗
  - [ ] 胜利条件正确判定游戏结束
  - [ ] autotiling 在地形变更后正确更新
  - [ ] 端到端流程无 panic
  - [ ] `cargo test --workspace` 通过
- **依赖**: M2-T01~T18 全部
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M2-T20: 集成测试与 M2 验收

- **负责**: qa-engineer
- **目标**: 编写 M2 集成测试、运行性能基准、试玩验证全部 M2 验收标准
- **具体内容**:
  - 集成测试（tests/）：
    - `test_event_chain_trigger`：事件链在正确 tick 触发
    - `test_event_chain_branch`：事件链分支选择正确影响后续
    - `test_zone_rule_resource`：区域资源倍率正确生效
    - `test_zone_rule_movement`：区域移动代价正确生效
    - `test_victory_condition_occupy`：占领胜利条件正确判定
    - `test_victory_condition_survive`：存活胜利条件正确判定
    - `test_sandbox_event_trigger`：沙盒动态事件正确触发
    - `test_sandbox_determinism`：同种子沙盒事件序列完全相同
    - `test_river_continuity_validation`：河流断开被正确检测
    - `test_full_validation`：全量校验正确检测各类问题
    - `test_fix_suggestion`：修复建议可正确应用
    - `test_preset_roundtrip`：预设导出->导入内容一致
    - `test_autotile_consistency`：autotiling 结果确定性
    - `test_map_gallery_scan`：画廊正确扫描地图文件
    - `test_end_to_end_ugc`：创建剧本->保存->加载->游玩完整流程
  - 性能基准（criterion）：
    - `bench_event_chain_eval`：事件链评估耗时
    - `bench_zone_rule_apply`：区域规则应用耗时
    - `bench_full_validation`：全量校验耗时（目标 < 2s）
    - `bench_autotile_rebuild`：autotiling chunk 重建耗时
  - CI 更新：
    - 新增 M2 测试到 CI 矩阵
    - 基准回归检查更新
  - 代码审查：
    - 随机抽查 5 个 M2 新增文件
    - 检查注释完整性、命名规范、错误处理
  - 试玩验收：
    - 按 AC-1 ~ AC-16 逐项验证
    - 输出缺陷清单（Blocker / Warning / Suggestion）
    - 输出 M2 KPI 报告
- **验收标准**:
  - [ ] 所有集成测试通过
  - [ ] 性能基准基线入库
  - [ ] CI 全绿
  - [ ] 代码审查无 Blocker
  - [ ] 试玩验收 AC-1 ~ AC-16 全部通过
  - [ ] 输出完整 M2 验收报告
- **依赖**: M2-T01~T19 全部
- **复杂度**: 中

---

## 4. 执行顺序

### 4.1 可并行的任务组

```
并行组 1（立即启动，无依赖）:
  M2-T01  事件链引擎              [core-engineer]
  M2-T02  区域规则引擎            [core-engineer]   （与 T01 同 agent，可交替进行）
  M2-T03  胜利条件引擎            [core-engineer]   （与 T01/T02 同 agent，可交替进行）
  M2-T05  生成预设导入导出        [core-engineer]   （低复杂度，穿插进行）
  M2-T06  河流编辑工具            [editor-engineer]
  M2-T07  选区与区域操作          [editor-engineer]  （与 T06 同 agent，可交替进行）
  M2-T08  图层管理                [editor-engineer]  （低复杂度，穿插进行）
  M2-T13  地形 autotiling         [render-engineer]
  M2-T18  地形过渡美术数据        [content-designer]

并行组 2（依赖并行组 1 部分完成）:
  M2-T04  沙盒动态事件            [core-engineer]    （依赖 T01）
  M2-T09  Stamp 模板库            [editor-engineer]  （依赖 T07）
  M2-T10  规则层编辑器            [editor-engineer]  （依赖 T01, T02, T03）
  M2-T16  沙盒事件内容            [content-designer]  （依赖 T01, T04）

并行组 3（依赖并行组 2 部分完成）:
  M2-T11  全量校验+修复建议       [editor-engineer]  （依赖 T06, T10）
  M2-T12  地图画廊后端            [editor-engineer]  （依赖 T05）
  M2-T14  地图画廊 UI             [render-engineer]  （依赖 T12）
  M2-T15  沙盒模式 UI             [render-engineer]  （依赖 T04, T16）
  M2-T17  剧本扩展内容            [content-designer]  （依赖 T01, T02, T03）

并行组 4（依赖全部前置）:
  M2-T19  全链路集成              [render + core + editor]
  M2-T20  集成测试与验收          [qa-engineer]      （依赖 T19）
```

### 4.2 必须串行的依赖链

**链 1：事件链 -> 规则层 -> 校验**（core + editor 主线，关键路径）

```
T01 (事件链引擎)
 -> T10 (规则层编辑器，依赖 T01+T02+T03)
 -> T11 (全量校验，依赖 T06+T10)
 -> T19 (集成)
 -> T20 (验收)
```

**链 2：选区 -> Stamp**（editor 独立链）

```
T07 (选区操作)
 -> T09 (Stamp 模板库)
 -> T19 (集成)
```

**链 3：事件链 -> 沙盒 -> 沙盒 UI**（core + render 链）

```
T01 (事件链引擎)
 -> T04 (沙盒动态事件)
 -> T15 (沙盒模式 UI)
 -> T19 (集成)
```

**链 4：autotiling 独立链**（render 独立）

```
T13 (地形 autotiling)
 -> T19 (集成)
```

**链 5：画廊链**（editor + render）

```
T05 (预设导入导出)
 -> T12 (画廊后端)
 -> T14 (画廊 UI)
 -> T19 (集成)
```

**链 6：内容链**（content-designer，独立于逻辑链）

```
T18 (地形过渡美术数据) -> T19
T16 (沙盒事件内容) -> T19
T17 (剧本扩展内容) -> T19
```

### 4.3 关键路径分析

**关键路径**（决定 M2 最短完成时间）：

```
T01 (事件链引擎，2~3 会话)
 -> T10 (规则层编辑器，2~3 会话)
 -> T11 (全量校验，2 会话)
 -> T19 (全链路集成，2~3 会话)
 -> T20 (测试验收，1 会话)

总计：9~12 个会话
```

**并行路径 A**（editor 独立工具，可与关键路径并行）：

```
T06 (河流编辑，2 会话) + T07 (选区操作，1 会话) + T08 (图层管理，0.5 会话)
 -> T09 (Stamp 模板，1 会话)
总计：4.5 会话（并行完成后汇入 T19）
```

**并行路径 B**（render 独立，可与关键路径并行）：

```
T13 (autotiling，2 会话)
 -> T19 (汇入集成)
总计：2 会话
```

**并行路径 C**（内容，完全独立）：

```
T16 (沙盒事件内容，1.5 会话) + T17 (剧本扩展，2 会话) + T18 (过渡美术数据，0.5 会话)
总计：4 会话（可并行，完成后汇入 T19）
```

### 4.4 core-engineer 任务排序

core-engineer 负担较重（T01/T02/T03/T04/T05 共 5 张卡），建议排序：

```
Session 1:  T01 (事件链引擎) - 开始
Session 2:  T01 续 + T02 (区域规则引擎)
Session 3:  T02 续 + T03 (胜利条件引擎)
Session 4:  T03 续 + T05 (生成预设导入导出)
Session 5:  T04 (沙盒动态事件)
Session 6:  T04 续
```

### 4.5 editor-engineer 任务排序

editor-engineer 负担最重（T06/T07/T08/T09/T10/T11/T12 共 7 张卡），建议排序：

```
Session 1:  T06 (河流编辑) + T08 (图层管理，低复杂度)
Session 2:  T06 续 + T07 (选区操作)
Session 3:  T07 续
Session 4:  T09 (Stamp 模板库)
Session 5:  T10 (规则层编辑器) - 等待 core T01/T02/T03 完成
Session 6:  T10 续
Session 7:  T11 (全量校验)
Session 8:  T11 续 + T12 (画廊后端)
```

### 4.6 render-engineer 任务排序

```
Session 1:  T13 (地形 autotiling)
Session 2:  T13 续
Session 3:  T14 (画廊 UI) - 等待 editor T12 完成
Session 4:  T15 (沙盒模式 UI) - 等待 core T04 + content T16 完成
Session 5:  T19 (集成，与 core + editor 协作)
```

### 4.7 content-designer 任务排序

```
Session 1:  T18 (地形过渡美术数据)
Session 2:  T16 (沙盒事件内容) - 等待 core T01/T04 完成
Session 3:  T16 续 + T17 (剧本扩展内容) - 等待 core T01/T02/T03 完成
Session 4:  T17 续
```

---

## 5. 跨 agent 协作节点

### 5.1 协作节点清单

| # | 节点 | 参与方 | 协作内容 | 触发时机 |
|---|------|--------|---------|---------|
| C1 | 事件链数据结构评审 | core -> editor, content | T01 定义的 EventChainDef/TriggerCondition/EventEffect 影响 T10 规则层编辑器与 T16/T17 内容制作 | T01 完成后 |
| C2 | 区域规则接口对齐 | core -> editor, render | T02 定义的 ZoneRule 结构影响 T10 编辑器 UI 与经济/寻路/战斗系统集成 | T02 完成后 |
| C3 | 胜利条件接口对齐 | core -> editor, render | T03 定义的 VictoryCondition 影响 T10 编辑器配置 UI 与 T15 游戏结束面板 | T03 完成后 |
| C4 | 规则层数据格式对齐 | editor -> core | T10 规则层编辑器产出的 MapDocument RuleLayer 格式需 core 引擎正确解析 | T10 期间 |
| C5 | autotiling 数据格式 | content -> render | T18 定义的过渡规则表格式需 render autotiling 系统正确读取 | T13 + T18 期间 |
| C6 | 事件 UI 数据流 | core -> render | 事件触发时 core 发送 GameEvent，render 订阅显示通知 | T04 + T15 期间 |
| C7 | 画廊数据格式 | editor -> render | T12 定义的 MapGalleryEntry 结构需 render 画廊 UI 正确消费 | T12 + T14 期间 |
| C8 | 校验与修复集成 | editor -> core | T11 校验器可能调用 core 的连通性/平衡性检查函数 | T11 期间 |
| C9 | 端到端 UGC 流程 | editor + core + render | 创建剧本 -> 保存 -> 加载 -> 游玩的完整流程需要三方协作 | T19 期间 |

### 5.2 协作协议

- 每个协作节点，发起方在完成报告中明确标注"下游影响"与"接口签名"
- 接收方在开始任务前先读取上游的接口定义
- 如有接口分歧，由 arch-guardian 依据 ARCHITECTURE.md 裁决
- master-coordinator 在协作节点完成后更新 PROGRESS.md
- 事件链/区域规则/胜利条件的数据结构设计（T01/T02/T03）需经 arch-guardian 评审确认后再进入下游任务

---

## 6. 风险与缓解

### 6.1 M2 层面风险

| # | 风险 | 影响 | 概率 | 缓解措施 |
|---|------|------|------|---------|
| MR1 | editor-engineer 负载过重（7 张卡） | 关键路径延长 | 高 | T06/T07/T08 低复杂度卡可快速完成；T10/T11 是关键路径核心，优先保障；T12 可后置 |
| MR2 | 事件链数据结构设计不合理导致下游大量返工 | 全局影响 | 中 | T01 完成后安排 arch-guardian 评审 + editor/content 确认；先写接口再填实现 |
| MR3 | 规则层编辑器复杂度超预期 | T10 延期 | 中 | T10 先实现最小可用（仅区域规则配置），事件链可视化/胜利条件编辑迭代补充 |
| MR4 | autotiling 性能不达标 | 60 FPS 不保 | 中 | T13 先用简单过渡（Hard/Blend 两种），复杂过渡后置；复用 M1 LOD 限流机制 |
| MR5 | 全量校验覆盖不全导致 UGC 质量不可控 | UGC 目标落空 | 中 | T11 先覆盖核心校验（连通性/实体重叠/河流连续），扩展校验迭代补充 |
| MR6 | 沙盒动态事件数值不平衡 | 体验差 | 中 | T16 事件数值先用保守值，试玩后迭代；事件频率可配置 |
| MR7 | 地图画廊与 Steam 创意工坊的格式不兼容 | V3 迁移成本 | 低 | 画廊条目格式保持与 .slgmap 容器解耦，V3 只需添加工坊 ID 字段 |
| MR8 | 事件链/区域规则/胜利条件三个新系统同时开发导致接口不一致 | 集成困难 | 中 | T01/T02/T03 由同一 agent（core-engineer）串行开发，保证内部一致性；完成后统一评审 |

### 6.2 性能预算检查点

| 检查点 | 时机 | 指标 | 不达标处理 |
|--------|------|------|-----------|
| 事件链评估 | T04 完成 | 100 条事件链 < 1ms/tick | 减少每 tick 评估的事件数量（分帧） |
| 区域规则应用 | T02 完成 | 不影响 tick 预算 | 缓存区域规则查询结果 |
| 全量校验 | T11 完成 | 256x256 < 2s | 分步校验 + 进度条 |
| autotiling | T13 完成 | 不影响 60 FPS | 复用 LOD 限流，过渡纹理预计算 |
| 沙盒事件调度 | T04 完成 | < 0.1ms/tick | 事件队列预排，减少条件检查 |

### 6.3 回退策略

如果 M2 某个子系统延期导致无法在计划内完成全部验收标准：

1. **规则层编辑器延期**：M2 仅支持通过 RON 文件手动定义规则，编辑器 UI 推迟到 M2.5
2. **autotiling 延期**：保留 M1 的纯色地形渲染，过渡美术推迟到 M2.5
3. **沙盒模式延期**：M2 仅验证剧本模式的事件链+胜利条件，沙盒动态事件推迟到 M2.5
4. **地图画廊延期**：用简单文件列表替代画廊 UI，功能推迟到 M2.5
5. **全量校验延期**：保留 M1 基础校验（实体重叠+连通性），扩展校验推迟到 M2.5

**降级优先级**（按对 UGC 目标的影响排序，不可降级项标 *）：

1. *事件链引擎（T01）— 剧本系统核心，不可降级
2. *胜利条件引擎（T03）— 剧本系统核心，不可降级
3. *规则层编辑器（T10）— UGC 创作入口，降级为 RON 手动编辑
4. 区域规则引擎（T02）— 可简化为仅资源倍率
5. 全量校验（T11）— 可保留基础校验
6. 河流编辑（T06）— 可推迟
7. autotiling（T13）— 可推迟
8. 沙盒模式（T04/T15）— 可推迟
9. 地图画廊（T12/T14）— 可简化

---

## 附录 A：任务卡总表

| 卡号 | 标题 | 负责 | 依赖 | 复杂度 | 阶段 |
|------|------|------|------|--------|------|
| M2-T01 | 事件链引擎 | core-engineer | 无 | 高 | A |
| M2-T02 | 区域规则引擎 | core-engineer | 无 | 中 | A |
| M2-T03 | 胜利条件引擎 | core-engineer | 无 | 中 | A |
| M2-T04 | 沙盒动态事件系统 | core-engineer | T01 | 中 | D |
| M2-T05 | 生成预设导入导出 | core-engineer | 无 | 低 | A |
| M2-T06 | 河流编辑工具 | editor-engineer | 无 | 高 | B |
| M2-T07 | 选区与区域操作 | editor-engineer | 无 | 中 | B |
| M2-T08 | 图层管理 | editor-engineer | 无 | 低 | B |
| M2-T09 | Stamp 模板库 | editor-engineer | T07 | 中 | B |
| M2-T10 | 规则层编辑器 | editor-engineer | T01, T02, T03 | 高 | B |
| M2-T11 | 全量校验与修复建议 | editor-engineer | T06, T10 | 高 | B |
| M2-T12 | 地图画廊后端 | editor-engineer | T05 | 中 | B |
| M2-T13 | 地形 Autotiling | render-engineer | 无 | 高 | C |
| M2-T14 | 地图画廊 UI | render-engineer | T12 | 中 | C |
| M2-T15 | 沙盒模式 UI | render-engineer | T04, T16 | 中 | C |
| M2-T16 | 沙盒事件内容 | content-designer | T01, T04 | 中 | E |
| M2-T17 | 剧本扩展内容 | content-designer | T01, T02, T03 | 高 | E |
| M2-T18 | 地形过渡美术数据 | content-designer | 无 | 低 | E |
| M2-T19 | 全链路集成 | render+core+editor | T01~T18 | 高 | F |
| M2-T20 | 集成测试与验收 | qa-engineer | T19 | 中 | F |

**统计**：
- 总任务卡：20 张
- core-engineer：5 张（T01/T02/T03/T04/T05）
- editor-engineer：7 张（T06/T07/T08/T09/T10/T11/T12）
- render-engineer：3 张（T13/T14/T15）
- content-designer：3 张（T16/T17/T18）
- qa-engineer：1 张（T20）
- 协作任务：1 张（T19，render+core+editor）
- arch-guardian：审查角色（在 T01/T02/T03 完成后参与数据结构评审）

---

## 附录 B：M2 新增数据结构快速参考

| 数据结构 | 定义位置 | 消费方 |
|----------|----------|--------|
| EventChainDef | slg-data (RON) + slg-core/src/event/chain.rs | core 事件引擎, editor 规则层编辑器, content 事件内容 |
| TriggerCondition | slg-core/src/event/trigger.rs | core 事件引擎 |
| EventEffect | slg-core/src/event/effect.rs | core 事件引擎 |
| ZoneRuleDef | slg-data (RON) + slg-core/src/rule/zone_rule.rs | core 经济/寻路/战斗, editor 规则层编辑器, content 区域规则 |
| VictoryCondition | slg-core/src/rule/victory.rs | core 胜利评估, editor 规则层编辑器, content 胜利条件 |
| SandboxEventConfig | slg-core/src/event/sandbox.rs | core 沙盒调度, render 沙盒 UI |
| GenerationPreset (扩展) | slg-core/src/gen/preset.rs | core 生成, editor 画廊, render 新建地图 |
| StampTemplate | slg-editor/src/tool/stamp.rs | editor Stamp 工具 |
| LayerState | slg-editor/src/layer_panel.rs | editor 图层管理, render 渲染过滤 |
| MapGalleryEntry | slg-editor/src/gallery.rs | editor 画廊后端, render 画廊 UI |
| TransitionRuleTable | assets/data/terrain_transitions.ron | render autotiling, content 过渡数据 |
