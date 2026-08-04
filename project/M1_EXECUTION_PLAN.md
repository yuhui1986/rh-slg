# M1 执行计划：V1 核心循环

> 版本：v1.0（2026-08-02）
> 基线：ARCHITECTURE.md v1.4
> 前置：M0 已完成（workspace 8 crate / CI / slg-data 类型 / slg-core hex / RON 占位 / 空窗口）

---

## 1. M1 总体目标与验收标准

### 1.1 一句话目标

**从空壳到能玩**：生成一张 256x256 地图，玩家占领格子、积累资源、触发战斗、查看战报，5 个 AI 势力正常决策，100 tick 无崩溃。

### 1.2 验收标准

| # | 标准 | 验证方法 |
|---|------|----------|
| AC-1 | 256x256 地图生成 < 5s，无 panic | `cargo test` + 手动计时 |
| AC-2 | 玩家可通过 UI 占领与己方领地相邻的格子 | 试玩验证 |
| AC-3 | 资源（金/粮/木/铁/石）每 tick 正确产出与消耗 | 单测 + 试玩 |
| AC-4 | 部队行军沿 hex 路径推进，到达后触发战斗 | 试玩验证 |
| AC-5 | 战斗纯函数 `simulate` 确定性：同种子 1000 次同战报 | proptest |
| AC-6 | 战报 UI 可查看回合详情与结果 | 试玩验证 |
| AC-7 | 5 个 AI 势力正常决策（占地/征兵/行军/外交），不卡死 | 100 tick 推演 |
| AC-8 | 迷雾系统正确：未探索区域黑、已探索半暗、视野内透明 | 试玩验证 |
| AC-9 | 100 tick 推演无 panic、无无限循环 | `cargo test` 集成测试 |
| AC-10 | `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test --workspace` 全绿 | CI |
| AC-11 | `grep -r "bevy" crates/slg-core crates/slg-data` 为空 | CI |
| AC-12 | 正常游玩 60 FPS（无大量实体时） | 手动验证 |
| AC-13 | 编辑器可 Paint 地形、FloodFill、撤销重做 | 试玩验证 |

### 1.3 "能玩"定义

玩家可以：启动程序 -> 新游戏（三国鼎立剧本）-> 在 256x256 地图上看到自己的势力 -> 点击格子发起占领 -> 等待资源积累 -> 征兵 -> 派遣部队行军 -> 与敌方部队交战 -> 查看战报 -> AI 势力自主行动 -> 持续 100 tick 不崩溃。

---

## 2. 参与 Agent 列表及职责

| Agent | 职责范围 | M1 主要交付 |
|-------|---------|------------|
| **core-engineer** | slg-core + slg-data（纯逻辑，零 Bevy 依赖） | ECS 组件、时钟、领地、经济、行军、战斗、AI、生成 |
| **render-engineer** | slg-engine + slg-ui（Bevy 渲染 + egui HUD） | Chunk 渲染、LOD、相机、拾取、迷雾、tick_dispatcher、HUD 面板 |
| **editor-engineer** | slg-editor + slg-save（编辑器 + 存档容器） | 命令栈、笔刷工具、校验器、存档读写 |
| **content-designer** | assets/data + assets/i18n（RON 数据表 + 文案） | 完整武将/战法/兵种表、剧本定义、i18n 文案 |
| **qa-engineer** | tests/ + CI + 基准（质量门禁） | 集成测试、确定性验证、性能基准、试玩验收 |
| **arch-guardian** | ARCHITECTURE.md 维护 + 红线巡检 | 架构审查、依赖方向检查、确定性纪律巡检 |

### 跨 crate 边界红线

- core-engineer **禁止**引入 bevy/egui 依赖
- render-engineer **禁止**在渲染层写游戏规则
- editor-engineer **禁止**私改 slg-data 字段结构
- content-designer **禁止**改逻辑或引擎代码

---

## 3. 任务卡清单

### 阶段 A：基础数据结构与核心系统（无外部依赖，可并行启动）

---

#### M1-T01: ECS 组件与游戏 Resource 定义

- **负责**: core-engineer
- **目标**: 定义所有运行时 ECS 组件与全局 Resource，为后续系统提供类型基础
- **具体内容**:
  - 在 `slg-core/src/entity/` 新建模块：`general.rs`、`army.rs`、`city.rs`、`faction.rs`
  - 定义 ECS Component 结构（纯数据，无 Bevy 依赖）：
    - `GeneralStats`（五维/等级/经验）、`GeneralSkills`（战法列表）、`GeneralTroopType`
    - `ArmyTroops`（兵种/数量/士气）、`ArmyMarch`（path_index/arrive_tick/path）、`ArmyPosition`（HexCoord）
    - `CityLevel`（1~10）、`CityGarrison`、`CityBuildQueue`
    - `OwnerFaction`（FactionId）
  - 定义 Chunk Component 结构（对应 §6.8 映射表）：
    - `TileTerrain([TerrainType; 1024])`、`TileOwner([u8; 1024])`、`TileLevel([u8; 1024])`
    - `TileResource([Option<ResourceType>; 1024])`、`ChunkDirty(bool)`
  - 定义全局 Resource 结构：
    - `GameClock { current_tick: u64, speed: Speed, accumulator: f64 }`
    - `FactionStore { factions: BTreeMap<FactionId, FactionState> }`
    - `CommandQueue { commands: VecDeque<PlayerCommand> }`
    - `PathCache { entries: LruCache<PathCacheKey, Vec<HexCoord>> }`
    - `CombatReportStore { reports: Vec<CombatReport> }`
    - `FogOfWar { chunks: Vec<FogChunk> }`（FogChunk = [u8; 1024]）
    - `AISlotAssignments { slots: [FactionId; 10] }`
    - `TerritoryGraph { union_finds: BTreeMap<FactionId, UnionFind>, owner_map: BTreeMap<TileKey, FactionId> }`
  - 为 `FactionState` 扩展字段：resources、diplomacy、personality tags、union-find 根节点
  - 定义 `PlayerCommand` 枚举：MoveArmy / OccupyTile / BuildBuilding / RecruitTroops / DiplomacyAction
  - 定义 `Speed` 枚举：Paused / x1 / x2 / x3
  - 定义 `CombatInput`、`CombatSide`、`CombatReport` 结构
- **验收标准**:
  - [ ] `cargo test -p slg-core -p slg-data` 通过
  - [ ] `cargo clippy -- -D warnings` 通过
  - [ ] 所有结构有 `#[derive(Debug, Clone, Serialize, Deserialize)]`
  - [ ] 无 bevy 依赖（CI 红线检查通过）
  - [ ] 公共 API 有文档注释
- **依赖**: 无（M0 的 ids.rs / grid.rs / tile.rs / config.rs 已就绪）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M1-T02: 数据表扩展与完整三国内容

- **负责**: content-designer
- **目标**: 将占位 RON 数据表扩展为完整可用的游戏数据，覆盖三国鼎立剧本所需全部内容
- **具体内容**:
  - 扩展 `generals.ron`：添加 30+ 名武将（魏/蜀/吴/群雄各阵营核心武将），包含完整五维、自带战法、可学习战法、兵种适性
  - 扩展 `skills.ron`：添加 20+ 战法（主动/被动/指挥/突击各类型），包含发动率、目标策略、伤害公式、效果列表
  - 扩展 `unit_types.ron`：补全三大兵种的完整属性（骑/弓/步），确认克制三角 ±15%
  - 扩展 `buildings.ron`：完善农田/兵营/仓库/城墙/烽火台等建筑的多级属性
  - 扩展 `terrain_types.ron`：补全 8 种地形（平原/山地/水域/森林/沙漠/沼泽/丘陵/关隘）的完整属性
  - 扩展 `global_params.ron`：调整经济/军事倍率达到可玩基准值
  - 新建 `events.ron` 扩展：开局事件、随机事件、胜利条件事件
  - 创建 `assets/data/scenarios/sanguo_dl/scenario.ron`：三国鼎立剧本定义
    - 5 个 AI 势力：魏（曹操·扩张）、蜀（刘备·外交）、吴（孙权·防御）、辽东（公孙渊·投机）、南中（孟获·保守）
    - 每个势力：初始城池位置、初始领土、初始资源、初始军队、人格标签
    - 玩家独立势力配置（自建君主名、一州起步）
    - 胜利条件：占领洛阳 / 统一全部州 / 存活 365 天
    - 初始外交关系矩阵
  - 创建 `assets/data/scenarios/sanguo_dl/events.ron`：完整事件链
    - 开局事件、黄巾余党事件、天灾事件、名将出世事件、势力覆灭事件
  - 创建 `assets/data/scenarios/sanguo_dl/diplomacy_lines.ron`：外交台词
    - 结盟/宣战/停战/送礼/威胁各场景的势力专属台词
  - 扩展 `assets/i18n/zh-CN/main.ftl`：覆盖所有新增 UI 文案
  - 创建 `project/numbers.md`：数值平衡设计说明文档
- **验收标准**:
  - [ ] 所有 RON 文件可被 `slg-assets::load_all` 正确解析（无语法错误）
  - [ ] 引用 ID 全部存在（战法引用武将、武将引用战法和兵种、建筑引用地形等交叉引用正确）
  - [ ] 兵种克制三角 ±15% 数值正确
  - [ ] 5 个 AI 势力各有独立人格标签
  - [ ] 胜利条件至少 2 种
  - [ ] i18n ftl key 齐全，零硬编码文案
- **依赖**: 无（数据表结构已在 M0 定义，content-designer 可独立工作）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M1-T03: slg-assets 数据加载完整实现

- **负责**: core-engineer
- **目标**: 将 slg-assets 从空壳实现为完整的 RON 数据表加载与校验系统
- **具体内容**:
  - 实现 `load_all(data_dir)` 函数：遍历目录下所有 `.ron` 文件，按文件名映射到对应结构体反序列化
  - `DataStore` 结构填充：`generals: BTreeMap<GeneralId, GeneralDef>`、`skills`、`unit_types`、`terrain_types`、`buildings`、`events`、`global_params`
  - 实现交叉引用校验：武将引用的战法/兵种 ID 必须存在
  - 实现 `load_scenario(path)` 函数：加载剧本 RON 文件
  - 实现 `merge_mods` 基础逻辑：按 priority 排序覆盖
  - 添加错误类型：`AssetError` 增加 `InvalidReference` / `SchemaMismatch` 变体
- **验收标准**:
  - [ ] `load_all("assets/data")` 成功加载所有 7 个 RON 文件
  - [ ] 交叉引用校验通过（无悬空 ID）
  - [ ] 单元测试覆盖：正常加载、缺失文件、格式错误、引用断裂
  - [ ] `cargo test -p slg-assets` 通过
- **依赖**: 无（与 M1-T02 并行，使用现有占位数据测试；完整数据在 T02 完成后可用）
- **复杂度**: 中

---

#### M1-T04: 程序化地图生成管线

- **负责**: core-engineer
- **目标**: 实现 256x256 六边形地图的确定性程序化生成
- **具体内容**:
  - 在 `slg-core/src/gen/` 新建模块：`terrain.rs`、`resource.rs`、`spawn.rs`、`validate.rs`
  - `terrain.rs`：
    - 主种子 -> ChaCha12 派生子种子（地形/资源/出生点）
    - Simplex fBm 高程图（6 octave + Domain Warping）
    - 湿度图（独立通道 + 距水源衰减）
    - 温度图（纬度梯度 + 海拔衰减）
    - 地形分类查找表（高程 x 湿度 -> TerrainType）
    - 河流后处理（山脊源头 -> 最陡梯度下降 -> 洼地成湖）
  - `resource.rs`：
    - 圈层梯度 + 噪声扰动 -> 土地等级 1~9
    - 约束泊松盘采样 -> 资源点分布
    - 地形掩码过滤（铁在山地、粮在平原）
  - `spawn.rs`：
    - 泊松盘候选池 -> 模拟退火优化出生点公平性
    - 各出生点资源/防御/扩展潜力评分均衡
  - `validate.rs`：
    - Union-Find 连通性校验
    - 出生点互相可达校验
    - 关隘/核心资源可达校验
  - `generate_map(seed, width, height, preset) -> MapDocument` 管线入口函数
  - 确定性纪律：ChaCha12Rng + libm + BTreeMap（禁 HashMap）+ `jump()` 并行分块
  - 性能目标：256x256 < 5s
- **验收标准**:
  - [ ] `generate_map(42, 256, 256, default_preset)` 返回有效 MapDocument
  - [ ] 同种子两次生成结果逐格相同（insta 快照测试）
  - [ ] 输出地图无大面积水域死区（陆地占比 > 60%）
  - [ ] 生成的出生点数量 = 预期势力数，互相距离合理
  - [ ] 连通性校验通过
  - [ ] 256x256 生成耗时 < 5s（criterion 基准）
  - [ ] `cargo test -p slg-core` 全绿
  - [ ] 无 bevy 依赖
- **依赖**: M1-T01（需要 TerrainType / ResourceType 等类型定义）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

#### M1-T05: MapDocument <-> Runtime World 转换

- **负责**: core-engineer
- **目标**: 实现 §6.1 定义的三种形态转换接口
- **具体内容**:
  - `load_map_to_world(doc, world)`：
    - 读 meta -> 按 32x32 分 Chunk -> RLE 解码地形 -> 展开为 [TileData; 1024]
    - 生成 Chunk Entity + Components（TileTerrain / TileOwner / TileLevel / TileResource）
    - 稀疏层（BTree）逐项生成独立 Entity（城池/要塞）
    - 初始化 FactionStore / TerritoryGraph / FogOfWar / GameClock
  - `world_to_save(world, map_hash)`：
    - 遍历 Chunk Entity -> 计算 tile_delta（与 MapDocument 对比）
    - 快照武将/部队/城池/势力状态
    - 组装 SaveFile
  - `load_save_to_world(save, doc, world)`：
    - 先 `load_map_to_world(doc)` -> 再应用 `save.tile_delta` -> 恢复实体/势力状态
  - 此任务的函数签名在 slg-core（纯逻辑），但实际操作 Bevy World 的桥接代码在 slg-app
- **验收标准**:
  - [ ] MapDocument -> World -> MapDocument 往返 diff 为空
  - [ ] 256x256 地图加载后 Chunk 数量 = (256/32) x (256/32) = 64
  - [ ] 单测覆盖：空地图、单 Chunk、边界 Chunk、含城池地图
  - [ ] `cargo test -p slg-core -p slg-data` 通过
- **依赖**: M1-T01（ECS 组件）、M1-T04（需要 MapDocument 输入）
- **复杂度**: 高

---

#### M1-T06: 存档容器读写实现

- **负责**: editor-engineer
- **目标**: 将 slg-save 从骨架实现为完整的 .slgmap/.slgsave 容器读写
- **具体内容**:
  - 实现 TOC（Table of Contents）结构：每节 offset / size / crc32
  - 实现 Section 枚举与序列化：Meta / TerrainLayer / ResourceLayer / EntityLayer / RuleLayer / PreviewPNG
  - `save_map_to_file(doc, path)`：MapDocument -> .slgmap 容器（bincode + zstd 分节）
  - `load_map_from_file(path)`：.slgmap -> MapDocument
  - `save_to_file(save, path)`：SaveFile -> .slgsave 容器
  - `load_save_from_file(path)`：.slgsave -> SaveFile
  - 加载时校验地图 content_hash（SHA-256）
  - zstd 压缩/解压每个 Section
  - crc32 校验每节完整性
  - 预览图 PNG 嵌入/提取（256x256 缩略图，可留空桩）
- **验收标准**:
  - [ ] 新建 MapDocument -> save_map_to_file -> load_map_from_file -> diff 为空
  - [ ] SaveFile -> save_to_file -> load_save_from_file -> diff 为空
  - [ ] content_hash 校验：篡改地图文件后加载 .slgsave 报错
  - [ ] zstd 压缩后文件体积显著小于 bincode 原始大小
  - [ ] `cargo test -p slg-save` 通过
- **依赖**: 无（MapDocument / SaveFile 类型已在 M0 定义）
- **复杂度**: 中

---

### 阶段 B：核心游戏系统（依赖阶段 A）

---

#### M1-T07: 游戏时钟与 tick_dispatcher

- **负责**: core-engineer + render-engineer（协作）
- **目标**: 实现 §6.2 可暂停实时制游戏循环
- **具体内容**:
  - **core-engineer** 交付（slg-core/src/clock.rs）：
    - `GameClock` 的 `advance(delta_ms)` 逻辑：accumulator 累加，每 100ms 推进一个 tick
    - `Speed` 枚举的倍率计算
    - `GameTickSchedule` 定义：TickStart -> ResourceProduction -> BuildQueue -> Recruitment -> MarchAdvance -> CombatResolution -> TerritoryUpdate -> AIDecision -> TickEnd
    - 各阶段的 trait/函数签名（空实现桩，后续任务填充）
    - `PlayerCommand` 注入逻辑：TickStart 阶段从 CommandQueue 取出并执行
  - **render-engineer** 交付（slg-engine/src/systems.rs）：
    - Bevy `tick_dispatcher` 系统：每渲染帧读 frame_delta，维护 accumulator，触发 GameTickSchedule
    - 暂停/恢复/变速控制（键盘快捷键：Space 暂停、1/2/3 变速）
    - `GameClock` 作为 Bevy Resource 注册
    - `CommandQueue` 作为 Bevy Resource 注册
    - 渲染插值支持：暴露 `current_tick + accumulator/tick_duration` 给渲染系统
- **验收标准**:
  - [ ] 暂停时 accumulator 不增长，恢复后继续
  - [ ] x1 速度：1 秒 = 10 tick
  - [ ] x3 速度：1 秒 = 30 tick
  - [ ] 暂停时入队的指令在恢复后第一个 tick 执行
  - [ ] `GameTickSchedule` 各阶段按序调用（tracing 日志验证）
  - [ ] 单测覆盖 clock 逻辑，集成测试验证调度
- **依赖**: M1-T01（GameClock / CommandQueue / Speed 定义）
- **复杂度**: 中

---

#### M1-T08: 领地与铺路系统

- **负责**: core-engineer
- **目标**: 实现 §6.5 领地管理与铺路校验
- **具体内容**:
  - 在 `slg-core/src/map/territory.rs` 实现：
    - `Union-Find` 数据结构：路径压缩 + 按秩合并，根节点记连通块大小
    - `TerritoryGraph`：每势力一个 Union-Find 实例 + 全局 owner map
    - 占地校验 `can_occupy(coord, faction, territory, tile_owners) -> bool`：
      - 目标格为空或敌方
      - 六邻（hex）有己方格
      - 该邻居与主城同连通分量
    - `occupy(coord, faction, territory)`：执行占领，union 合并
    - 断连处理 `handle_disconnect(coord, territory)`：格子被夺取时对该连通块做块内 BFS 分裂，不与主城相连的子块标记"飞地"
    - 飞地宽限 N tick 后自动丢失机制
  - 在 slg-core/src/rule/territory_rule.rs 封装规则层接口
- **验收标准**:
  - [ ] 占领相邻格成功，占领非相邻格失败
  - [ ] 连通性正确维护：占领后新格加入连通块
  - [ ] 断连检测：中间格被夺后，飞地 BFS 分裂正确
  - [ ] proptest：随机占领序列后连通性一致性校验
  - [ ] 性能：1000 次占地操作 < 1ms（criterion）
  - [ ] `cargo test -p slg-core` 通过
- **依赖**: M1-T01（TerritoryGraph / OwnerFaction / HexCoord）
- **复杂度**: 高

---

#### M1-T09: 经济与资源系统

- **负责**: core-engineer
- **目标**: 实现资源产出、消耗、建筑队列与征兵
- **具体内容**:
  - 在 `slg-core/src/rule/economy.rs` 实现：
    - `tick_resources(faction_store, tile_owners, tile_levels, tile_resources, global_params)`：
      - 每 tick 根据领地等级计算资源产出（金/粮/木/铁/石）
      - 产出公式：base x level_multiplier x tile_resource_bonus x global_params.economy.resource_multiplier
      - 维护消耗：每部队每 tick 消耗粮食
    - `tick_build_queue(city_build_queues)`：推进建造队列，完成时应用效果
    - `tick_recruitment(city_garrisons, faction_store)`：征兵消耗资源，增加部队
  - `can_afford(faction, cost) -> bool`：检查资源是否足够
  - `spend_resources(faction, cost)`：扣除资源
  - 建筑效果解析：`food_production:N` / `recruit_speed:N` / 等
- **验收标准**:
  - [ ] 空领地无产出，占领 1 格 lv1 平原有正确基础产出
  - [ ] 高等级土地产出 > 低等级（线性或预设曲线）
  - [ ] 建造队列正确推进，完成后效果生效
  - [ ] 征兵消耗资源，资源不足时无法征兵
  - [ ] 粮食消耗：有部队时每 tick 扣粮
  - [ ] proptest：任意操作序列后资源守恒（产出 - 消耗 = 差额）
  - [ ] `cargo test -p slg-core` 通过
- **依赖**: M1-T01（FactionState / FactionResources / CityBuildQueue / GameClock）
- **复杂度**: 中

---

#### M1-T10: 行军与寻路系统

- **负责**: core-engineer
- **目标**: 实现 §6.3 Hex A* 寻路与部队行军推进
- **具体内容**:
  - 在 `slg-core/src/map/pathfinding.rs` 实现：
    - Hex A* 寻路：cube 坐标距离启发式，地形移动代价数据表驱动
    - LRU 路径缓存：key = (起点 TileKey, 终点 TileKey, 通行掩码)，容量 4096
    - 通行性判断：地形 passable 字段、河流跨越规则（仅渡口可渡）、敌方领地阻断
    - 移动代价：平原 1.0 / 山地 3.0 / 森林 1.5 / 关隘 2.0（读 terrain_types.ron）
  - 在 `slg-core/src/rule/movement.rs` 实现：
    - `request_march(army, destination, path_cache, tile_owners, terrain) -> Result<Vec<HexCoord>>`：
      - A* 计算路径，写入缓存
      - 预计算 `arrive_tick = current_tick + path.len() / army.speed`
    - `tick_march(armies, current_tick)`：每 tick 推进 path_index，到达终点时触发事件
    - 并发限流：每 tick 最多 32 个路径请求
  - `MarchArrived` 事件定义
- **验收标准**:
  - [ ] A* 路径正确：直线/绕山/绕水三种场景
  - [ ] 移动代价正确：山地路径更长
  - [ ] 缓存命中：相同起终点第二次请求更快
  - [ ] 行军推进：path_index 每 tick +1（或按速度），到达时触发事件
  - [ ] 限流：超过 32 请求时排队到下一 tick
  - [ ] `cargo test -p slg-core` 通过
- **依赖**: M1-T01（ArmyMarch / PathCache / HexCoord）、M1-T09（需要了解 FactionState 以判断通行性）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M1-T11: 战斗模拟系统

- **负责**: core-engineer
- **目标**: 实现 §6.4 纯函数确定性战斗模拟
- **具体内容**:
  - 在 `slg-core/src/rule/combat.rs` 实现：
    - `fn simulate(input: CombatInput) -> CombatReport`（纯函数，零 ECS 依赖）
    - `CombatInput { seed, attacker: CombatSide, defender: CombatSide, terrain, weather }`
    - `CombatSide { generals: [GeneralSnapshot; 3], troops: TroopInfo, formation, tech_bonuses }`
    - `GeneralSnapshot { stats, skills: Vec<SkillSnapshot>, troop_type }`
    - 流程：
      1. 准备阶段：计算阵法加成、兵种克制系数（骑->弓 / 弓->步 / 步->骑 各 x1.15，反向 x0.85）
      2. 最多 8 回合循环：
         - 速度定序（双方按 speed 排列交替行动）
         - 战法概率发动（读 SkillDef.trigger_rate，用种子派生子 RNG 判定）
         - 普攻计算（attack x 克制系数 x 地形适性 x 随机扰动 - defense）
         - 伤兵结算（HP 减少，士气影响）
         - 撤退判定（兵力 < 阈值或士气归零）
      3. 战损统计：最终兵力、经验获取、战利品
    - `CombatReport { rounds: Vec<RoundReport>, final_troops, winner, exp_gained, loot }`
    - 战法效果处理：damage / heal / buff / debuff 各类型的执行逻辑
    - 确定性：`seed = hash(attacker_id, defender_id, tile_key, tick)`
    - 数学函数用 `libm` 确保跨平台一致
  - 在 `slg-core/src/rule/combat_resolve.rs` 封装 ECS 桥接：
    - `resolve_combats(world, combat_reports)`：从 World 快照构建输入 -> simulate -> 写回兵力/经验 -> 发事件
    - 战斗限流：每 tick 最多 20 场，超出排队
- **验收标准**:
  - [ ] 同种子 1000 次调用 simulate 结果完全相同（proptest）
  - [ ] 兵种克制：骑兵 vs 弓兵伤害 x1.15
  - [ ] 8 回合上限：不会无限循环
  - [ ] 撤退：兵力归零或士气归零时战斗结束
  - [ ] 战法发动：trigger_rate=1.0 时必定发动，trigger_rate=0.0 时永不发动
  - [ ] 单场战斗 < 0.25ms（criterion）
  - [ ] `cargo test -p slg-core` 通过
  - [ ] 无 bevy 依赖
- **依赖**: M1-T01（CombatInput / CombatReport / GeneralStats / SkillDef）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

### 阶段 C：渲染与 UI（依赖阶段 A 数据结构，可与阶段 B 并行）

---

#### M1-T12: Chunk 渲染与 LOD 系统

- **负责**: render-engineer
- **目标**: 实现 §12 性能预算内的六边形地图渲染
- **具体内容**:
  - 在 `slg-engine/src/render/` 新建模块：`chunk_mesh.rs`、`lod.rs`、`atlas.rs`
  - `chunk_mesh.rs`：
    - 每 32x32 格生成一个 Bevy Mesh（六边形 pointy-top 几何体）
    - 顶点着色：按 TerrainType 从纹理图集采样，叠加 OwnerFaction 着色
    - 六边形几何：中心点 + 6 个三角形扇，UV 映射到 atlas
  - `atlas.rs`：
    - 加载或生成 8 种地形的占位纹理（纯色或简单图案）
    - 纹理图集打包为单张 atlas 纹理
    - 势力着色：5 种 AI 势力 + 玩家 = 6 种颜色叠加方案
  - `lod.rs`：
    - 4 级 LOD：Full（1 hex = 1 色）/ Merged4（2x2 合并）/ Merged16（4x4）/ Minimap（纯色块）
    - 基于相机缩放级别自动切换
    - 重建限流：每帧最多重建 16 个 Chunk，排队处理
    - ChunkDirty 标记驱动增量更新
  - 在 `slg-engine/src/render/mod.rs` 注册渲染系统
- **验收标准**:
  - [ ] 256x256 地图渲染无 panic
  - [ ] 64 个 Chunk 全部可见（相机居中时）
  - [ ] LOD 切换平滑，无闪烁
  - [ ] draw call < 200（粗略统计）
  - [ ] 60 FPS（无大量实体时）
  - [ ] 势力着色正确区分各阵营
- **依赖**: M1-T01（Chunk Component 结构）、M1-T04（需要 MapDocument 测试数据）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M1-T13: 相机系统与 hex 拾取

- **负责**: render-engineer
- **目标**: 实现相机平移缩放与屏幕坐标到 hex 坐标的精确拾取
- **具体内容**:
  - 在 `slg-engine/src/camera.rs` 实现：
    - 相机平移：鼠标中键拖拽 / WASD / 边缘滚屏（距离边缘 50px 开始减速滚动）
    - 相机缩放：滚轮缩放，限制 min/max zoom
    - 缩放范围：能看全图（minimap 级）到能看清单格（Full LOD 级）
  - hex 拾取（§7.1）：
    - 屏幕坐标 -> 相机射线 -> 与地面平面求交 -> 得到世界坐标 (x, y)
    - 世界坐标 -> axial 坐标转换（pointy-top hex 公式）
    - axial 坐标 -> `HexCoord::round()` 取最近六边形中心
    - 拾取高亮：鼠标悬停时高亮当前 hex
  - 在 SlgEnginePlugin 注册相机系统
- **验收标准**:
  - [ ] 鼠标拖拽平移流畅
  - [ ] 滚轮缩放在限制范围内
  - [ ] 边缘滚屏生效（鼠标移近窗口边缘时地图滚动）
  - [ ] hex 拾取精确：点击 hex 中心与边缘均返回正确坐标
  - [ ] 拾取高亮：鼠标悬停 hex 有视觉反馈
  - [ ] 性能：拾取每帧 < 0.01ms
- **依赖**: M1-T12（需要 Chunk 渲染才能看到地图）
- **复杂度**: 中

---

#### M1-T14: 迷雾渲染系统

- **负责**: render-engineer
- **目标**: 实现 §6.7 迷雾/视野的渲染层
- **具体内容**:
  - 迷雾纹理：
    - 每个 Chunk 对应一张 R8 纹理（256x256 像素，每格 8x8 像素）
    - 0 = 未探索（黑色）、1 = 已探索不可见（半透明黑）、2 = 当前可见（透明）
    - 纹理数据来自 `FogOfWar` Resource
  - Fragment shader 混合：
    - 在地形纹理之上叠加迷雾层
    - 三态视觉效果：全黑 / 半暗 / 完全透明
  - FogOfWar 更新逻辑（核心侧，在 TickEnd 阶段）：
    - 每个势力的视野来源：领地 hex + 部队 hex + 城池 hex
    - 视野范围：领地半径 2、部队半径 3、城池半径 4（cube ring）
    - 遮挡：cube 视线算法，山地/关隘阻挡视野
    - 大范围重算分帧：每 tick 处理 64 格
  - 渲染侧：每帧读 FogOfWar 纹理数据更新 GPU 纹理
- **验收标准**:
  - [ ] 未探索区域完全黑色
  - [ ] 已探索但无视野区域半透明
  - [ ] 当前视野内完全透明
  - [ ] 视野随部队移动正确更新
  - [ ] 山地正确遮挡视线
  - [ ] 迷雾渲染不影响 60 FPS
- **依赖**: M1-T01（FogOfWar Resource）、M1-T12（Chunk 渲染）、M1-T13（需要相机定位）
- **复杂度**: 高

---

#### M1-T15: 游玩 HUD 面板

- **负责**: render-engineer
- **目标**: 实现游戏游玩所需的基础 egui HUD
- **具体内容**:
  - 在 `slg-ui/src/panels/` 新建模块：`top_bar.rs`、`minimap.rs`、`general_card.rs`、`battle_report.rs`、`command_panel.rs`、`diplomacy.rs`
  - `top_bar.rs`：
    - 顶部资源栏：显示金/粮/木/铁/石 数值
    - tick 显示、速度控制按钮（暂停/x1/x2/x3）
    - 游戏内日期/天数
  - `minimap.rs`：
    - 小地图（窗口右下角）：按 Chunk 着色的缩略地图
    - 相机视野框显示
    - 点击小地图跳转相机位置
  - `general_card.rs`：
    - 武将卡片面板：显示五维雷达图、等级、经验、战法列表、兵种
    - 点击武将 Entity 弹出
  - `battle_report.rs`：
    - 战报面板：从 CombatReportStore 读取
    - 显示攻守双方、每回合战况、最终结果、战损
    - 战报列表（最近 N 条）
  - `command_panel.rs`：
    - 部队指令面板：选中部队后显示
    - 行军目标选择、征兵按钮、建造按钮
    - 指令发送到 CommandQueue
  - `diplomacy.rs`：
    - 外交面板（基础版）：显示各势力关系、好感度
    - 结盟/宣战/停战按钮
  - i18n：所有面板文案走 fluent（zh-CN），key 在 main.ftl 定义
- **验收标准**:
  - [ ] 顶部资源栏实时更新（资源变化后 1 tick 内反映）
  - [ ] 小地图正确显示势力分布
  - [ ] 武将卡片显示正确信息
  - [ ] 战报面板可查看最近战报
  - [ ] 指令面板可发送行军/征兵指令
  - [ ] 外交面板显示势力关系
  - [ ] 所有文案走 ftl，无硬编码
  - [ ] egui 渲染不影响 60 FPS
- **依赖**: M1-T01（ECS 组件定义）、M1-T13（相机系统用于小地图交互）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

### 阶段 D：AI 与集成（依赖阶段 B 核心系统）

---

#### M1-T16: AI 决策系统

- **负责**: core-engineer
- **目标**: 实现 §6.6 三层 AI 架构 + 基础外交
- **具体内容**:
  - 在 `slg-core/src/ai/` 新建模块：`utility.rs`、`strategy.rs`、`tactics.rs`、`diplomacy.rs`、`persona.rs`
  - `persona.rs`：
    - 势力人格模板：`FactionPersonality { aggression: f64, expansion: f64, diplomacy: f64, caution: f64 }`
    - 5 个预设：魏（扩张好战）、蜀（外交温和）、吴（防御稳健）、辽东（投机冒险）、南中（保守自守）
    - 人格影响效用评分的权重
  - `utility.rs`：
    - 效用评分函数：`score(action, personality, game_state) -> f64`
    - 评分公式示例：`score(占地) = 资源价值 x 距离衰减 x (1 - 敌方密度 x 0.5) x personality.aggression`
    - 候选动作枚举：OccupyTile / MarchAttack / Recruit / Build / SendDiplomacy / Reinforce / Scout
  - `strategy.rs`（Layer 1，每 50 tick）：
    - Region 价值评估 -> 主攻方向
    - 外交威胁评估 -> 结盟/宣战决策
    - 资源规划 -> 建造优先级
  - `tactics.rs`（Layer 2，每 10 tick）：
    - 候选动作生成 -> 效用评分 -> 取 Top-N
    - 动作生成规则：空闲部队 -> 寻找可占领格 / 可攻击目标
  - `execution.rs`（Layer 3，每 tick）：
    - 战术指令 -> 具体行军/建造/征兵命令入队
    - 转换为 PlayerCommand 写入 CommandQueue
  - `diplomacy.rs`：
    - 好感度系统：-100 ~ 100
    - 动作集：结盟 / 宣战 / 停战 / 送礼 / 威胁 / 贸易
    - 关系衰减：每 tick 好感度向 0 微调
    - 盟约效果：共享视野、互不攻击
  - 硬规则兜底（Layer 0，每 tick）：
    - 主城被围 -> 全军回防
    - 兵力 < 阈值 -> 停攻征兵
    - 资源 < 7 天消耗 -> 停建
  - 错峰调度：AISlotAssignments，`tick % 10 == slot` 时执行
  - 反作弊纪律：AI 无隐形成就加成，难度只调三个显式参数
- **验收标准**:
  - [ ] 5 个 AI 势力行为有明显差异（魏扩张快、蜀结盟多、吴防守强）
  - [ ] AI 不会做出违反规则的操作（不能占非相邻格）
  - [ ] 外交动作正确执行（结盟后互不攻击）
  - [ ] 错峰调度：每 tick 只有 1 个势力执行决策
  - [ ] 硬规则兜底：主城被围时 AI 回防
  - [ ] 100 tick 推演无 panic
  - [ ] `cargo test -p slg-core` 通过
- **依赖**: M1-T01（FactionState / AISlotAssignments）、M1-T08（领地校验）、M1-T09（资源系统）、M1-T10（行军系统）、M1-T11（战斗触发）
- **复杂度**: 高
- **预估工时**: 2~3 个会话

---

### 阶段 E：编辑器（依赖阶段 A + C 部分）

---

#### M1-T17: 编辑器基础工具

- **负责**: editor-engineer
- **目标**: 实现编辑器 Paint / FloodFill / PlaceEntity 工具与命令栈
- **具体内容**:
  - 在 `slg-editor/src/command.rs` 实现：
    - `EditorCommand` trait：`execute(doc) -> Result<()>` / `undo(doc) -> Result<()>` / `merge_hint() -> Option<MergeHint>`
    - `CommandHistory { undo_stack, redo_stack, max_depth: 200 }`
    - `execute_command(cmd, history, doc)`：执行并压入 undo_stack，清空 redo_stack
    - `undo(history, doc)`：弹出 undo_stack 顶部执行 undo
    - `redo(history, doc)`：弹出 redo_stack 顶部执行 execute
    - 连续笔刷合并：相同位置连续操作合并为单次 stroke
  - 在 `slg-editor/src/tool.rs` 实现：
    - `PaintBrush` 命令：修改单格地形类型，笔刷大小 1/3/5/10（cube ring 范围）
    - `FloodFill` 命令：从目标格开始泛洪填充相同地形为新地形
    - `PlaceEntity` 命令：在指定位置放置城池/要塞/关隘实体
    - `RemoveEntity` 命令：移除指定位置实体
    - Ghost 预览：鼠标移动时显示将要影响的 hex 范围
  - 在 `slg-editor/src/validate.rs` 实现基础校验：
    - 实体重叠检测（每笔操作后 <5ms）
    - 保存前全量校验（Error 阻止保存）
  - 编辑器模式切换 UI（egui 面板）：
    - 工具选择栏（Paint / Fill / Place / Select）
    - 图层选择（地形 / 资源 / 实体）
    - 笔刷大小选择
    - 撤销/重做按钮
    - 保存按钮
- **验收标准**:
  - [ ] Paint 笔刷可修改地形类型
  - [ ] FloodFill 从目标格开始正确填充
  - [ ] PlaceEntity 可放置城池实体
  - [ ] 撤销/重做正确（连续 10 次操作后逐级撤销）
  - [ ] 实体重叠校验阻止同一位置放两个城
  - [ ] 编辑器模式可通过快捷键切换（游玩 <-> 编辑）
  - [ ] `cargo test -p slg-editor` 通过
- **依赖**: M1-T01（MapDocument 类型）、M1-T12（渲染层需要可见才能编辑）
- **复杂度**: 中

---

### 阶段 F：集成与验证（依赖全部前置任务）

---

#### M1-T18: 全链路集成

- **负责**: render-engineer + core-engineer（协作）
- **目标**: 将所有子系统串联，实现从启动到可玩的完整流程
- **具体内容**:
  - **slg-app 集成**（render-engineer）：
    - `load_map_to_world` 桥接：调用 slg-core 的转换函数，将 MapDocument 加载到 Bevy World
    - 注册所有 ECS Component 到 Bevy App
    - 注册所有 Resource（GameClock / FactionStore / FogOfWar / CommandQueue 等）
    - 注册 GameTickSchedule 的各阶段系统
    - 新游戏流程：加载剧本 -> 生成地图 -> 初始化势力 -> 注册 AI 槽位 -> 开始 tick
    - 加载存档流程：读 .slgsave -> 校验 hash -> load_save_to_world
  - **三国鼎立剧本加载**：
    - 解析 scenario.ron -> 初始化 5 个 AI 势力 + 玩家势力
    - 放置初始城池/部队/武将
    - 设置初始外交关系
    - 注册事件链
  - **RenderPlugin 完善**：
    - 确保 Chunk 渲染、相机、迷雾、HUD 全部注册并协同
    - 渲染插值：读 GameClock 的 accumulator 平滑动画
  - **AI 调度集成**：
    - 错峰调度：tick_dispatcher 中按 slot 触发 AI 决策
    - AI 命令 -> CommandQueue -> 下一 tick 执行
  - **模式切换**：
    - EditorMode Resource 控制游玩/编辑模式切换
    - 编辑器模式：隐藏 HUD，显示编辑器面板，启用编辑工具
    - 游玩模式：显示 HUD，隐藏编辑器面板，启用 tick 推进
- **验收标准**:
  - [ ] 启动程序 -> 新游戏 -> 看到 256x256 地图渲染
  - [ ] 5 个 AI 势力初始城池/领土正确显示
  - [ ] 玩家势力初始位置正确
  - [ ] 点击格子可发起占领（需要相邻己方格）
  - [ ] 资源每 tick 正确更新
  - [ ] 部队行军可视化
  - [ ] 遭遇敌方时触发战斗，战报可查看
  - [ ] AI 势力自主行动（占地/征兵/行军）
  - [ ] 暂停/恢复/变速正常
  - [ ] 编辑器模式可切换，Paint 地形后切回游玩模式生效
  - [ ] 100 tick 推演无 panic
- **依赖**: M1-T01~T17 全部（这是最终集成任务）
- **复杂度**: 高
- **预估工时**: 2 个会话

---

#### M1-T19: 集成测试与试玩验收

- **负责**: qa-engineer
- **目标**: 编写集成测试、运行性能基准、试玩验证全部验收标准
- **具体内容**:
  - 集成测试（tests/）：
    - `test_map_generation_256`：生成 256x256 地图 -> 验证尺寸/连通性/出生点
    - `test_load_map_to_world`：加载 MapDocument -> 验证 Chunk 数量/内容
    - `test_100_tick_simulation`：初始化完整游戏状态 -> 推进 100 tick -> 无 panic
    - `test_combat_determinism`：同种子 1000 次战斗结果相同（proptest）
    - `test_generation_determinism`：同种子生成逐格快照（insta）
    - `test_save_load_roundtrip`：新建 -> 保存 -> 加载 -> diff 为空
    - `test_occupy_adjacent`：占领相邻格成功、非相邻格失败
    - `test_resource_production`：占领土地后资源正确产出
    - `test_march_arrival`：部队行军到达后触发事件
    - `test_ai_decision`：AI 势力在 100 tick 内做出决策
  - 性能基准（criterion）：
    - `bench_tick`：单 tick 耗时（目标 < 10ms）
    - `bench_pathfinding`：Hex A* 耗时
    - `bench_combat`：单场战斗模拟耗时（目标 < 0.25ms）
    - `bench_generation`：256x256 地图生成（目标 < 5s）
  - CI 更新：
    - 添加基准回归检查（>10% 阻断合并）
    - 确保依赖方向检查覆盖新增模块
  - 代码审查：
    - 随机抽查 5 个新增文件
    - 检查注释完整性、命名规范、错误处理（无裸 unwrap）、性能反模式
  - 试玩验收：
    - 按 AC-1 ~ AC-13 逐项验证
    - 输出缺陷清单（Blocker / Warning / Suggestion）
    - 输出 KPI 报告（§12 各项指标 vs 实测值）
- **验收标准**:
  - [ ] 所有集成测试通过
  - [ ] 性能基准基线入库
  - [ ] CI 全绿
  - [ ] 代码审查无 Blocker
  - [ ] 试玩验收 AC-1 ~ AC-13 全部通过
  - [ ] 输出完整验收报告
- **依赖**: M1-T01~T18 全部（这是最终验证任务）
- **复杂度**: 中

---

## 4. 执行顺序

### 4.1 可并行的任务组

```
并行组 1（立即启动，无依赖）:
  M1-T01  ECS 组件定义          [core-engineer]
  M1-T02  数据表扩展             [content-designer]
  M1-T03  slg-assets 加载实现    [core-engineer]  （注意：与 T01 同 agent，需串行或交替）
  M1-T06  存档容器读写           [editor-engineer]

并行组 2（依赖 T01 完成后）:
  M1-T04  地图生成管线           [core-engineer]
  M1-T07  时钟与 tick_dispatcher [core-engineer + render-engineer]
  M1-T08  领地与铺路             [core-engineer]
  M1-T09  经济与资源             [core-engineer]
  M1-T11  战斗模拟               [core-engineer]

并行组 3（依赖 T01 + 并行组 2 部分完成）:
  M1-T05  MapDoc<->World 转换    [core-engineer]  （依赖 T01 + T04）
  M1-T10  行军与寻路             [core-engineer]  （依赖 T01 + T09）
  M1-T12  Chunk 渲染与 LOD       [render-engineer]（依赖 T01 + T04）
  M1-T15  HUD 面板              [render-engineer]（依赖 T01）
  M1-T17  编辑器工具             [editor-engineer]（依赖 T01 + T12）

并行组 4（依赖并行组 3 部分完成）:
  M1-T13  相机与 hex 拾取        [render-engineer]（依赖 T12）
  M1-T14  迷雾渲染               [render-engineer]（依赖 T01 + T12）
  M1-T16  AI 决策系统            [core-engineer]  （依赖 T08 + T09 + T10 + T11）

并行组 5（依赖全部前置）:
  M1-T18  全链路集成             [render + core]
  M1-T19  集成测试与验收         [qa-engineer]    （依赖 T18）
```

### 4.2 必须串行的依赖链

**链 1：核心逻辑链**（core-engineer 主线，关键路径）

```
T01 -> T08 -> T16 -> T18 -> T19
  \     \      ^
   \     +-> T10 (行军，依赖 T09)
    \    ^
     +-> T09 (经济)
     +-> T11 (战斗)
```

**链 2：地图与渲染链**（render-engineer 主线）

```
T01 -> T12 -> T13 -> T18
  \      \     ^
   \      +-> T14
    +-> T15 -> T18
```

**链 3：地图生成链**（core-engineer）

```
T01 -> T04 -> T05 -> T18
```

**链 4：数据内容链**（content-designer，独立于逻辑链）

```
T02 -> T16 (AI 使用数据表)
    -> T18 (集成时使用完整数据)
```

**链 5：编辑器链**（editor-engineer）

```
T06 -> T18 (存档在集成时使用)
T01 -> T12 -> T17 -> T18 (编辑器依赖渲染)
```

### 4.3 关键路径分析

**关键路径**（决定 M1 最短完成时间）：

```
T01 (ECS 组件，2 会话)
 -> T08 (领地系统，1 会话)
 -> T16 (AI 系统，2~3 会话)
 -> T18 (全链路集成，2 会话)
 -> T19 (测试验收，1 会话)

总计：8~9 个会话
```

**并行路径**（渲染，可与关键路径并行）：

```
T01 -> T12 (Chunk 渲染，2 会话) -> T13 (相机，1 会话) -> T14 (迷雾，1 会话)
总计：4 个会话
```

**并行路径**（数据，完全独立）：

```
T02 (数据表，2 会话) -> 可随时合并
```

### 4.4 core-engineer 任务排序

core-engineer 负担最重（T01/T03/T04/T05/T07/T08/T09/T10/T11/T16 共 10 张卡），建议排序：

```
Session 1:  T01 (ECS 组件定义)
Session 2:  T03 (slg-assets) + T07-part (clock 逻辑)
Session 3:  T04 (地图生成管线)
Session 4:  T04 续 + T05 (MapDoc<->World)
Session 5:  T08 (领地系统)
Session 6:  T09 (经济系统)
Session 7:  T10 (行军寻路)
Session 8:  T11 (战斗模拟)
Session 9:  T16 (AI 决策)
Session 10: T16 续
```

### 4.5 render-engineer 任务排序

```
Session 1:  T07-part (tick_dispatcher Bevy 侧)
Session 2:  T12 (Chunk 渲染)
Session 3:  T12 续 + T13 (相机)
Session 4:  T14 (迷雾渲染)
Session 5:  T15 (HUD 面板)
Session 6:  T15 续
Session 7:  T18 (集成，与 core-engineer 协作)
```

---

## 5. 跨 agent 协作节点

### 5.1 协作节点清单

| # | 节点 | 参与方 | 协作内容 | 触发时机 |
|---|------|--------|---------|---------|
| C1 | ECS 组件定义评审 | core -> render, editor | core 定义的 ECS 组件结构影响所有下游；render/editor 需确认可映射到 Bevy | T01 完成后 |
| C2 | tick_dispatcher 接口对齐 | core <-> render | core 定义 GameTickSchedule 阶段签名，render 在 Bevy 侧实现调度 | T07 期间 |
| C3 | MapDocument 接口对齐 | core -> editor | core 实现 load_map_to_world 签名，editor 实现存档读写时需匹配 | T05 + T06 期间 |
| C4 | 地图生成数据对接 | core -> render | core 生成 MapDocument，render 需能正确渲染 | T04 + T12 期间 |
| C5 | 战斗模拟接口 | core -> render (UI) | CombatReport 结构影响战报面板设计 | T11 + T15 期间 |
| C6 | AI 命令注入 | core -> render | AI 生成的 PlayerCommand 写入 CommandQueue，render 的 tick_dispatcher 消费 | T07 + T16 期间 |
| C7 | 迷雾数据流 | core -> render | FogOfWar Resource 由 core 更新，render 读取渲染 | T14 期间 |
| C8 | 数据表与 AI 对齐 | content -> core | content 定义的势力人格标签、事件触发条件需 core AI 系统支持 | T02 + T16 期间 |
| C9 | 编辑器模式切换 | editor <-> render | EditorMode Resource 控制 UI 显示与系统启用 | T17 + T18 期间 |
| C10 | 存档往返验证 | editor + core + qa | editor 实现存档 IO，core 实现状态快照，qa 编写往返测试 | T06 + T05 + T19 期间 |

### 5.2 协作协议

- 每个协作节点，发起方在完成报告中明确标注"下游影响"与"接口签名"
- 接收方在开始任务前先读取上游的接口定义
- 如有接口分歧，由 arch-guardian 依据 ARCHITECTURE.md 裁决
- master-coordinator 在协作节点完成后更新 PROGRESS.md

---

## 6. 风险与缓解

### 6.1 M1 层面风险

| # | 风险 | 影响 | 概率 | 缓解措施 |
|---|------|------|------|---------|
| MR1 | core-engineer 负载过重（10 张卡） | 关键路径延长 | 高 | 拆分 T01 为两阶段（先结构后 Resource）；T07 的 clock 逻辑可由 core 先做纯逻辑层，render 做 Bevy 桥接 |
| MR2 | ECS 组件设计不合理导致下游大量返工 | 全局影响 | 中 | T01 完成后安排 arch-guardian 评审 + render/editor 确认；先写接口再填实现 |
| MR3 | 战斗数值平衡导致 AI 行为异常 | AI 测试困难 | 中 | T11 先用极端简单参数（纯普攻）跑通流程，数值迭代后置；content-designer 提供基准数值 |
| MR4 | Chunk 渲染性能不达标 | 用户体验差 | 中 | T12 先做最小可用（无 LOD），性能验证后再加 LOD；占位纹理避免加载延迟 |
| MR5 | 地图生成管线复杂度超预期 | T04 延期 | 中 | T04 先实现简化版（仅高程 + 地形分类，无河流），后续迭代；256x256 比 512x512 压力小 4 倍 |
| MR6 | Bevy 0.15 API 变更导致适配问题 | render 层延期 | 低 | M0 已验证 bevy 0.15 可编译；render 层集中处理引擎适配 |
| MR7 | 六边形渲染几何错误 | 拾取不准 | 中 | 先用简单六边形验证坐标公式正确性，再加视觉细节；Red Blob Games 公式已有验证 |
| MR8 | AI 势力决策导致游戏死锁 | 100 tick 无法完成 | 低 | 硬规则兜底 Layer 0 每 tick 执行；限流机制（32 路径/tick、20 战斗/tick） |
| MR9 | 数据表内容量大导致 content-designer 延期 | 集成时缺数据 | 中 | T02 先完成最小可玩数据集（10 武将 + 5 战法 + 3 兵种），完整数据迭代补充 |
| MR10 | 编辑器与游戏模式切换数据丢失 | 用户体验差 | 低 | M1 用全量转换（V1 简化策略），不保留游玩状态 |

### 6.2 性能预算检查点

| 检查点 | 时机 | 指标 | 不达标处理 |
|--------|------|------|-----------|
| 地图生成 | T04 完成 | 256x256 < 5s | 降 octave 数 / 简化后处理 |
| Chunk 渲染 | T12 完成 | 60 FPS 64 chunks | 降 LOD 级别 / 限流 chunk 重建 |
| 单 tick | T18 集成后 | < 10ms | profile 热点，优化 Union-Find / A* |
| 战斗模拟 | T11 完成 | < 0.25ms/场 | 简化战法效果 / 预计算 |
| 内存 | T18 集成后 | < 200MB（256x256） | 检查纹理 / Chunk 内存 |

### 6.3 回退策略

如果 M1 某个子系统延期导致无法在计划内完成全部验收标准：

1. **AI 系统延期**：降级为随机 AI（随机选择动作），保留接口，后续迭代
2. **迷雾系统延期**：暂时移除迷雾，全图可见，后续迭代
3. **编辑器延期**：M1 仅验证核心循环（AC-1 ~ AC-12），编辑器（AC-13）推迟到 M1.5
4. **HUD 面板延期**：用最简文本 UI 替代（egui 窗口显示关键数值），后续美化

---

## 附录 A：任务卡总表

| 卡号 | 标题 | 负责 | 依赖 | 复杂度 | 阶段 |
|------|------|------|------|--------|------|
| M1-T01 | ECS 组件与 Resource 定义 | core-engineer | 无 | 高 | A |
| M1-T02 | 数据表扩展与三国内容 | content-designer | 无 | 高 | A |
| M1-T03 | slg-assets 加载实现 | core-engineer | 无 | 中 | A |
| M1-T04 | 程序化地图生成管线 | core-engineer | T01 | 高 | A |
| M1-T05 | MapDoc<->World 转换 | core-engineer | T01, T04 | 高 | B |
| M1-T06 | 存档容器读写 | editor-engineer | 无 | 中 | A |
| M1-T07 | 时钟与 tick_dispatcher | core + render | T01 | 中 | B |
| M1-T08 | 领地与铺路系统 | core-engineer | T01 | 高 | B |
| M1-T09 | 经济与资源系统 | core-engineer | T01 | 中 | B |
| M1-T10 | 行军与寻路系统 | core-engineer | T01, T09 | 高 | B |
| M1-T11 | 战斗模拟系统 | core-engineer | T01 | 高 | B |
| M1-T12 | Chunk 渲染与 LOD | render-engineer | T01, T04 | 高 | C |
| M1-T13 | 相机与 hex 拾取 | render-engineer | T12 | 中 | C |
| M1-T14 | 迷雾渲染 | render-engineer | T01, T12 | 高 | C |
| M1-T15 | HUD 面板 | render-engineer | T01 | 高 | C |
| M1-T16 | AI 决策系统 | core-engineer | T08, T09, T10, T11 | 高 | D |
| M1-T17 | 编辑器基础工具 | editor-engineer | T01, T12 | 中 | E |
| M1-T18 | 全链路集成 | render + core | T01~T17 | 高 | F |
| M1-T19 | 集成测试与验收 | qa-engineer | T18 | 中 | F |

**统计**：
- 总任务卡：19 张
- core-engineer：10 张（T01/T03/T04/T05/T07/T08/T09/T10/T11/T16）
- render-engineer：6 张（T07/T12/T13/T14/T15/T18）
- editor-engineer：2 张（T06/T17）
- content-designer：1 张（T02）
- qa-engineer：1 张（T19）
- arch-guardian：审查角色（无独立任务卡，在 T01/T07/T18 节点参与评审）

---

## 附录 B：§6.8 ECS 数据映射表快速参考

| 游戏对象 | 存储方式 | 负责模块 |
|----------|----------|---------|
| Tile 地形 | Chunk Component `TileTerrain([TerrainType; 1024])` | T01 |
| Tile 归属 | Chunk Component `TileOwner([u8; 1024])` | T01 |
| Tile 等级 | Chunk Component `TileLevel([u8; 1024])` | T01 |
| Tile 资源 | Chunk Component `TileResource([Option<ResourceType>; 1024])` | T01 |
| Chunk dirty | Chunk Component `ChunkDirty(bool)` | T01 |
| 武将 | ECS Entity + GeneralStats/GeneralSkills/OwnerFaction | T01 |
| 部队 | ECS Entity + ArmyTroops/ArmyMarch/ArmyPosition/OwnerFaction | T01 |
| 城池 | ECS Entity + CityLevel/CityGarrison/CityBuildQueue/Position/OwnerFaction | T01 |
| 势力状态 | Resource `FactionStore` | T01 |
| 游戏参数 | Resource `GlobalParams` | T01 |
| 迷雾 | Resource `FogOfWar` | T01 |
| 命令队列 | Resource `CommandQueue` | T01 |
| 时钟 | Resource `GameClock` | T01 |
| 领地 | Resource `TerritoryGraph` | T01/T08 |
| 寻路缓存 | Resource `PathCache` | T01/T10 |
| 战报存储 | Resource `CombatReportStore` | T01/T11 |
| AI 决策槽 | Resource `AISlotAssignments` | T01/T16 |
