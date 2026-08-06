# mahjong-analytics

天凤凤凰桌（Houou）牌谱转换与分析工具。将 mjlog XML 解析为 Polars DataFrame，存储为 Parquet 快照表，并提供桌面可视化。

## 功能

- **牌谱解析** — mjlog XML → 结构化快照数据（每行 = 一次玩家决策）
- **批量转换** — 从 houou-logs SQLite 数据库批量提取
- **Parquet 存储** — 列式压缩，适合大规模分析（已处理 ~93 万局、4.85 亿行）
- **GUI 可视化** — egui/eframe 桌面应用，牌桌渲染 + 牌河回放
- **麻将算法** — 向听数 / 听牌 / 和了判定

## 快速开始

```sh
# CLI 转换单局
cargo run --release -- convert game.xml -o output/

# 批量转换
cargo run --release -- batch ../houou-logs/db/5ydb.db -o tables/

# GUI 可视化
cargo run --release --bin mahjong-viz -- snapshots.parquet
```

## 数据格式

输出为 Parquet 文件（44 列），核心字段：

| 组 | 字段 | 说明 |
|----|------|------|
| 定位 | `game_id`, `seq`, `actor` | 对局 ID、决策序号、行动玩家 |
| 手牌 | `hand_before_p0`~`p3` | 决策前手牌（天凤实例 ID） |
| 牌河 | `river_p0`~`p3`, `river_tsumo_p0`~`p3` | 牌河 + 手摸切标记 |
| 副露 | `melds_p0_json`~`p3_json` | 副露 JSON |
| 局势 | `scores`, `riichi`, `dora_indicators` | 点数、立直、宝牌 |
| 决策 | `action_type`, `drawn_tile`, `discard_tile` | 行动类型、摸牌、打牌 |
| 和了 | `agari_han`, `agari_fu`, `agari_points` | 飜数、符数、点数 |
| 终局 | `round_end_kind`, `round_winner`, `round_point_delta` | 终局类型、胜者、点差 |

## 项目结构

```
src/
├── main.rs         # CLI 入口
├── lib.rs          # 公共 API
├── cli.rs          # clap 参数定义
├── parser.rs       # mjlog XML 解析 + 副露解码
├── snapshot.rs     # 核心数据类型
├── convert.rs      # Snapshot → Polars DataFrame
├── table.rs        # Parquet 读写
├── mahjong/        # 麻将规则工具
│   ├── tiles.rs    # 牌键排序、赤牌处理
│   └── hand.rs     # 向听数、听牌、和了判定
└── viz/            # GUI 可视化 (egui/eframe)
    ├── app.rs      # 应用状态管理
    ├── board.rs    # 牌桌渲染
    └── tiles.rs    # 牌画加载 (mahjim)
```

## 牌画素材

GUI 需要牌画图片，放在 `G:\麻雀\mahjim\mahjim-master\assets\files\`。

文件命名：`1mjp.png`~`9mjp.png`, `1p.png`~`9p.png`, `1sjp.png`+`2s.png`~`9s.png`，風/字牌用中文（`东jp.png` 等）。

## 数据来源

天凤凤凰桌（Houou）牌谱，由 [hounest/houou-log-kyoku](https://github.com/hounest/houou-log-kyoku) 项目抓取。

## License

MIT
