# mahjong-analytics

天凤凤凰桌（Houou）牌谱转换与分析工具。将 mjlog XML 解析为 Polars DataFrame，存储为 Parquet 快照表，并提供桌面可视化。

## 项目结构

```
src/
├── main.rs              # CLI 入口 (mahjong-analytics)
├── lib.rs               # 公共 API: convert_one(), convert_batch()
├── cli.rs               # clap 参数定义 (convert / batch 子命令)
│
├── snapshot.rs          # 核心数据类型
│   ├── RiverEntry       #   牌河条目 (tile, stolen_by, stolen_as, is_riichi, is_tsumogiri)
│   ├── MeldEntry        #   副露条目 (meld_type, tiles, called_tile, from_player, discard_after)
│   ├── GameState        #   解析中的游戏状态 (内部)
│   └── Snapshot         #   快照 (pub, 每行一次决策, 52列写入 Parquet)
│
├── parser.rs            # mjlog XML → Vec<Snapshot>
│   ├── decode_meld()    #   Perl 参考算法: bit2=CHI, bits3-4=PON/KAN, else=ANKAN
│   ├── parse_game_xml() #   单次遍历状态机: TUVW摸/DEFG打/N副露/REACH/DORA/AGARI
│   └── 辅助函数          #   emit_snapshot, backfill_agari, discard_river 等
│
├── convert.rs           # Vec<Snapshot> → Polars DataFrame (52列)
│   └── list_i32_series  #   list 列构建 (每行小Series收集为List Series)
│
├── table.rs             # ConversionResult: 多表 HashMap + Parquet 读写
│
├── mahjong/             # 麻将规则工具
│   ├── tiles.rs         #   TileKey 排序键, sort_hand(), sort_hand_with_draw()
│   └── hand.rs          #   shanten(), is_tenpai(), is_agari(), calc_score()
│
└── viz/                 # GUI 可视化 (egui/eframe)
    ├── app.rs           #   VizApp 状态管理 + parquet 回读
    ├── board.rs         #   牌桌渲染: 风盘 + 四家手牌/牌河 (群组旋转)
    ├── tiles.rs         #   牌名映射 (ASCII), TileAssets 图片加载 (mahjim)
    └── mod.rs

src/bin/viz.rs           # mahjong-viz 独立二进制入口
```

## 数据流

```
XML 文件 (从天凤 houou-logs SQLite 提取)
  │ parse_game_xml()
  ▼
Vec<Snapshot>     ← 每行 = 一个玩家的一次决策时刻
  │ build_snapshot_df()
  ▼
DataFrame (44列)  ← list[i32] 手牌/牌河 + JSON 副露 + 标量字段
  │ ParquetWriter
  ▼
E:\tables\*.parquet  ← 每局一个文件, 列式压缩, ~90 bytes/row
  (932,331 文件, 45 GB)
```

## Snapshot 表结构 (52列) — 代码内部 / 理想模型

> **注意**：生产数据 `E:\tables\` 中的实际 Parquet 为 45 列精简版，详见上方「生产数据」章节。以下为代码中构建的理想 52 列模型。

| 组 | 列 | 类型 |
|----|----|------|
| 定位 | game_id, round, honba, turn, actor, event_in_turn | str, i32×3, i8×2 |
| 手牌 | hand_before_p0~p3, hand_after_p0~p3 | list[i32] ×8 |
| 牌河(原生) | river_p0~_tiles, river_p0~_stolen | list[i32], list[i8] ×8 |
| 副露(原生) | melds_p0~_types/called/from | list[str]/list[i32]/list[i8] ×12 |
| 牌河(JSON) | river_p0~_json | str ×4 |
| 副露(JSON) | melds_p0~_json | str ×4 |
| 局面 | scores, riichi, dora_list | list[i32;4], list[bool;4], list[i32] |
| 决策 | action_type, drawn_tile, discard_tile | str, i32×2 |
| 标签 | is_deal_in, deal_in_to, deal_in_fu, tenpai | bool, i8, i32, list[bool;4] |

## 关键算法

### 副露解码 (parser.rs decode_meld)

基于天凤 Perl 参考实现。**类型检测用 bit 2-4，不是 bit 0-1**：

```
bit 0-1 (0x03): 显示标记 (无关类型)
bit 2   (0x04): CHI (吃)
bit 3-4 (0x18): PON/KAN (碰/槓)
  其中 bit 4 (0x10): KAKAN (加槓)
都不 set: ANKAN (暗槓)
```

牌值编码为 packed integer：
- CHI: `p=(m>>10)&0x3F, r=p%3, p/=3, suit=p/7, n=p%7+1`
- PON: `p=(m>>9)&0x7F, r=p%3, p/=3, suit=p/9, n=p%9+1`
- ANKAN: `p=(m>>8)&0xFF, p/=4, suit=p/9, n=p%9+1`

### 天凤实例 ID → 牌种 ID

天凤用 0-135 区分 34 种 × 4 枚 = 136 张牌。

```
type_id = instance_id / 4
牌种: 0-8=万子, 9-17=筒子, 18-26=索子, 27-33=字牌
赤牌: 34=赤5m, 37=赤5p, 40=赤5s
```

在 viz 渲染前用 `tiles::instance_to_type()` 转换。

### 手牌理牌 (mahjong/tiles.rs)

排序: 万→筒→索→字, 同花色数字升序, 赤牌后置。
摸牌分离到末尾 + 12px 间隙。

## 生产数据

已转换的快照数据位于 **`E:\tables\`**，每局一个 Parquet 文件，扁平目录存储。

| 指标 | 值 |
|------|-----|
| 文件数 | 932,331 |
| 总大小 | 45 GB |
| 日期范围 | 2021-01-01 ~ 2026-04-01（~5.3 年，1918 天） |
| 日均对局 | ~486 局（400~700 局/天） |
| 预估总行数 | ~5.1 亿行（每局平均 ~546 行） |
| 目录结构 | 扁平（无子目录） |
| 文件命名 | `YYYYMMDDHHgm-00a9-0000-XXXXXXXX.parquet`，前 10 位为日期小时 |
| 合并文件 | `snapshots_all.parquet` — 14 GB, 4.85 亿行, 45 列 |

### 实际 Parquet Schema (45 列)

比 CLAUDE.md 中描述的理想 52 列精简，主要差异：无 `hand_after_*` 列，副露/牌河仅用 JSON 列存储，新增和风/分数增量列，2026-08 新增 `round`（局编号，seed[0]）。

| 组 | 列 | 类型 | 说明 |
|----|----|------|------|
| 定位 | `game_id` | str | 天凤对局 ID |
| | `seq` | i32 | 决策序号（0-based） |
| | `round` | i32 | 局编号（0=東一, 1=東二, …, 4=南一）。場風=`round/4` |
| | `honba` | i32 | 本场数 |
| | `oya` | i8 | 亲家座位 (0-3) |
| | `turns` | list[i32] | 各玩家剩余巡数 |
| | `actor` | i8 | 当前行动玩家 (0-3) |
| 手牌 | `hand_before_p0`~`p3` | list[i32] ×4 | 决策前手牌（天凤实例 ID） |
| 牌河 | `river_p0`~`p3` | list[i32] ×4 | 牌河（天凤实例 ID） |
| | `river_tsumo_p0`~`p3` | list[bool] ×4 | 牌河每张是否手摸切 (T=手切) |
| 副露 | `melds_p0_json`~`p3_json` | str ×4 | 副露 JSON 数组 |
| 局面 | `scores` | list[i32;4] | 四人点数 |
| | `riichi` | list[bool;4] | 各家是否立直 |
| | `riichi_turn` | list[i32;4] | 立直宣言巡目 |
| | `riichi_tile` | list[i32;4] | 立直宣言牌 |
| | `dora_indicators` | list[i32] | 宝牌指示牌 |
| | `wall_remaining` | i32 | 牌山剩余枚数 |
| | `riichi_sticks` | i32 | 场上供託数 |
| 决策 | `action_type` | str | discard/tsumogiri/chi/pon/ankan/reach/tsumo/ron |
| | `drawn_tile` | i32 | 摸入牌（天凤实例 ID，-1=无） |
| | `called_tile` | i32 | 副露时被叫牌（-1=无） |
| | `discard_tile` | i32 | 打出牌（-1=无） |
| | `is_tsumogiri` | bool | 是否手摸切打牌 |
| 和了 | `agari_han_ids` | list[i32] | 役种 ID 列表 |
| | `agari_han` | i32 | 飜数 |
| | `agari_fu` | i32 | 符数 |
| | `agari_points` | i32 | 和了点 |
| | `agari_from` | i8 | 放铳者 (-1=自摸) |
| | `agari_ura_dora` | list[i32] | 里宝牌 |
| 终局 | `round_end_kind` | i8 | 终局类型 (0=无事,1=和了,2=流局) |
| | `round_winner` | i8 | 和了者 (0-3) |
| | `round_point_delta` | list[i32;4] | 各人点数变化 |
| | `round_tenpai_count` | i8 | 流局听牌人数 (-1=非流局) |

### 数据读取示例

```python
import polars as pl

# 读取单局文件
df = pl.read_parquet("E:/tables/2021010100gm-00a9-0000-01e8ec31.parquet")

# 读取全量合并文件（13 GB, 4.85 亿行）—— 推荐分析用
df_all = pl.scan_parquet("E:/tables/snapshots_all.parquet")

# 扫描全部散文件（延迟计算，注意文件名通配可能遍历慢）
df_all = pl.scan_parquet("E:/tables/*.parquet")
```

## 构建与运行

```sh
# CLI 转换
cargo run --release -- convert game.xml -o output/

# 批量转换 (从 houou-logs SQLite)
cargo run --release -- batch ../houou-logs/db/5ydb.db -o tables/

# GUI 可视化
cargo run --release --bin mahjong-viz -- snapshots.parquet
```

### 牌画素材

放在 `G:\麻雀\mahjim\mahjim-master\assets\files\`。
文件命名: `1mjp.png` ~ `9mjp.png`, `1p.png` ~ `9p.png`, `1sjp.png` + `2s.png`~`9s.png`, 風/字牌用中文文件名 (`东jp.png` 等)。
加载数显示在 viz 顶部栏。

## 已知限制

- **副露 from_player**: CHI 固定为左家, PON 用 `(who+1+r)%4`。大明槓的 from 未与碰区分
- **赤宝牌**: 实例 ID 层面未区分（需检查 instance_id 的 copy 位置），当前所有 5 视为同牌种
- **牌河旋转**: P1/P2/P3 牌画仅在中心定位时旋转，图片本身保持正位（egui 0.31 无图片旋转 API）
- **风盘编码**: egui 默认字体无 CJK，所有 UI 文本用 ASCII (E1/S1/honba/riichi bou 等)
- **game_info 表**: 仅含 game_id + num_players，待补充昵称/段位/R值

## 调试

```sh
# VS Code F5 启动调试 (需 cppvsdbg)
# 预配置: .vscode/launch.json + tasks.json

# 查看牌画加载日志 (stderr)
target/release/mahjong-viz.exe snapshots.parquet 2>&1

# 快速检查 parquet 内容
uv run python -c "import polars as pl; df=pl.read_parquet('...'); print(df.columns)"
```
