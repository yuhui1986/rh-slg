# M0 执行计划 —— workspace 骨架与 CI 全绿

> 制定人：master-coordinator
> 制定日期：2026-08-02
> 架构基线：ARCHITECTURE.md v1.3
> M0 目标：`cargo build` 跑起来 + CI 全绿 + 关键架构接口定义落地

---

## 1. M0 总体目标与验收标准

### 1.1 一句话目标

**从零到一：项目能编译、能测试、CI 能跑、架构接口有文档、agent 能接力开发。**

### 1.2 验收标准（M0 闭环条件）

- [ ] `Cargo.toml` workspace 根存在，8 个 crate 全部可 `cargo check`
- [ ] `cargo fmt --check` 通过（workspace 全量）
- [ ] `cargo clippy -- -D warnings` 通过（workspace 全量）
- [ ] `cargo test` 全绿（至少 slg-core hex 模块有单测）
- [ ] `grep -r "bevy" crates/slg-core crates/slg-data` 输出为空（核心层零引擎依赖红线）
- [ ] `.github/workflows/ci.yml` 存在且本地可复现
- [ ] Bevy 版本已 pin（Cargo.toml 中指定具体版本号，如 `0.15.x`）
- [ ] ARCHITECTURE.md §6.1 已补充"地图三形态转换接口"伪代码签名
- [ ] ARCHITECTURE.md 已补充"ECS 数据映射表"
- [ ] `assets/data/` 目录存在，至少有 1 个 RON 文件可被 slg-data 类型反序列化
- [ ] `assets/i18n/zh-CN/` 目录存在，至少有 1 个 `.ftl` 文件

### 1.3 M0 不做的事

- 不实现任何游戏逻辑（战斗/寻路/AI/生成管线均为空壳）
- 不实现渲染（slg-engine/slg-ui 只有 Bevy 插件骨架）
- 不实现编辑器（slg-editor 只有空 crate）
- 不实现存档（slg-save 只有容器格式定义骨架）
- 不做数值平衡（content-designer 只放占位数据）

---

## 2. 参与 Agent 列表及职责

| Agent | M0 职责 | 涉及 crate |
|-------|---------|-----------|
| `arch-guardian` | 补充 ECS 数据映射表、地图三形态转换接口；审查 workspace 结构合规性 | ARCHITECTURE.md |
| `core-engineer` | slg-data 类型骨架 + slg-core hex 网格模块 | slg-data, slg-core |
| `render-engineer` | slg-engine/slg-ui Bevy 插件骨架（空壳，能编译） | slg-engine, slg-ui |
| `editor-engineer` | slg-editor/slg-save 空 crate 骨架 | slg-editor, slg-save |
| `content-designer` | assets/data/ RON 占位数据 + assets/i18n/ ftl 文件 | assets/ |
| `qa-engineer` | workspace Cargo.toml + CI 配置 + 集成验证 | 根 Cargo.toml, .github/, tests/ |

---

## 3. 任务卡清单

---

#### M0-T1: workspace 骨架创建
- 负责: `qa-engineer`
- 目标: 创建 workspace 根 Cargo.toml 及 8 个 crate 的目录与 Cargo.toml，确保 `cargo check` 全量通过
- 具体内容:
  - 创建根 `Cargo.toml`，定义 workspace members（8 crate）
  - 为每个 crate 创建 `Cargo.toml`（名称、版本 0.1.0、edition 2021）
  - 为每个 crate 创建 `src/lib.rs`（空文件或 `//! crate doc`）
  - 每个 crate 的 Cargo.toml 声明正确的依赖关系（参照 ARCHITECTURE §5.2 依赖方向）
  - 外部依赖版本清单（参照 §5.3）：
    - bevy = "0.15" (slg-engine, slg-ui, slg-app)
    - bevy_egui (slg-ui, slg-editor)
    - serde + ron (slg-data, slg-assets)
    - bincode + zstd (slg-save)
    - rand + rand_chacha (slg-core)
    - noise (slg-core)
    - thiserror (所有库 crate)
    - anyhow (slg-app)
    - tracing + tracing-subscriber (slg-app)
    - fluent + i18n-embed (slg-ui, slg-assets)
  - 创建 `assets/` 目录结构（textures/, audio/, fonts/, data/, i18n/zh-CN/, maps/）
  - 创建 `tests/` 目录（空）
  - 创建 `mods/` 和 `user/` 目录（空）
- 验收标准:
  - [ ] `cargo check` workspace 全量通过
  - [ ] `cargo fmt --check` 全量通过
  - [ ] `cargo clippy -- -D warnings` 全量通过
  - [ ] 目录结构符合 ARCHITECTURE §5.1
  - [ ] 依赖方向符合 §5.2（slg-data 不依赖 bevy，slg-core 不依赖 bevy）
- 依赖: 无
- 复杂度: 中

---

#### M0-T2: CI 流水线配置
- 负责: `qa-engineer`
- 目标: GitHub Actions CI 在 Windows 上全量通过
- 具体内容:
  - 创建 `.github/workflows/ci.yml`
  - 触发条件：push 到 main/master + PR
  - 平台矩阵：仅 Windows（Q4 决策）
  - Job 1: `lint` — `cargo fmt --check` + `cargo clippy -- -D warnings`
  - Job 2: `test` — `cargo test --workspace`
  - Job 3: `dependency-check` — `grep -r "bevy" crates/slg-core crates/slg-data` 确认为空
  - Job 4: `build` — `cargo build --release`
  - Job 依赖：lint → test → build；dependency-check 与 lint 并行
  - 缓存：cargo registry + target 目录
  - 所有 job 运行在 `windows-latest`
- 验收标准:
  - [ ] `.github/workflows/ci.yml` 存在且语法正确
  - [ ] 本地 `act` 或手动验证：lint 通过、test 通过、dependency-check 通过、build 通过
  - [ ] CI 包含依赖方向红线检查
- 依赖: M0-T1（需要 workspace 先能编译）
- 复杂度: 中

---

#### M0-T3: ECS 数据映射表与地图三形态转换接口
- 负责: `arch-guardian`
- 目标: 在 ARCHITECTURE.md 中补充实现必需的两个关键接口文档
- 具体内容:
  - **ECS 数据映射表**（新增 §6.8 或补充 §6.1）：

    | 游戏对象 | 存储方式 | 说明 |
    |----------|----------|------|
    | Tile 数据（terrain/owner/level/resource） | Chunk Entity 上的 Component（`Tiles` 包含 `[TileData; 1024]`） | 每 32×32 格一个 Chunk Entity |
    | Chunk dirty 标记 | Chunk Entity 的 Component `ChunkDirty(bool)` | 用于增量渲染更新 |
    | 武将 | ECS Entity + Components（`GeneralStats`, `GeneralSkills`, `OwnerFaction`） | 全图数百~数千 |
    | 部队 | ECS Entity + Components（`ArmyTroops`, `MarchPath`, `Position`） | 全图数百~数千 |
    | 城池 | ECS Entity + Components（`CityLevel`, `CityGarrison`, `Position`）+ 静态数据在 Resource | 全图数十~数百 |
    | 势力状态 | Resource `FactionStore`（HashMap<FactionId, FactionState>） | 4~8 个势力 |
    | 游戏参数 | Resource `GlobalParams` | 经济/军事倍率 |
    | 迷雾 | Resource `FogOfWar`（每 Chunk 一个 `[u8; 1024]`） | TickEnd 更新 |
    | 命令队列 | Resource `CommandQueue` | 暂停时存指令 |
    | 时钟 | Resource `GameClock`（current_tick, speed, accumulator） | tick_dispatcher 维护 |

  - **地图三形态转换接口伪代码**（补充 §6.1）：

    ```rust
    // MapDocument → Runtime World
    fn load_map_to_world(doc: &MapDocument, world: &mut World) -> Result<()>
    // 流程：读 meta → 按 32×32 分 Chunk → RLE 解码地形 → 展开为 [TileData; 1024]
    // → 生成 Chunk Entity + Components → 稀疏层(BTree)逐项生成独立 Entity

    // Runtime World → Save
    fn world_to_save(world: &World, map_hash: [u8; 32]) -> Result<SaveFile>
    // 流程：遍历 Chunk Entity → 计算 tile_delta（与 MapDocument 对比）
    // → 快照武将/部队/城池/势力状态 → 组装 SaveFile

    // Save → Runtime World
    fn load_save_to_world(save: &SaveFile, doc: &MapDocument, world: &mut World) -> Result<()>
    // 流程：先 load_map_to_world(doc) → 再应用 save.tile_delta → 恢复实体/势力状态

    // MapDocument ↔ 磁盘
    fn save_map_to_file(doc: &MapDocument, path: &Path) -> Result<()>  // .slgmap 容器
    fn load_map_from_file(path: &Path) -> Result<MapDocument>

    // Save ↔ 磁盘
    fn save_to_file(save: &SaveFile, path: &Path) -> Result<()>  // .slgsave 容器
    fn load_save_from_file(path: &Path) -> Result<SaveFile>
    ```

  - **编辑器模式同步策略**（补充 §7.1）：
    - 编辑器操作直接修改 `MapDocument`（内存中）
    - 切换到游玩模式时调用 `load_map_to_world`
    - 切换回编辑器时调用 `world_to_save` 提取 delta，合并回 `MapDocument`
    - 这是 V1 简化策略，V2 可优化为实时双缓冲

- 验收标准:
  - [ ] ARCHITECTURE.md 已更新，版本号升至 v1.4
  - [ ] ECS 数据映射表覆盖所有 §6 中提到的游戏对象
  - [ ] 转换接口伪代码覆盖 MapDocument/World/Save 三向转换 + 磁盘 IO
  - [ ] 编辑器同步策略已定义
  - [ ] 变更记录已添加
- 依赖: 无（纯文档工作）
- 复杂度: 高

---

#### M0-T4: slg-data 共享数据结构骨架
- 负责: `core-engineer`
- 目标: 定义 slg-data 中所有共享类型与 ID 结构，`cargo check -p slg-data` 通过
- 具体内容:
  - `ids.rs`：语义化 ID 类型别名（`GeneralId`, `SkillId`, `FactionId`, `TileKey` 等均为 `String` 封装或 newtype）
  - `map_doc.rs`：`MapDocument` 结构体骨架
    - `meta: MapMeta`（name, seed, size, preset_name）
    - `terrain_layer: TerrainLayer`（RLE 密集数组骨架）
    - `resource_layer: BTreeMap<TileKey, ResourceEntry>`（稀疏）
    - `entity_layer: BTreeMap<TileKey, EntityPlacement>`（稀疏）
    - `rule_layer: RuleLayer`（区域/触发器，骨架）
  - `config.rs`：配置表结构定义
    - `GeneralDef`（id, name, rarity, stats, skill_ids, unit_type_ids）
    - `SkillDef`（id, skill_type, trigger_rate, target_strategy, damage_formula, effects）
    - `UnitTypeDef`（id, name, attack, defense, hp, speed, counter_target, terrain_adaptation）
    - `TerrainTypeDef`（id, movement_cost, defense_bonus, passable, buildable）
    - `BuildingDef`（id, category, levels, terrain_req, effect）
    - `EventDef`（id, trigger, effect, script_hook: Option<String>）
    - `GlobalParams`（economy, military, map, diplomacy 倍率组）
  - `save.rs`：`SaveFile` 结构体骨架
    - `map_ref: MapRef`（path, content_hash: [u8; 32]）
    - `tick: u64`
    - `faction_states: Vec<FactionState>`
    - `entity_snapshots: Vec<EntitySnapshot>`
    - `tile_delta: Vec<TileDelta>`
    - `event_log: Vec<EventLogEntry>`
  - 所有类型 derive `Serialize, Deserialize, Debug, Clone`
  - 所有 ID 字段为语义化字符串
  - 无硬编码游戏数值
- 验收标准:
  - [ ] `cargo check -p slg-data` 通过
  - [ ] `cargo clippy -p slg-data -- -D warnings` 通过
  - [ ] 所有公共类型有 doc comment
  - [ ] 无 bevy 依赖（`grep -r "bevy" crates/slg-data` 为空）
  - [ ] 无硬编码数值（数值均来自配置表类型定义，不在结构体中写默认值）
- 依赖: M0-T3（需要参考 ECS 数据映射表确定类型字段）
- 复杂度: 中

---

#### M0-T5: slg-core hex 网格数学模块
- 负责: `core-engineer`
- 目标: 实现六边形坐标系基础数学，附完整单测
- 具体内容:
  - `map/mod.rs`：模块导出
  - `map/grid.rs`：
    - `HexCoord { q: i32, r: i32 }` — axial 坐标（pointy-top）
    - `to_cube(self) -> (i32, i32, i32)` — axial → cube 转换
    - `from_cube(x: i32, y: i32, z: i32) -> Self` — cube → axial
    - `distance(self, other: Self) -> i32` — cube 距离
    - `neighbors(self) -> [Self; 6]` — 6 邻域（pointy-top 方向常量）
    - `ring(self, radius: i32) -> Vec<Self>` — cube ring
    - `line(a: Self, b: Self) -> Vec<Self>` — cube 视线（用于迷雾 LOS）
    - `round(fq: f64, fr: f64) -> Self` — hex rounding（拾取用）
  - `map/tile.rs`：
    - `TileData { terrain_id: String, owner: Option<String>, level: u8, resource: Option<String> }` — 单格数据
    - `TileType` 枚举占位（Plains, Mountain, Water, Forest, ...）
  - 测试：
    - `distance` 对称性、三角不等式
    - `neighbors` 返回 6 个不同坐标
    - `ring(1)` 返回 6 个坐标
    - `round` 已知输入输出对
    - `line` 对称性
- 验收标准:
  - [ ] `cargo test -p slg-core` 通过，至少 10 个测试用例
  - [ ] `cargo clippy -p slg-core -- -D warnings` 通过
  - [ ] 无 bevy 依赖（`grep -r "bevy" crates/slg-core` 为空）
  - [ ] 公共 API 有 doc comment
  - [ ] 坐标系使用 axial `(q,r)` + cube 计算，符合 D3 决策
- 依赖: M0-T3（需要参考 ECS 映射确认 TileData 字段）
- 复杂度: 中

---

#### M0-T6: 内容数据骨架
- 负责: `content-designer`
- 目标: 创建 assets/data/ 与 assets/i18n/ 目录及占位数据文件
- 具体内容:
  - `assets/data/generals.ron`：至少 3 个武将占位条目（曹操、刘备、孙权），字段与 slg-data `GeneralDef` 对齐
  - `assets/data/skills.ron`：至少 2 个战法占位条目
  - `assets/data/unit_types.ron`：至少 3 个兵种（骑/弓/步）
  - `assets/data/terrain_types.ron`：至少 4 种地形（平原/山地/水域/森林）
  - `assets/data/buildings.ron`：至少 2 个建筑占位条目
  - `assets/data/events.ron`：至少 1 个事件占位条目
  - `assets/data/global_params.ron`：经济/军事/地图/外交四组倍率（均设为 1.0 基准）
  - `assets/i18n/zh-CN/main.ftl`：至少 10 个基础文案 key（游戏标题、菜单项、资源名称等）
  - 所有 ID 使用语义化命名（如 `general_wei_caocao`）
  - 数值为占位基准值，标注 `// TODO: 平衡调整`
- 验收标准:
  - [ ] 所有 RON 文件语法正确（可被 `ron::from_str` 解析）
  - [ ] ID 命名符合语义化规范
  - [ ] 字段与 slg-data 定义的类型对齐（需等 M0-T4 完成后确认）
  - [ ] 零硬编码：所有数值在 RON 文件中，不在代码中
  - [ ] ftl 文件 UTF-8 编码，key 命名规范
- 依赖: M0-T4（需要 slg-data 类型定义来对齐 RON 字段）
- 复杂度: 低

---

#### M0-T7: 渲染与编辑器 crate 骨架
- 负责: `render-engineer` + `editor-engineer`（并行）
- 目标: slg-engine/slg-ui/slg-editor/slg-save 有最小可编译骨架
- 具体内容:

  **render-engineer 负责 slg-engine + slg-ui：**
  - `slg-engine/src/lib.rs`：
    - 定义 `SlgEnginePlugin`（Bevy Plugin 空壳）
    - `pub fn build(&self, app: &mut App) {}` — 空实现，M1 再填充
  - `slg-ui/src/lib.rs`：
    - 定义 `SlgUiPlugin`（Bevy Plugin 空壳）
    - 导出 `pub mod panels;`（空模块）

  **editor-engineer 负责 slg-editor + slg-save：**
  - `slg-editor/src/lib.rs`：
    - 定义 `SlgEditorPlugin`（Bevy Plugin 空壳）
    - 导出 `pub mod tool;` `pub mod command;`（空模块）
  - `slg-save/src/lib.rs`：
    - 定义容器格式常量：`MAGIC: &[u8] = b"SLGM"`, `VERSION: u32 = 1`
    - `SaveHeader { magic, version, toc_offset }` 结构体
    - `SaveError` 错误类型（thiserror）
    - 导出 `pub mod container;`（空模块）

- 验收标准:
  - [ ] `cargo check -p slg-engine -p slg-ui -p slg-editor -p slg-save` 全部通过
  - [ ] `cargo clippy` 无警告
  - [ ] slg-engine/slg-ui 的 Plugin 实现符合 Bevy Plugin trait
  - [ ] slg-save 容器常量与 ARCHITECTURE §10.1 一致
- 依赖: M0-T1（需要 Cargo.toml 存在）
- 复杂度: 低

---

#### M0-T8: slg-app 入口与 slg-assets 加载器骨架
- 负责: `render-engineer`（slg-app 入口）+ `core-engineer`（slg-assets 加载器）
- 目标: slg-app 能启动空 Bevy 窗口，slg-assets 有 RON 加载骨架
- 具体内容:

  **render-engineer 负责 slg-app：**
  - `slg-app/src/main.rs`：
    - `fn main()` 启动 Bevy App
    - 注册 `SlgEnginePlugin`, `SlgUiPlugin`, `SlgEditorPlugin`
    - 基础窗口配置：标题"天下策"，分辨率 1280×720
    - `setup` system：spawn 2D Camera
  - `slg-app/src/lib.rs`：
    - `SlgAppPlugin` — 组装所有子插件

  **core-engineer 负责 slg-assets：**
  - `slg-assets/src/lib.rs`：
    - `DataStore` Resource 骨架（持有所有配置表的 HashMap）
    - `pub fn load_all(data_dir: &Path) -> Result<DataStore>` — 空实现占位
    - 错误类型 `AssetError`（thiserror）

- 验收标准:
  - [ ] `cargo check -p slg-app -p slg-assets` 通过
  - [ ] `cargo build -p slg-app` 成功（生成 exe）
  - [ ] slg-app 启动后弹出空白窗口（可手动验证，不纳入 CI 自动化）
  - [ ] slg-app 不直接依赖 slg-core（通过 slg-engine 间接依赖）
- 依赖: M0-T7（需要 Plugin 骨架存在）
- 复杂度: 中

---

#### M0-T9: 集成验证与红线巡检
- 负责: `qa-engineer` + `arch-guardian`
- 目标: 全量构建+测试通过，红线巡检清零
- 具体内容:

  **qa-engineer：**
  - `tests/workspace_smoke.rs`：
    - 测试 slg-core hex 坐标基本运算（跨 crate 调用）
    - 测试 slg-save 容器常量正确
  - 全量验证：
    - `cargo fmt --check` 通过
    - `cargo clippy -- -D warnings` 通过
    - `cargo test --workspace` 通过
    - `cargo build --release` 通过
  - CI 本地复现：确认 `.github/workflows/ci.yml` 中所有 step 在本地可执行

  **arch-guardian：**
  - 红线巡检：
    - `grep -r "bevy" crates/slg-core crates/slg-data` 为空
    - `grep -rn "unwrap()" crates/ --include="*.rs"` — 非测试代码无 unwrap
    - 确认 Cargo.toml 依赖方向符合 §5.2
  - 输出审查结论

- 验收标准:
  - [ ] 全量 `cargo test --workspace` 通过
  - [ ] 红线巡检全部清零
  - [ ] arch-guardian 审查结论为"通过"
  - [ ] 所有 M0 验收标准（§1.2）逐条勾选完成
- 依赖: M0-T1 到 M0-T8 全部完成
- 复杂度: 低

---

## 4. 执行顺序

### 4.1 可并行的任务组

```
阶段 1（零依赖，立即启动）：
  ┌─ M0-T1  workspace 骨架创建           [qa-engineer]
  ├─ M0-T3  ECS 映射表 + 转换接口        [arch-guardian]
  └─ M0-T7  渲染/编辑器 crate 骨架       [render-engineer + editor-engineer]
      （T7 依赖 T1 的 Cargo.toml 存在，但可与 T1 同步进行——T1 先写 Cargo.toml，T7 写 src/）

阶段 2（依赖 T1 + T3）：
  ┌─ M0-T4  slg-data 类型骨架            [core-engineer]    ← 依赖 T3
  ├─ M0-T2  CI 流水线配置                [qa-engineer]      ← 依赖 T1
  └─ M0-T5  hex 网格数学模块             [core-engineer]    ← 依赖 T3
      （T4 和 T5 可由 core-engineer 在同一会话中串行完成）

阶段 3（依赖 T4）：
  └─ M0-T6  内容数据骨架                 [content-designer] ← 依赖 T4

阶段 4（依赖 T7）：
  └─ M0-T8  slg-app 入口 + slg-assets    [render-engineer + core-engineer] ← 依赖 T7

阶段 5（全部完成后）：
  └─ M0-T9  集成验证 + 红线巡检          [qa-engineer + arch-guardian]
```

### 4.2 关键路径

```
T3 (ECS映射, 0.5d) → T4 (slg-data, 0.5d) → T6 (RON数据, 0.5d)
                                                ↘
T1 (workspace, 0.5d) → T7 (骨架, 0.5d) → T8 (入口, 0.5d) → T9 (验证, 0.5d)
                        → T2 (CI, 0.5d) ──────────────────────↗
```

**关键路径**：T3 → T4 → T6 → T9（需等 content-designer 完成 RON 对齐）
**并行路径**：T1 → T2 → T9 可与关键路径并行

### 4.3 预估工期

| 阶段 | 任务 | 预估耗时 | 说明 |
|------|------|---------|------|
| 阶段 1 | T1 + T3 + T7 | 0.5 天 | T1/T3/T7 三路并行 |
| 阶段 2 | T4 + T5 + T2 | 0.5 天 | T4+T5 同 agent 串行，T2 并行 |
| 阶段 3 | T6 | 0.5 天 | content-designer 独立 |
| 阶段 4 | T8 | 0.5 天 | render + core 并行 |
| 阶段 5 | T9 | 0.5 天 | 验证 + 巡检 |
| **合计** | | **~2.5 天** | 含验收与返工余量 |

### 4.4 甘特图

```
Day 1 AM:  [T1: qa-engineer]  [T3: arch-guardian]  [T7: render+editor]
Day 1 PM:  [T1 完成→T2]       [T3 完成→T4+T5: core-engineer]
Day 2 AM:  [T2 继续]          [T4+T5 完成→T6: content-designer]  [T7 完成→T8: render+core]
Day 2 PM:  [T6 完成]          [T8 完成]
Day 3 AM:  [T9: qa+arch 集成验证]
Day 3 PM:  [验收 / 返工缓冲]
```

---

## 5. 跨 Agent 协作节点

### 5.1 T3 → T4/T5：架构接口传递

- `arch-guardian` 完成 T3（ECS 映射表 + 转换接口）后，产出写入 ARCHITECTURE.md
- `core-engineer` 开始 T4/T5 前必须先读更新后的 ARCHITECTURE.md §6.1（含新增 §6.8）
- **抄送**：arch-guardian 完成 T3 后，向 master-coordinator 报告，master-coordinator 通知 core-engineer 可以启动

### 5.2 T4 → T6：类型定义对齐

- `core-engineer` 完成 T4（slg-data 类型）后，所有配置表结构定义落地
- `content-designer` 开始 T6 前必须参考 slg-data 的类型定义来对齐 RON 字段
- **抄送**：core-engineer 完成 T4 后，向 master-coordinator 报告，master-coordinator 通知 content-designer 可以启动
- **协作细节**：core-engineer 需在完成报告中列出所有公共类型的字段清单，供 content-designer 直接参照

### 5.3 T7 → T8：Plugin 骨架依赖

- `render-engineer` 完成 slg-engine/slg-ui 的 Plugin 骨架后，slg-app 才能注册这些 Plugin
- `editor-engineer` 完成 slg-editor 的 Plugin 骨架后，slg-app 才能注册
- **并行可行**：T7 的 render 和 editor 两部分可并行，但 T8 需等 T7 全部完成

### 5.4 T9：联合验收

- `qa-engineer` 执行全量构建与测试
- `arch-guardian` 执行红线巡检
- 两者独立进行，结论汇总后由 master-coordinator 做最终 M0 闭环判定

---

## 6. 风险与缓解（M0 层面）

### R1: Bevy 版本未 pin（阻断 T1/T7/T8）

- **风险**：Bevy minor 版本间有破坏性变更（D20），不 pin 版本则 CI 不稳定
- **缓解**：
  - T1 中 qa-engineer 需确认当前最新稳定版（截至 2026-08-02 应为 Bevy 0.15.x）
  - Cargo.toml 中使用精确版本号（如 `bevy = "0.15.3"` 而非 `bevy = "0.15"`）
  - 如果最新版 API 与 ARCHITECTURE.md 设计有冲突，qa-engineer 上报 master-coordinator，由 arch-guardian 裁决
- **兜底**：如果 Bevy 0.15 有重大不兼容，降级到 0.14 最新版，更新 D20 记录

### R2: ECS 数据映射未定义（阻断 T4/T5）

- **风险**：不知道 tile/entity 放 Component 还是 Resource，core-engineer 无法设计 slg-data 类型
- **缓解**：T3 专门解决此问题。arch-guardian 在 M0 第一天完成 T3，产出明确的映射表
- **状态**：T3 已纳入 M0 计划，M0 结束前 R2 降级为"已解决"

### R3: 地图三形态转换接口未定义（阻断后续 T2/T3 V1 任务）

- **风险**：MapDocument ↔ World ↔ Save 的转换逻辑不明确
- **缓解**：T3 专门解决此问题。arch-guardian 在 T3 中补充伪代码签名和编辑器同步策略
- **状态**：T3 已纳入 M0 计划，M0 结束前 R3 降级为"已解决"。实际代码实现留到 M1

### R4: T2 依赖 T3 的 map/ 模块

- **风险**：render-engineer 需要 slg-core 的地图数据结构才能实现 Chunk 渲染
- **缓解**：M0 范围内 slg-engine 只做空壳 Plugin，不涉及实际 Chunk 渲染。T2（大地图渲染）是 M1 任务
- **M0 内处理**：T7 中 slg-engine 仅输出 Plugin 骨架 + 模块声明，实际渲染逻辑留 M1
- **M1 前置条件**：M1 启动前需确认 slg-core map/ 模块（hex + tile + chunk 数据结构）已完成

### R5: Bevy API 不确定导致 Plugin 骨架写法有误

- **风险**：Bevy 0.15 的 Plugin trait / App 构建 API 可能与预期不同
- **缓解**：
  - T1 中 qa-engineer 先验证 `bevy = "0.15.x"` 的基本编译
  - T7 中 render-engineer 先写最简 Plugin（空 build 方法），确认能编译后再扩展
  - 如果 Bevy API 与设计有重大差异，上报 master-coordinator 协调

### R6: core-engineer 负载集中（T4 + T5 + T8 部分）

- **风险**：core-engineer 需完成 slg-data 类型 + hex 模块 + slg-assets 加载器，任务量偏大
- **缓解**：
  - T4 和 T5 可在同一会话串行完成（类型定义 + hex 数学，逻辑独立）
  - T8 的 slg-assets 部分极轻量（仅 DataStore 骨架 + 空加载函数）
  - 如果时间不够，slg-assets 可推迟到 M1 首个迭代

### R7: 内容数据与类型定义双向依赖

- **风险**：content-designer 的 RON 文件需要 slg-data 类型来对齐字段，但类型定义也需要参考实际内容需求
- **缓解**：
  - T4 先定义类型骨架（字段基于 ARCHITECTURE §9.1 清单）
  - T6 中 content-designer 按类型骨架填写 RON 数据
  - 如果类型字段不够用，content-designer 向 core-engineer 提需求，经 arch-guardian 评审后增补
  - M0 范围内只放占位数据，不追求完整性

---

## 附录：M0 任务卡快速索引

| 卡号 | 标题 | 负责 | 依赖 | 状态 |
|------|------|------|------|------|
| M0-T1 | workspace 骨架创建 | qa-engineer | 无 | 待派发 |
| M0-T2 | CI 流水线配置 | qa-engineer | T1 | 待派发 |
| M0-T3 | ECS 数据映射表与转换接口 | arch-guardian | 无 | 待派发 |
| M0-T4 | slg-data 共享数据结构骨架 | core-engineer | T3 | 待派发 |
| M0-T5 | slg-core hex 网格数学模块 | core-engineer | T3 | 待派发 |
| M0-T6 | 内容数据骨架 | content-designer | T4 | 待派发 |
| M0-T7 | 渲染/编辑器 crate 骨架 | render + editor | T1 | 待派发 |
| M0-T8 | slg-app 入口 + slg-assets | render + core | T7 | 待派发 |
| M0-T9 | 集成验证与红线巡检 | qa + arch | T1-T8 | 待派发 |

---

*M0 完成后，项目状态应为：架构基线 ✅ / agent 团队 ✅ / workspace 骨架 ✅ / CI ✅ → 进入 M1（V1 核心循环）。*
