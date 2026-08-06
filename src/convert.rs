//! XML → Polars DataFrame 转换流水线。
//!
//! # 流程
//! 1. `convert_xml` / `convert_xml_file` 调用 [`parser::parse_game_xml`]
//!    生成 `Vec<Snapshot>`
//! 2. `build_snapshot_df` 将快照列表转为单张 Polars DataFrame（33 列）
//! 3. 直接写出为 `snapshots.parquet`（不再拆分 game_info 表）
//!
//! # 列存储策略
//! - **list[i32]** 列（手牌/牌河）: 每行先建一个小 Series，再收集为 List Series
//! - **list[i32;4]** 列（scores/turns/riichi 等）: 固定 4 元素 List
//! - **JSON 列**（副露）: 手写 JSON 序列化，供分析时解析
//! - **标量列**（seq/actor/action_type）: 标准 ChunkedArray

use std::path::Path;

use polars::prelude::*;

use crate::parser;
use crate::snapshot::{MeldEntry, Snapshot};

/// `PlSmallStr` 快捷构造宏（crate 内部使用）。
macro_rules! col_name {
    ($s:literal) => {
        PlSmallStr::from_str($s)
    };
}

// ── 公共 API ──────────────────────────────────────────────────

/// 将 mjlog XML 字符串转换为单张 snapshot DataFrame。
///
/// # 参数
/// - `xml`: mjlog XML 文本
/// - `game_id`: 牌谱 ID（通常为文件名主干，空字符串表示未知）
///
/// # 返回
/// - `Ok(df)` — 33 列 Polars DataFrame（见模块文档）
/// - `Err` — XML 解析失败
pub fn convert_xml(
    xml: &str,
    game_id: &str,
) -> anyhow::Result<DataFrame> {
    let snapshots = parser::parse_game_xml(xml, game_id)?;
    if snapshots.is_empty() {
        anyhow::bail!("no snapshots produced from XML");
    }
    build_snapshot_df(&snapshots)
}

/// 读取 mjlog XML 文件并转换。
///
/// `game_id` 自动从文件名主干提取。
pub fn convert_xml_file(path: &Path) -> anyhow::Result<DataFrame> {
    let game_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let raw = std::fs::read_to_string(path)?;
    convert_xml(&raw, game_id)
}

// ── DataFrame 构建 ────────────────────────────────────────────

/// 单张 33 列快照表。
fn build_snapshot_df(
    snapshots: &[Snapshot],
) -> anyhow::Result<DataFrame> {
    let len = snapshots.len();

    // ── 辅助宏 ─────────────────────────────────
    macro_rules! i32_col {
        ($name:literal, $field:ident) => {
            Int32Chunked::from_iter_values(
                col_name!($name),
                snapshots.iter().map(|s| s.$field),
            )
        };
    }
    macro_rules! i8_col {
        ($name:literal, $field:ident) => {
            Int8Chunked::from_iter_values(
                col_name!($name),
                snapshots.iter().map(|s| s.$field),
            )
        };
    }

    // ── 定位列 ─────────────────────────────────
    let game_id: StringChunked = {
        let mut ca: StringChunked =
            snapshots.iter().map(|s| s.game_id.as_str()).collect();
        ca.rename(col_name!("game_id"));
        ca
    };
    let seq = i32_col!("seq", seq);
    let round = i32_col!("round", round);
    let honba = i32_col!("honba", honba);
    let oya = i8_col!("oya", oya);
    let turns = list_i32_fixed_4("turns", snapshots, |s| &s.turns);
    let actor = i8_col!("actor", actor);

    // ── 手牌（仅决策前）──────────────
    let hb_p0 = list_i32_series_ref("hand_before_p0", snapshots, |s| &s.hand_before[0]);
    let hb_p1 = list_i32_series_ref("hand_before_p1", snapshots, |s| &s.hand_before[1]);
    let hb_p2 = list_i32_series_ref("hand_before_p2", snapshots, |s| &s.hand_before[2]);
    let hb_p3 = list_i32_series_ref("hand_before_p3", snapshots, |s| &s.hand_before[3]);

    // ── 牌河 ───────────────────────────────────
    let rv_p0 = list_i32_series_ref("river_p0", snapshots, |s| &s.river_p0);
    let rv_p1 = list_i32_series_ref("river_p1", snapshots, |s| &s.river_p1);
    let rv_p2 = list_i32_series_ref("river_p2", snapshots, |s| &s.river_p2);
    let rv_p3 = list_i32_series_ref("river_p3", snapshots, |s| &s.river_p3);
    let rvt_p0 = list_bool_series("river_tsumo_p0", snapshots, |s| &s.river_tsumo_p0);
    let rvt_p1 = list_bool_series("river_tsumo_p1", snapshots, |s| &s.river_tsumo_p1);
    let rvt_p2 = list_bool_series("river_tsumo_p2", snapshots, |s| &s.river_tsumo_p2);
    let rvt_p3 = list_bool_series("river_tsumo_p3", snapshots, |s| &s.river_tsumo_p3);

    // ── 副露（JSON 编码）──────────────
    let md_p0 = meld_json("melds_p0_json", snapshots, 0);
    let md_p1 = meld_json("melds_p1_json", snapshots, 1);
    let md_p2 = meld_json("melds_p2_json", snapshots, 2);
    let md_p3 = meld_json("melds_p3_json", snapshots, 3);

    // ── 场上状态 ───────────────────────────────
    let scores = list_i32_fixed_4("scores", snapshots, |s| &s.scores);
    let riichi = list_bool_fixed_4("riichi", snapshots, |s| &s.riichi);
    let riichi_turn =
        list_i32_fixed_4("riichi_turn", snapshots, |s| &s.riichi_turn);
    let riichi_tile =
        list_i32_fixed_4("riichi_tile", snapshots, |s| &s.riichi_tile);
    let dora_indicators =
        list_i32_series_ref("dora_indicators", snapshots, |s| &s.dora_indicators);
    let wall_remaining = i32_col!("wall_remaining", wall_remaining);
    let riichi_sticks = i32_col!("riichi_sticks", riichi_sticks);

    // ── 决策 ───────────────────────────────────
    let action_type: StringChunked = {
        let mut ca: StringChunked =
            snapshots.iter().map(|s| s.action_type.as_str()).collect();
        ca.rename(col_name!("action_type"));
        ca
    };
    let drawn_tile = i32_col!("drawn_tile", drawn_tile);
    let called_tile = i32_col!("called_tile", called_tile);
    let discard_tile = i32_col!("discard_tile", discard_tile);
    let is_tsumogiri: BooleanChunked = {
        let mut ca: BooleanChunked =
            snapshots.iter().map(|s| s.is_tsumogiri).collect();
        ca.rename(col_name!("is_tsumogiri"));
        ca
    };

    // ── 和了结果 ───────────────────────────────
    let agari_han_ids =
        list_i32_series_ref("agari_han_ids", snapshots, |s| &s.agari_han_ids);
    let agari_han = i32_col!("agari_han", agari_han);
    let agari_fu = i32_col!("agari_fu", agari_fu);
    let agari_points = i32_col!("agari_points", agari_points);
    let agari_from = i8_col!("agari_from", agari_from);
    let agari_ura_dora =
        list_i32_series_ref("agari_ura_dora", snapshots, |s| &s.agari_ura_dora);

    // ── 局末信息（每行冗余）─────────────
    let round_end_kind = i8_col!("round_end_kind", round_end_kind);
    let round_winner = i8_col!("round_winner", round_winner);
    let round_point_delta =
        list_i32_fixed_4("round_point_delta", snapshots, |s| &s.round_point_delta);
    let round_tenpai_count =
        i8_col!("round_tenpai_count", round_tenpai_count);

    // ── 组装 ────────────────────────────────────
    let mut cols: Vec<Column> = vec![
        game_id.into_series().into(),
        seq.into_series().into(),
        round.into_series().into(),
        honba.into_series().into(),
        oya.into_series().into(),
        turns.into(),
        actor.into_series().into(),
        // hand
        hb_p0.into(), hb_p1.into(), hb_p2.into(), hb_p3.into(),
        // river
        rv_p0.into(), rv_p1.into(), rv_p2.into(), rv_p3.into(),
        rvt_p0.into(), rvt_p1.into(), rvt_p2.into(), rvt_p3.into(),
        // melds JSON
        md_p0.into(), md_p1.into(), md_p2.into(), md_p3.into(),
        // board
        scores.into(), riichi.into(),
        riichi_turn.into(), riichi_tile.into(),
        dora_indicators.into(), wall_remaining.into_series().into(),
        riichi_sticks.into_series().into(),
        // action
        action_type.into_series().into(),
        drawn_tile.into_series().into(),
        called_tile.into_series().into(),
        discard_tile.into_series().into(),
        is_tsumogiri.into_series().into(),
        // agari
        agari_han_ids.into(), agari_han.into_series().into(),
        agari_fu.into_series().into(), agari_points.into_series().into(),
        agari_from.into_series().into(),
        agari_ura_dora.into(),
        // round-end
        round_end_kind.into_series().into(),
        round_winner.into_series().into(),
        round_point_delta.into(),
        round_tenpai_count.into_series().into(),
    ];

    // 对齐长度（防卫性代码，空输入已在 convert_xml 中处理）
    for c in &mut cols {
        if c.len() != len {
            *c = c.new_from_index(0, len).into();
        }
    }

    Ok(DataFrame::new(cols)?)
}

// ── list 列构建器 ─────────────────────────────────────────────

fn list_i32_series_ref(
    name: &str,
    snapshots: &[Snapshot],
    get: impl Fn(&Snapshot) -> &Vec<i32>,
) -> Series {
    let inner: Vec<Series> = snapshots
        .iter()
        .map(|s| Series::new(PlSmallStr::from_static(""), get(s).as_slice()))
        .collect();
    Series::new(PlSmallStr::from_str(name), inner)
}

fn list_i32_fixed_4(
    name: &str,
    snapshots: &[Snapshot],
    get: impl Fn(&Snapshot) -> &[i32; 4],
) -> Series {
    let inner: Vec<Series> = snapshots
        .iter()
        .map(|s| Series::new(PlSmallStr::from_static(""), get(s)))
        .collect();
    Series::new(PlSmallStr::from_str(name), inner)
}

fn list_bool_series(
    name: &str,
    snapshots: &[Snapshot],
    get: impl Fn(&Snapshot) -> &Vec<bool>,
) -> Series {
    let inner: Vec<Series> = snapshots
        .iter()
        .map(|s| Series::new(PlSmallStr::from_static(""), get(s).as_slice()))
        .collect();
    Series::new(PlSmallStr::from_str(name), inner)
}

fn list_bool_fixed_4(
    name: &str,
    snapshots: &[Snapshot],
    get: impl Fn(&Snapshot) -> &[bool; 4],
) -> Series {
    let inner: Vec<Series> = snapshots
        .iter()
        .map(|s| Series::new(PlSmallStr::from_static(""), get(s)))
        .collect();
    Series::new(PlSmallStr::from_str(name), inner)
}

// ── JSON 序列化（副露）─────────────────────────────────────────

fn meld_json(
    name: &str,
    snapshots: &[Snapshot],
    player: usize,
) -> Series {
    let mut vals: StringChunked = snapshots
        .iter()
        .map(|s| json_melds(s.melds_for(player)))
        .collect();
    vals.rename(PlSmallStr::from_str(name));
    vals.into_series()
}

fn json_melds(melds: &[MeldEntry]) -> String {
    if melds.is_empty() {
        return "[]".into();
    }
    let items: Vec<String> = melds
        .iter()
        .map(|e| {
            let tiles_str: Vec<String> =
                e.tiles.iter().map(|t| t.to_string()).collect();
            format!(
                r#"{{"type":"{}","tiles":[{}],"called":{},"from":{},"discard_n":{},"pos":{}}}"#,
                e.meld_type,
                tiles_str.join(","),
                e.called_tile,
                e.from_player,
                e.discard_n,
                e.called_pos
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}
