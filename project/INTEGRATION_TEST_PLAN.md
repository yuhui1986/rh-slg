# 《天下策》全项目端到端集成测试方案

> 版本：v1.0（2026-08-05）
> 架构基线：[ARCHITECTURE.md](../ARCHITECTURE.md) v1.4
> 范围：workspace 8 crate 跨模块交互、数据流、业务链路
> 定位：**仅集成测试**，不含单元测试、单函数测试

---

## 目录

1. [集成测试范围界定](#1-集成测试范围界定)
2. [核心集成业务链路梳理](#2-核心集成业务链路梳理)
3. [集成测试场景分类](#3-集成测试场景分类)
4. [高风险集成缺陷点清单](#4-高风险集成缺陷点清单)
5. [测试前置环境准备](#5-测试前置环境准备)
6. [自动化测试分层策略](#6-自动化测试分层策略)

---

## 1. 集成测试范围界定

### 1.1 纳入测试范围

以下模块/子系统的**跨 crate 交互、数据流转、接口契约**属于集成测试覆盖范围：

| 领域 | 涉及 Crate | 测试焦点 |
|------|-----------|---------|
| 地图三层模型转换 | slg-data ↔ slg-core ↔ slg-engine ↔ slg-save | MapDocument → Runtime World → SaveFile 双向转换数据零偏差 |
| Tick 全逻辑链路 | slg-core (clock/resource/ai/event) + slg-engine (systems) | GameTickSchedule 9 阶段顺序执行、状态流转正确性 |
| 铺路与领地连通 | slg-core (map/territory) + slg-data | Union-Find 占地校验、断连 BFS 分裂、飞地自动丢失 |
| 行军与寻路 | slg-core (map/pathfinding + military) + slg-data | Hex A* 寻路正确性、行军推进 tick 精度、路径阻断处理 |
| 战斗全流程 | slg-core (rule/combat + entity) + slg-data | 纯函数确定性模拟、战报生成、兵力结算、经验分配 |
| AI 完整决策链路 | slg-core (ai/utility + ai/diplomacy + ai/persona) | 三层错峰决策、效用评分排序、外交动作执行 |
| 存档读写往返 | slg-save (container/migration) + slg-data + slg-core | .slgmap/.slgsave 二进制容器读写、CRC 校验、版本迁移 |
| 编辑器双向切换 | slg-editor + slg-engine + slg-core + slg-data | 编辑器 ↔ 游玩模式 MapDocument/RuntimeWorld 转换、命令栈撤销重做 |
| Mod 数据加载合并 | slg-assets + slg-data + slg-core | RON 数据表优先级覆盖、冲突合并、热重载生效 |
| 迷雾视野 | slg-core (fog) + slg-engine (render/fog) | 视野计算、迷雾状态同步、Chunk 迷雾数据一致性 |
| 事件链系统 | slg-core (event/chain + trigger + effect) | 触发条件评估、效果执行、分支导航、沙盒事件调度 |
| 程序化生成 | slg-core (gen/terrain + resource + spawn + validate) | 种子确定性、连通性校验、出生点公平性 |

### 1.2 排除范围

| 排除项 | 理由 |
|--------|------|
| 纯 UI 渲染像素校验 | egui 面板布局、字体渲染、颜色像素级对比不属于逻辑集成测试 |
| 单函数单元逻辑 | 如 `HexCoord::distance()` 返回值、`UnionFind::find()` 单调用——这些是单元测试 |
| 美术资源静态校验 | 纹理尺寸、音频采样率等——仅校验数据加载成功，不校验画面表现 |
| GPU 特定行为 | shader 编译、GPU 内存分配——依赖硬件环境，不适合 CI |
| Steam SDK 功能 | 成就解锁、云存档同步——需 Steam 运行时，CI 不可用 |

---

## 2. 核心集成业务链路梳理

### 链路 ① 基础开局链路

**覆盖**：地图加载 → 运行时初始化 → 多 tick 推演全系统联动

```
slg-save::load_map_from_file(.slgmap)
  → slg-data::MapDocument
    → slg-core::map::loader::load_map_to_world(doc)
      → slg-engine::SlgEnginePlugin 注册 Chunk Entity
        → slg-app::setup_game 初始化 FactionStore/TerritoryGraph/FogOfWar
          → slg-core::clock::advance_clock × 100 次
            → 每 tick 执行 GameTickSchedule 9 阶段
```

**校验点**：
- MapDocument 各层（地形 RLE、资源 BTree、实体 BTree）完整展开为 Chunk 定长数组
- Chunk Entity 数量 = `ceil(map_w/32) × ceil(map_h/32)`
- 100 tick 后 `GameClock.current_tick == 100`
- 资源产出阶段各势力 `FactionResources` 按公式增长
- AI 势力在 `tick % 10 == slot` 时产生 `PlayerCommand`
- 迷雾在 TickEnd 阶段正确刷新视野范围

### 链路 ② 玩家操作链路

**覆盖**：占地 → 征兵建造 → 行军 → 战斗 → 战报 → 资源变更

```
slg-engine::HexClickEvent { coord, world_pos }
  → slg-app 识别为 OccupyTile 指令
    → slg-core::resource::CommandQueue::push(PlayerCommand::OccupyTile)
      → TickStart 阶段注入
        → slg-core::map::territory::TerritoryManager::can_occupy(coord, faction)
          → 校验：六邻有己方格 + Union-Find 连通主城
          → territory::occupy(coord, faction) → union 合并
            → ResourceProduction 阶段：新地块产出计入 FactionResources
              → BuildQueue 阶段：建造指令消耗资源
                → Recruitment 阶段：征兵扣资源、增加 troops
                  → MarchAdvance 阶段：
                    slg-core::map::pathfinding::hex_a_star(start, end, terrain_costs)
                    → 预计算路径写入 ArmyTroops::MarchPath
                    → 每 tick 推进 path_index
                      → 抵达目标格 → CombatResolution 阶段：
                        slg-core::rule::combat::simulate(CombatInput)
                        → CombatReport 写入 CombatReportStore
                        → 兵力/经验写回 ECS Entity
                        → 战败方城池易手 → TerritoryUpdate
```

**校验点**：
- 非相邻/不可通行格 `can_occupy` 返回 false
- 占领后 Union-Find `connected(tile, main_city)` 为 true
- 行军路径每 tick 推进一格，`arrive_tick` 精确匹配
- 战斗同种子 100 次结果完全一致（确定性）
- 战报中兵力损耗 + 剩余兵力 = 战前总兵力（守恒）
- 城池易手后原势力 Union-Find 断连分裂正确触发

### 链路 ③ AI 势力完整链路

**覆盖**：三层错峰决策 → 外交 → 自主扩张 → 战斗

```
slg-core::clock::should_ai_decide(tick, slot) == true
  → slg-core::ai::tick_ai(faction_id, faction, slot, tick, ...)
    → Layer 0 硬规则：资源 < 阈值 → 停止扩张
    → Layer 1 战略层（tick % 50 == 0）：
      → 区域价值评估 → 主攻方向
      → slg-core::ai::diplomacy::DiplomacySystem 评估威胁
        → 好感度 < 阈值 → DeclareWar / > 阈值 → Alliance
    → Layer 2 战术层（tick % 10 == 0）：
      → slg-core::ai::utility::generate_candidates()
        → score_occupy / score_defend / score_recruit 排序
        → 取 Top-N 候选动作
    → Layer 3 执行层：
      → execute_action() → CommandQueue::push(PlayerCommand)
        → 后续由 TickStart 注入，与玩家指令同管线处理
```

**校验点**：
- 每 tick 仅 `tick % 10 == slot` 的势力执行决策，其余势力 `CommandQueue` 无新增
- 好战人格（如魏 `AGGRESSIVE`）的 `score_occupy` 显著高于保守人格
- 外交动作正确修改 `FactionState.diplomacy.relations`
- AI 征兵/建造后资源扣减正确
- AI 部队行军路径合法（不穿越敌方领地、不渡无渡口河流）
- AI 战斗结算后战报写入 `CombatReportStore`

### 链路 ④ 存档往返链路

**覆盖**：Runtime World → SaveFile → 磁盘 → SaveFile → Runtime World 数据零偏差

```
slg-core 运行时状态
  → slg-app::world_to_save(world, map_hash)
    → 遍历 Chunk Entity → 计算 tile_delta（对比原始 MapDocument）
    → 快照 FactionState / EntitySnapshot / EventLog
    → slg-data::SaveFile { map_ref, tick, faction_states, tile_delta, event_log }
      → slg-save::save_save_to_file(save, path)
        → 写入 .slgsave 容器（Magic + TOC + bincode/zstd sections + CRC32）
          → slg-save::load_save_from_file(path)
            → CRC32 校验 → 版本迁移链（如需要）→ SaveFile
              → slg-app::load_save_to_world(save, doc, world)
                → load_map_to_world(doc) → 应用 tile_delta → 恢复实体/势力
```

**校验点**：
- 保存前后 `map_hash` 一致
- 加载后 `tile_delta` 逐项应用，地形/归属/等级与保存前逐格一致
- `FactionState` 资源/外交/武将/部队快照完全还原
- `EventLog` 已触发事件不重放
- CRC32 篡改检测：修改文件 1 字节 → 加载失败 + 明确错误
- 旧版本存档通过迁移链自动升级到最新版本

### 链路 ⑤ 编辑器双向切换链路

**覆盖**：MapDocument → RuntimeWorld → 游玩 → 增量合并回 MapDocument

```
编辑器模式：
  slg-editor::EditorState { doc: MapDocument, history: CommandHistory }
    → 用户操作 → EditorCommand::execute(doc) → undo_stack.push
    → 切换至游玩模式：
      slg-app::load_map_to_world(doc, world)
        → MapDocument 完整展开为 Runtime World
        → 游玩若干 tick...
    → 切回编辑器：
      slg-app::world_to_save(world, map_hash) → tile_delta
      → 将 tile_delta 合并回 MapDocument 对应层
      → 丢弃运行时临时实体（部队/战斗状态）
      → EditorState.doc 更新，history 清空
```

**校验点**：
- 编辑器 → 游玩：Chunk Entity 地形/归属与 MapDocument RLE 解码一致
- 游玩 → 编辑器：tile_delta 合并后 MapDocument 反映游玩期间地形/归属变更
- 游玩期间的部队位置、战斗状态不污染 MapDocument
- 编辑器命令栈 undo/redo 在切换后重置（V1 策略）
- 高频切换（10 次往返）数据无累积偏差

### 链路 ⑥ Mod 数据合并链路

**覆盖**：内置 data + mod 目录 → 优先级覆盖 → 热重载

```
slg-assets::load_data_tables(base_path: "assets/data/")
  → 加载 generals.ron / skills.ron / unit_types.ron / terrain_types.ron
     / buildings.ron / events.ron / global_params.ron
  → slg-assets::merge_mod_data(mod_paths: ["mods/mod_a/data/", "mods/mod_b/data/"])
    → 按 mod.toml priority 排序
    → 同 ID 记录整条覆盖；"+" 后缀文件追加；__delete 标记删除
    → 冲突日志记录
      → slg-core 使用合并后数据表初始化
        → 开发模式 AssetPlugin::watch_for_changes
          → 修改 RON 文件 → 热重载 → 数据表更新（存档不受影响）
```

**校验点**：
- Mod 同 ID 武将覆盖基础属性（如修改曹操攻击力后战斗结算使用新值）
- "+" 追加文件的新记录可被正确加载
- `__delete` 标记的记录在合并后数据表中不存在
- 多层 Mod 覆盖优先级正确（user > mod_b > mod_a > base）
- 热重载后新数据立即生效，无需重启
- 缺失必填字段时加载失败 + 明确错误信息

### 链路 ⑦ 异常容错链路

**覆盖**：各类异常场景的优雅降级

| 异常场景 | 触发方式 | 期望行为 |
|----------|---------|---------|
| 地图哈希不匹配 | 修改 .slgmap 后加载旧 .slgsave | 警告 + 提供选项（继续/取消），不崩溃 |
| tick 资源不足 | 势力资源耗尽 | 建造/征兵队列暂停，不产生负资源 |
| 行军路径阻断 | 行军途中地块被敌方占领 | 重新寻路或原地等待，不穿越敌领 |
| 战斗兵力归零 | 战斗结算后部队 troops=0 | Entity 销毁，不残留幽灵部队 |
| AI 无资源 | AI 资源 < 7 天消耗 | Layer 0 硬规则触发，停止建造/扩张 |
| tile_delta 超 30% | 大规模地形变更 | 自动触发全量快照，delta 重置为空 |
| 存档版本过低 | 加载旧版 .slgsave | 自动通过迁移链升级，数据完整 |
| 数据表 ID 不存在 | Mod 引用了删除的武将 ID | 加载阶段报错 + 指明缺失 ID，不 panic |

---

## 3. 集成测试场景分类

### 3.1 正向完整全链路场景

#### S1.1 标准剧本「三国鼎立」完整推演

- **前置**：加载 `assets/data/scenarios/sanguo_dl/` 剧本配置
- **流程**：
  1. 加载内置 .slgmap 地图 → 初始化 Runtime World
  2. 初始化 6 个势力（玩家 + 魏/蜀/吴/辽东/南中 5 AI）
  3. 分配 AI 决策槽位（随机 seed 固定）
  4. 执行 500 个完整 tick（覆盖 AI 战略层 10 次决策周期）
- **校验**：
  - 所有势力 `FactionResources` 非负
  - 至少发生 1 次 AI 间战斗（CombatReportStore 非空）
  - 至少 1 个 AI 势力扩张了领地（Union-Find 块大小增长）
  - 迷雾视野正确限制玩家可见范围
  - 事件链至少触发 1 个（如时间触发类事件）
  - tick 耗时 < 10ms/tick（性能预算）

#### S1.2 编辑器创建剧本 → 游玩通关

- **前置**：程序化生成 256×256 地图（固定种子 42）
- **流程**：
  1. 编辑器中配置 3 个势力、胜利条件（占领洛阳）
  2. 保存为 .slgmap
  3. 切换游玩模式 → 加载地图 → 推演至某势力达成胜利条件
- **校验**：
  - 保存的 .slgmap 通过 `validate_for_save_full()` 全量校验
  - 加载后势力配置、胜利条件与编辑器设置一致
  - 胜利触发时 `GameOver` 状态正确激活

### 3.2 跨模块异常交互场景

#### S2.1 下游系统数据缺失

- **流程**：
  1. 构造缺少 `terrain_types.ron` 的数据目录
  2. 尝试 `slg-assets::load_data_tables()`
- **校验**：返回明确错误（非 panic），指明缺失文件路径

#### S2.2 寻路无可行路径

- **流程**：
  1. 构造地图：起点和终点被不可通行地形（水域/山脉）完全隔离
  2. 调用 `hex_a_star(start, end, terrain_costs)`
- **校验**：返回 `None`/`Err`，行军指令被拒绝，部队原地不动

#### S2.3 数据表 ID 不存在

- **流程**：
  1. Mod 中引用 `general_id: "cao_cao_fake"` 但 generals.ron 无此记录
  2. 加载合并后数据表 → 初始化剧本
- **校验**：加载阶段报错，指明缺失 ID 和引用来源文件

#### S2.4 Mod 字段冲突

- **流程**：
  1. Mod A 和 Mod B 同时覆盖 `general_id: "cao_cao"` 的不同字段
  2. 按优先级合并后检查最终值
- **校验**：高优先级 Mod 的字段值生效；冲突日志记录两个 Mod 的覆盖关系

#### S2.5 存档版本过低触发自动迁移

- **流程**：
  1. 使用 v1 格式夹具 .slgsave（缺少 v2 新增字段）
  2. `slg-save::load_save_from_file()`
- **校验**：
  - 自动调用 `migrate_v1_to_v2()`
  - 新增字段使用 `#[serde(default)]` 填充
  - 迁移后数据与直接创建 v2 存档一致（insta 快照对比）

### 3.3 数据一致性校验场景

#### S3.1 Tick 资源收支守恒

- **流程**：
  1. 记录 tick N 所有势力 `FactionResources` 总和
  2. 执行 1 个完整 tick
  3. 计算：产出（地块产出 + 建筑加成）- 消耗（征兵 + 建造 + 维护费）
- **校验**：`tick_N+1 资源 == tick_N 资源 + 产出 - 消耗`，精确到个位

#### S3.2 Union-Find 飞地断连自动丢失

- **流程**：
  1. 势力 A 占领一条线形领地（10 格），主城在端点
  2. 敌方占领中间 1 格切断连接
  3. `TerritoryManager::handle_disconnect()` 触发 BFS 分裂
  4. 等待宽限期（10 tick）
- **校验**：
  - 断连后产生 2 个连通分量
  - 不与主城相连的分量标记为飞地
  - 宽限期结束后飞地 `owner` 归零（自动丢失）
  - 与主城相连的分量保持正常

#### S3.3 战斗兵力损耗精准

- **流程**：
  1. 构造 `CombatInput`：攻方 10000 步卒 vs 守方 8000 弓兵
  2. 固定种子执行 `simulate()` 100 次
- **校验**：
  - 100 次 `CombatReport` 完全一致（逐字段对比）
  - 每回合 `round.attacker_losses + round.defender_losses` 合理
  - `final_troops == initial_troops - total_losses`
  - 克制系数正确应用（步 vs 弓：攻方 ×0.85，守方 ×1.15）

#### S3.4 存档 delta 往返无数据丢失

- **流程**：
  1. 加载地图 → 游玩 200 tick（期间占领 50 格、建造 3 座建筑）
  2. 保存为 .slgsave（含 tile_delta）
  3. 重新加载 .slgmap + .slgsave → 应用 tile_delta
- **校验**：
  - 还原后每格 terrain/owner/level 与保存前逐格一致
  - 建筑位置/等级一致
  - 势力资源/兵力一致

#### S3.5 种子驱动地图完全确定性

- **流程**：
  1. 使用种子 12345 调用程序化生成管线 3 次
  2. 对比 3 次输出的 MapDocument
- **校验**：
  - 地形层 RLE 数据逐字节一致
  - 资源层 BTreeMap 逐条目一致
  - 出生点位置完全相同
  - 河流走向完全相同

### 3.4 时序/并发场景

#### S4.1 多部队同步行军

- **流程**：
  1. 玩家同时派出 5 支部队向不同目标行军
  2. 路径有交叉但目标不同
- **校验**：
  - 每支部队独立按 `MarchPath` 推进，互不干扰
  - 交叉点不发生碰撞（不同部队可经过同一格）
  - 各部队 `arrive_tick` 独立计算且正确

#### S4.2 多 AI 同 tick 错峰决策

- **流程**：
  1. 6 个势力，slot 0~5 随机分配
  2. 执行 100 tick，记录每个势力的决策 tick 集合
- **校验**：
  - 势力 i 仅在 `tick % 10 == slot[i]` 时产生 `PlayerCommand`
  - 每 tick 最多 1 个势力执行决策（slot 不重复时）
  - 100 tick 内每个势力恰好决策 10 次

#### S4.3 大量地块同时占领

- **流程**：
  1. 构造场景：5 个势力在同一 tick 各提交 10 个 OccupyTile 指令
  2. 通过 `CommandQueue` 批量处理
- **校验**：
  - 所有合法占领成功执行
  - 冲突格（两个势力同 tick 争夺）按指令队列顺序处理，先到先得
  - Union-Find 在批量操作后保持一致性

#### S4.4 大批量战斗分帧结算

- **流程**：
  1. 构造 25 支部队同时抵达目标格
  2. CombatResolution 阶段限流：每 tick ≤ 20 场
- **校验**：
  - 前 20 场在当前 tick 结算，剩余 5 场延迟至下一 tick
  - 所有战斗最终都结算完毕
  - 单 tick 战斗结算总耗时 < 10ms（20 场 × 0.25ms + 余量）

#### S4.5 编辑器批量填充图层

- **流程**：
  1. 编辑器中使用 FloodFill 填充 100×100 区域为新地形
  2. 执行 `AreaFillCommand::execute()`
- **校验**：
  - 填充区域所有格地形一致
  - 填充边界精确（不溢出到非目标区域）
  - 撤销后完全恢复原始地形
  - 单次填充耗时 < 500ms

### 3.5 边界场景

#### S5.1 两种地图尺寸

| 尺寸 | 格数 | Chunk 数 | 校验点 |
|------|------|---------|--------|
| 256×256 | 65,536 | 64 | 生成 < 5s，tick < 10ms，内存 < 50MB |
| 512×512 | 262,144 | 256 | 生成 < 15s，tick < 10ms，内存 < 100MB |

#### S5.2 Tick 倍速 ×3 运行

- **流程**：设置 `GameClock.speed = Speed::X3`，运行 100 渲染帧
- **校验**：
  - 每渲染帧推进 3 个逻辑 tick（accumulator 正确累加）
  - 300 个逻辑 tick 全部执行完毕
  - 无 tick 被跳过或重复执行

#### S5.3 满地块占领

- **流程**：单势力占领全图所有可通行格
- **校验**：
  - Union-Find 只有一个连通分量
  - `can_occupy` 对所有格返回 false（无空/敌格）
  - 资源产出达到理论最大值

#### S5.4 单势力全部城池丢失

- **流程**：通过战斗让某 AI 势力失去所有城池
- **校验**：
  - `FactionEliminated` 事件触发
  - 该势力领地变为无主（owner = 0）
  - 该势力部队销毁或变为中立
  - 其他势力外交关系清理该势力条目
  - 事件链可响应 `FactionEliminated` 触发器

#### S5.5 Mod 多层覆盖数据

- **流程**：
  1. base: `generals.ron` 定义曹操 atk=80
  2. mod_a (priority=10): 覆盖 atk=90
  3. mod_b (priority=20): 覆盖 atk=100
  4. user 目录: 覆盖 atk=110
- **校验**：最终加载值 atk=110（最高优先级生效）

#### S5.6 存档接近 30% 变更阈值

- **流程**：
  1. 加载 256×256 地图（65,536 格）
  2. 占领/修改恰好 19,660 格（29.99%）→ 保存 → 检查 delta 模式
  3. 再修改 1 格（30.00%）→ 保存 → 检查是否触发全量快照
- **校验**：
  - 29.99% 时保存为增量 delta
  - 30.00% 时自动合并为全量快照，delta 重置为空
  - 两种方式加载后数据一致

### 3.6 模式切换场景

#### S6.1 游玩 ↔ 编辑器高频切换

- **流程**：
  1. 编辑器修改地形 → 切换游玩 → 运行 10 tick
  2. 切回编辑器 → 修改归属 → 切换游玩 → 运行 10 tick
  3. 重复 10 次
- **校验**：
  - 每次切换数据完整转换，无累积偏差
  - 最终 MapDocument 包含所有编辑器修改 + 游玩期间变更
  - 无内存泄漏（切换 10 次后内存增长 < 10%）

#### S6.2 暂停/恢复 Tick

- **流程**：
  1. 运行 50 tick → 设置 `Speed::Paused`
  2. 暂停期间提交 5 个 `PlayerCommand` 到 `CommandQueue`
  3. 恢复 `Speed::X1`
- **校验**：
  - 暂停期间 `current_tick` 不变
  - 暂停期间指令累积但不执行
  - 恢复后 5 个指令在下一个 TickStart 阶段全部注入并执行

#### S6.3 多存档槽交替读写

- **流程**：
  1. 游玩至 tick 100 → 保存到 slot_1
  2. 继续游玩至 tick 200 → 保存到 slot_2
  3. 加载 slot_1 → 校验 tick == 100
  4. 继续游玩至 tick 150 → 保存到 slot_1（覆盖）
  5. 加载 slot_2 → 校验 tick == 200
  6. 加载 slot_1 → 校验 tick == 150
- **校验**：各存档槽独立，覆盖不影响其他槽位

---

## 4. 高风险集成缺陷点清单

基于架构分析，以下耦合点最容易出现集成缺陷，需重点覆盖：

| # | 风险点 | 风险等级 | 涉及 Crate | 缺陷模式 | 对应测试场景 |
|---|--------|---------|-----------|---------|-------------|
| R1 | **MapDocument ↔ Runtime World 转换精度** | 🔴 高 | slg-data ↔ slg-core ↔ slg-engine | RLE 解码偏移导致地形错位；BTree 稀疏层实体漏转；Chunk 边界格索引计算错误（off-by-one） | S1.1, S3.4, S5.1, S6.1 |
| R2 | **tile_delta 计算与应用不对称** | 🔴 高 | slg-core ↔ slg-save ↔ slg-app | 保存时 diff 算法漏算变更格；加载时 delta 应用顺序错误导致覆盖；地形/归属/等级三层 delta 不同步 | S3.4, S4.5, S5.6 |
| R3 | **Tick 流水线阶段间状态依赖** | 🔴 高 | slg-core (全 module) | 资源产出未到账即进入建造阶段扣资源失败；行军到达与战斗结算同 tick 顺序不确定；领地更新滞后于 AI 决策 | S1.1, S3.1, S4.3 |
| R4 | **Union-Find 断连分裂竞态** | 🟡 中 | slg-core (map/territory) | 同 tick 多格被夺取时 BFS 分裂不完整；飞地宽限期计时器未正确初始化；根节点变更后缓存未失效 | S3.2, S4.3, S5.4 |
| R5 | **AI 错峰决策与共享状态** | 🟡 中 | slg-core (ai + resource) | AI 读取上一势力刚修改的 CommandQueue 产生依赖；外交关系修改非原子操作导致中间态被读 | S4.2, S3.3 |
| R6 | **存档容器 CRC32 与版本迁移链** | 🟡 中 | slg-save (container + migration) | 迁移函数组合后行为不等价于直接创建最新版；CRC32 计算范围不包含新增 section | S2.5, S3.4 |
| R7 | **Mod 合并后数据表交叉引用完整性** | 🟡 中 | slg-assets + slg-data | Mod 删除了被其他表引用的 ID；覆盖后技能公式参数与新武将五维不匹配 | S2.3, S2.4, S5.5 |
| R8 | **编辑器命令栈与 MapDocument 状态同步** | 🟡 中 | slg-editor (command + tool) | 连续笔刷合并后 undo 不完全；FloodFill 边界条件导致溢出后无法撤销 | S4.5, S6.1 |
| R9 | **迷雾分帧重算跨 tick 一致性** | 🟢 低 | slg-core (fog) + slg-engine | 大范围重算（迁城）分帧期间部分 Chunk 已更新部分未更新，渲染出现闪烁边界 | S1.1 |
| R10 | **事件链分支导航与存档兼容性** | 🟢 低 | slg-core (event) + slg-save | 事件链执行到中间节点时保存，加载后从正确节点继续；分支选择后旧分支状态清理 | S1.1, S3.4 |

---

## 5. 测试前置环境准备

### 5.1 Headless 测试环境

**目标**：CI 中无 GPU/无窗口运行全部集成测试。

| 组件 | 方案 |
|------|------|
| Bevy App | 使用 `MinimalPlugins` 替代 `DefaultPlugins`，不创建窗口 |
| 渲染 | 不注册 `ChunkRenderPlugin`；仅注册 `ClockPlugin` + 逻辑系统 |
| 输入 | 通过代码直接发送 `HexClickEvent`，不依赖窗口事件循环 |
| 资源加载 | `AssetPlugin` 配置为 headless 模式，从 `assets/` 目录加载 |
| 时间 | 使用 `TimeUpdateStrategy::ManualDuration` 手动推进，不依赖系统时钟 |

**测试入口结构**：

```
tests/
├── common/
│   ├── mod.rs              # 共享夹具与辅助函数
│   ├── headless_app.rs     # headless Bevy App 构建器
│   ├── fixtures.rs         # 测试地图/存档/Mod 数据构造器
│   └── assertions.rs       # 自定义断言宏（势力一致性、地图对比等）
├── integration_*.rs        # 各链路集成测试文件
```

### 5.2 测试地图夹具

| 夹具名 | 规格 | 用途 |
|--------|------|------|
| `tiny_16x16` | 16×16 hex，1 Chunk | 快速单元测试替代，验证基本转换 |
| `small_64x64` | 64×64 hex，4 Chunk | 寻路、领地、战斗基础场景 |
| `medium_256x256` | 256×256 hex，64 Chunk | 标准剧本推演、性能基准 |
| `large_512x512` | 512×512 hex，256 Chunk | 大地图边界、内存压力 |
| `islands_64x64` | 64×64，多水域隔离陆地 | 寻路无路径、飞地断连 |
| `corridor_64x64` | 64×64，单格宽通道连接两区域 | 铺路校验、 choke point 战斗 |

**构造方式**：通过代码生成 `MapDocument`（非读取文件），确保测试自包含：

```rust
// 伪代码示例
fn build_test_map(w: u32, h: u32, seed: u64) -> MapDocument {
    // 使用 slg-core::gen 管线生成确定性地图
    // 或手动构造 RLE 地形层 + BTree 资源/实体层
}
```

### 5.3 测试 Mod 数据

在 `tests/fixtures/mods/` 下构造：

| 目录 | 内容 | 用途 |
|------|------|------|
| `mod_override/` | `generals.ron` 覆盖曹操 atk | 基础覆盖测试 |
| `mod_append/` | `+generals.ron` 追加自定义武将 | 追加模式测试 |
| `mod_delete/` | `generals.ron` 含 `__delete` 标记 | 删除模式测试 |
| `mod_conflict_a/` + `mod_conflict_b/` | 同 ID 不同值，不同 priority | 优先级冲突测试 |
| `mod_broken/` | 引用不存在 ID 的 skills.ron | 异常加载测试 |

### 5.4 存档版本迁移夹具

在 `tests/fixtures/saves/` 下维护：

| 文件 | 版本 | 用途 |
|------|------|------|
| `save_v1.slgsave` | v1 | 迁移链起点测试 |
| `save_v2.slgsave` | v2 | 跨版本迁移测试 |
| `save_latest.slgsave` | 当前版本 | 往返基准测试 |
| `save_corrupted_crc.slgsave` | 当前版本 + 篡改字节 | CRC 检测测试 |

**纪律**：旧版本夹具永久保留，不随版本升级更新（参考 ARCHITECTURE §11.5 第 3 条）。

### 5.5 随机种子固定方案

| 场景 | 种子策略 |
|------|---------|
| 地图生成 | `ChaCha12Rng::seed_from_u64(42)` — 固定种子 |
| 战斗模拟 | `seed = hash(attacker_id, defender_id, tile_key, tick)` — 架构定义 |
| AI 决策 | `ChaCha12Rng::seed_from_u64(tick ^ faction_slot)` — 确定性但每 tick 不同 |
| 事件链 | `seed = tick ^ node_index ^ hash(chain_id)` — 架构定义 |
| 测试重复性 | 每个测试用例硬编码种子，确保 CI 结果可复现 |

---

## 6. 自动化测试分层策略

### 6.1 测试层级总览

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 4: CI 门禁（GitHub Actions）                          │
│  fmt + clippy + 依赖方向 + 基准回归 + 全量集成测试            │
├─────────────────────────────────────────────────────────────┤
│  Layer 3: tests/ 顶层跨 crate 集成测试                       │
│  全链路场景、跨 crate 数据流、模式切换                        │
│  依赖：slg-app（作为聚合入口）+ 所有 crate                    │
├─────────────────────────────────────────────────────────────┤
│  Layer 2: slg-core 快照回归 + 性能基准                       │
│  insta 快照（地图生成、战斗结果）                             │
│  criterion 基准（tick 耗时、寻路耗时、战斗耗时）              │
├─────────────────────────────────────────────────────────────┤
│  Layer 1: slg-engine headless 冒烟测试                       │
│  启动不崩、Chunk 数正确、系统注册完整                         │
├─────────────────────────────────────────────────────────────┤
│  Layer 0: 各 crate 单元测试（已有 392+，不在本方案范围）       │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 Layer 3：tests/ 顶层跨 crate 集成测试

**位置**：`tests/` 目录（workspace 根）

**Cargo.toml 配置**：

```toml
# workspace 根 Cargo.toml 中
[dev-dependencies]
slg-data = { path = "crates/slg-data" }
slg-core = { path = "crates/slg-core" }
slg-save = { path = "crates/slg-save" }
slg-assets = { path = "crates/slg-assets" }
slg-engine = { path = "crates/slg-engine" }
slg-editor = { path = "crates/slg-editor" }
slg-app = { path = "crates/slg-app" }
bevy = { version = "0.15", default-features = false }
proptest = "1"
insta = "1"
```

**测试文件规划**：

| 文件 | 覆盖链路 | 场景编号 |
|------|---------|---------|
| `tests/integration_startup.rs` | 链路① 基础开局 | S1.1, S5.1 |
| `tests/integration_player_ops.rs` | 链路② 玩家操作 | S3.1, S3.2, S3.3, S4.1 |
| `tests/integration_ai.rs` | 链路③ AI 决策 | S4.2, S5.4 |
| `tests/integration_save_load.rs` | 链路④ 存档往返 | S3.4, S5.6, S2.5 |
| `tests/integration_editor.rs` | 链路⑤ 编辑器切换 | S1.2, S6.1, S4.5 |
| `tests/integration_mod.rs` | 链路⑥ Mod 合并 | S2.3, S2.4, S5.5 |
| `tests/integration_errors.rs` | 链路⑦ 异常容错 | S2.1, S2.2, S5.3 |
| `tests/integration_determinism.rs` | 确定性校验 | S3.5, S3.3 |
| `tests/integration_concurrency.rs` | 时序/并发 | S4.3, S4.4 |
| `tests/integration_mode_switch.rs` | 模式切换 | S6.2, S6.3 |

**测试命名规范**：

```rust
#[test]
fn test_{链路编号}_{场景描述}_{期望结果}() {
    // 例：test_chain2_occupy_adjacent_updates_union_find()
    // 例：test_chain4_save_load_roundtrip_zero_diff()
    // 例：test_chain7_hash_mismatch_warns_not_crashes()
}
```

### 6.3 Layer 2：slg-core 快照回归测试

**位置**：`crates/slg-core/tests/`

| 测试 | 手段 | 校验内容 |
|------|------|---------|
| 地图生成快照 | `insta::assert_snapshot!` | 同种子 256×256 地图逐格地形/资源/出生点 |
| 战斗结果快照 | `insta::assert_debug_snapshot!` | 固定输入 CombatInput → CombatReport 全字段 |
| 经济结算快照 | `insta::assert_snapshot!` | 100 tick 各势力资源序列 |
| AI 决策快照 | `insta::assert_debug_snapshot!` | 固定局面下 AI 选择的 Top-5 动作 |

**快照更新纪律**：
- 数值调整（如战斗公式参数变更）→ 主动更新快照 + PR 说明
- 非预期变更 → 阻断合并，必须排查原因

### 6.4 Layer 2：criterion 性能基准

**位置**：`crates/slg-core/benches/`

| 基准名 | 测量目标 | 性能预算（ARCHITECTURE §12） | 阻断阈值 |
|--------|---------|---------------------------|---------|
| `bench_tick_full` | 单 tick 完整 9 阶段（256² 地图） | < 10ms | 回归 > 10% |
| `bench_combat_simulate` | 单场战斗模拟（3v3 满战法） | < 0.25ms | 回归 > 10% |
| `bench_hex_a_star` | 跨图寻路（256² 对角线） | < 5ms | 回归 > 10% |
| `bench_map_generation` | 256² 地图完整生成 | < 5s | 回归 > 10% |
| `bench_territory_occupy` | 单次占地 + Union-Find 更新 | < 0.1ms | 回归 > 10% |
| `bench_fog_recalc` | 全图迷雾重算（256²） | < 200ms | 回归 > 10% |
| `bench_save_delta` | 增量存档保存（1000 格变更） | < 100ms | 回归 > 10% |

### 6.5 Layer 1：slg-engine headless 冒烟测试

**位置**：`crates/slg-engine/tests/`

| 测试 | 校验内容 |
|------|---------|
| `headless_startup` | `MinimalPlugins` + `SlgEnginePlugin` 启动不 panic |
| `chunk_entity_count` | 256² 地图 → 64 个 Chunk Entity |
| `clock_plugin_tick` | `GameClockResource` 在手动时间推进下正确累加 |
| `hex_pick_roundtrip` | `hex_world_position` → `world_to_hex` 往返坐标一致 |

### 6.6 Layer 4：CI 门禁规则

**位置**：`.github/workflows/ci.yml`

```
Job 1: lint
  ├── cargo fmt --check
  ├── cargo clippy -- -D warnings
  └── 依赖方向检查：grep -r "bevy" crates/slg-core crates/slg-data → 必须为空

Job 2: test
  ├── cargo test --workspace（含 Layer 0 单元测试 + Layer 1 冒烟）
  ├── cargo test --test integration_*（Layer 3 集成测试）
  └── cargo test --package slg-core --test snapshot_*（Layer 2 快照回归）

Job 3: bench-regression
  ├── cargo bench --package slg-core（Layer 2 性能基准）
  └── 对比 baseline，回归 > 10% 阻断

Job 4: build
  └── cargo build --release
```

**CI 触发规则**：
- PR → 全部 4 个 Job
- push to main → Job 1 + Job 2 + Job 4
- 每日定时 → Job 3（性能基准耗时较长）

**门禁阻断条件**：

| 条件 | 阻断级别 |
|------|---------|
| `cargo fmt --check` 失败 | PR 阻断 |
| `cargo clippy -- -D warnings` 有 warning | PR 阻断 |
| 依赖方向检查发现 bevy 在 core/data 中 | PR 阻断 |
| 任何单元测试/集成测试失败 | PR 阻断 |
| insta 快照不匹配 | PR 阻断 |
| 性能基准回归 > 10% | PR 警告（不阻断，需人工确认） |
| 非测试代码存在 `unwrap()` | PR 阻断 |

### 6.7 测试执行时间预算

| 层级 | 目标时间 | 策略 |
|------|---------|------|
| Layer 0 单元测试 | < 30s | 并行执行 |
| Layer 1 headless 冒烟 | < 10s | 无渲染 |
| Layer 2 快照回归 | < 20s | insta 对比 |
| Layer 3 集成测试 | < 120s | 小地图夹具为主，256² 仅 S1.1 |
| Layer 2 性能基准 | < 300s | 每日定时，非 PR 必须 |
| **PR 总计** | **< 3 分钟** | Layer 0-3 |

---

## 附录 A：测试场景与链路覆盖矩阵

| 场景 | 链路①开局 | 链路②玩家 | 链路③AI | 链路④存档 | 链路⑤编辑器 | 链路⑥Mod | 链路⑦异常 |
|------|:---------:|:---------:|:-------:|:---------:|:-----------:|:--------:|:---------:|
| S1.1 标准剧本推演 | ✅ | ✅ | ✅ | | | | |
| S1.2 编辑器创建→游玩 | ✅ | | | | ✅ | | |
| S2.1 数据缺失 | | | | | | | ✅ |
| S2.2 寻路无路径 | | ✅ | | | | | ✅ |
| S2.3 ID 不存在 | | | | | | ✅ | ✅ |
| S2.4 Mod 字段冲突 | | | | | | ✅ | |
| S2.5 存档版本迁移 | | | | ✅ | | | ✅ |
| S3.1 资源守恒 | ✅ | ✅ | | | | | |
| S3.2 飞地断连 | | ✅ | | | | | |
| S3.3 战斗确定性 | | ✅ | | | | | |
| S3.4 存档 delta 往返 | | | | ✅ | | | |
| S3.5 种子确定性 | ✅ | | | | | | |
| S4.1 多部队同步行军 | | ✅ | | | | | |
| S4.2 AI 错峰决策 | | | ✅ | | | | |
| S4.3 大量地块占领 | | ✅ | | | | | |
| S4.4 批量战斗分帧 | | ✅ | ✅ | | | | |
| S4.5 编辑器批量填充 | | | | | ✅ | | |
| S5.1 两种地图尺寸 | ✅ | | | | | | |
| S5.2 ×3 倍速 | ✅ | | | | | | |
| S5.3 满地块占领 | | ✅ | | | | | ✅ |
| S5.4 全城池丢失 | | | ✅ | | | | ✅ |
| S5.5 多层 Mod 覆盖 | | | | | | ✅ | |
| S5.6 30% 阈值 | | | | ✅ | | | |
| S6.1 高频切换 | | | | | ✅ | | |
| S6.2 暂停/恢复 | ✅ | ✅ | | | | | |
| S6.3 多存档槽 | | | | ✅ | | | |

## 附录 B：关键断言辅助函数清单

| 函数名 | 用途 | 所在模块 |
|--------|------|---------|
| `assert_world_matches_doc(world, doc)` | Runtime World 逐格对比 MapDocument | `tests/common/assertions.rs` |
| `assert_save_roundtrip(world, doc)` | 保存→加载→对比零偏差 | `tests/common/assertions.rs` |
| `assert_faction_resources_consistent(faction, tick_delta)` | 资源收支守恒校验 | `tests/common/assertions.rs` |
| `assert_territory_connected(territory, faction)` | Union-Find 连通性断言 | `tests/common/assertions.rs` |
| `assert_combat_deterministic(input, runs)` | 同输入 N 次结果一致 | `tests/common/assertions.rs` |
| `assert_tile_delta_threshold(save, map, pct)` | delta 占比断言 | `tests/common/assertions.rs` |
| `build_headless_app(map_doc)` | 构造无渲染 Bevy App | `tests/common/headless_app.rs` |
| `run_ticks(app, count)` | 手动推进 N 个逻辑 tick | `tests/common/headless_app.rs` |
| `create_test_faction(id, personality)` | 构造测试势力 | `tests/common/fixtures.rs` |
| `create_combat_input(atk, def, seed)` | 构造战斗输入 | `tests/common/fixtures.rs` |

---

*本方案由 qa-engineer 基于 ARCHITECTURE.md v1.4 编制，随架构基线同步维护。*
