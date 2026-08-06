//! GUI 应用状态管理与 eframe 集成。
//!
//! [`VizApp`] 是可视化工具的核心状态容器，持有：
//! - 从 Parquet 加载的 [`Snapshot`] 列表
//! - 当前查看的索引
//! - [`TileAssets`] 牌画缓存
//! - 导航控件状态
//!
//! # 数据加载流程
//! 1. 用户打开 `snapshots.parquet` → `try_load`
//! 2. 逐列读取 Polars DataFrame（33 列）→ 组装 `Vec<Snapshot>`
//! 3. 错误信息写入 `self.error`，在主界面展示

use std::path::PathBuf;

use egui::{Color32, Context, Key};
use polars::prelude::*;

use crate::snapshot::{MeldEntry, Snapshot};
use crate::viz::board;
use crate::viz::tiles::TileAssets;

// ── 主结构 ────────────────────────────────────────────────────

pub struct VizApp {
    snapshots: Vec<Snapshot>,
    idx: usize,
    path: String,
    error: Option<String>,
    jump_seq: String,
    tile_assets: TileAssets,
}

impl Default for VizApp {
    fn default() -> Self {
        let mut tile_assets = TileAssets::new();
        tile_assets.discover(None);
        Self {
            snapshots: vec![],
            idx: 0,
            path: String::new(),
            error: None,
            jump_seq: String::new(),
            tile_assets,
        }
    }
}

impl VizApp {
    /// 从 Parquet 文件加载快照。
    pub fn load_parquet(&mut self, path: PathBuf) {
        self.path = path.display().to_string();
        self.error = None;
        self.idx = 0;

        match self.try_load(&path) {
            Ok(snaps) => {
                self.snapshots = snaps;
                if self.snapshots.is_empty() {
                    self.error =
                        Some("No snapshots found in file.".into());
                }
            }
            Err(e) => {
                self.error =
                    Some(format!("Failed to load: {e}"));
                self.snapshots = vec![];
            }
        }
    }

    /// Parquet → `Vec<Snapshot>` 转换（33 列新 Schema）。
    fn try_load(
        &self,
        path: &PathBuf,
    ) -> anyhow::Result<Vec<Snapshot>> {
        let df = ParquetReader::new(std::fs::File::open(path)?)
            .finish()?;

        let len = df.height();
        if len == 0 {
            anyhow::bail!("File has 0 rows.");
        }

        // 验证关键列
        let cols: Vec<String> = df
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        for required in &["seq", "actor", "action_type"] {
            if !cols.iter().any(|c| c == required) {
                anyhow::bail!(
                    "Column '{required}' not found. Available: {cols:?}"
                );
            }
        }

        // ── 标量列 ───────────────────────────────
        let game_id: Vec<String> =
            read_str_col(&df, "game_id")?;
        let seq: Vec<i32> = read_i32_col(&df, "seq")?;
        let honba: Vec<i32> = read_i32_col(&df, "honba")?;
        let oya: Vec<i8> = read_i8_col(&df, "oya")?;
        let actor: Vec<i8> = read_i8_col(&df, "actor")?;
        let action_type: Vec<String> =
            read_str_col(&df, "action_type")?;
        let drawn_tile: Vec<i32> =
            read_i32_col(&df, "drawn_tile")?;
        let called_tile: Vec<i32> =
            read_i32_col(&df, "called_tile")?;
        let discard_tile: Vec<i32> =
            read_i32_col(&df, "discard_tile")?;
        let is_tsumogiri: Vec<bool> =
            read_bool_col(&df, "is_tsumogiri")?;
        let wall_remaining: Vec<i32> =
            read_i32_col(&df, "wall_remaining")?;
        let riichi_sticks: Vec<i32> =
            read_i32_col(&df, "riichi_sticks")?;
        let agari_han: Vec<i32> =
            read_i32_col(&df, "agari_han")?;
        let agari_fu: Vec<i32> =
            read_i32_col(&df, "agari_fu")?;
        let agari_points: Vec<i32> =
            read_i32_col(&df, "agari_points")?;
        let agari_from: Vec<i8> =
            read_i8_col(&df, "agari_from")?;
        let agari_ura_dora =
            read_list_i32_any(&df, "agari_ura_dora")?;

        // ── list[i32;4] 列 ────────────────────────
        let turns =
            read_list_i32_4(&df, "turns")?;
        let scores =
            read_list_i32_4(&df, "scores")?;
        let riichi_turn =
            read_list_i32_4(&df, "riichi_turn")?;
        let riichi_tile =
            read_list_i32_4(&df, "riichi_tile")?;

        // ── list[bool;4] 列 ───────────────────────
        let riichi =
            read_list_bool_4(&df, "riichi")?;
        let round_end_kind: Vec<i8> =
            read_i8_col(&df, "round_end_kind")?;
        let round_winner: Vec<i8> =
            read_i8_col(&df, "round_winner")?;
        let round_point_delta =
            read_list_i32_4(&df, "round_point_delta")?;
        let round_tenpai_count: Vec<i8> =
            read_i8_col(&df, "round_tenpai_count")?;

        // ── list[i32] 可变长列 ────────────────────
        let hand_before =
            read_hand_cols(&df, "hand_before")?;
        let river_p0 =
            read_list_i32_any(&df, "river_p0")?;
        let river_p1 =
            read_list_i32_any(&df, "river_p1")?;
        let river_p2 =
            read_list_i32_any(&df, "river_p2")?;
        let river_p3 =
            read_list_i32_any(&df, "river_p3")?;
        let rvt_p0 =
            read_list_bool_any(&df, "river_tsumo_p0")?;
        let rvt_p1 =
            read_list_bool_any(&df, "river_tsumo_p1")?;
        let rvt_p2 =
            read_list_bool_any(&df, "river_tsumo_p2")?;
        let rvt_p3 =
            read_list_bool_any(&df, "river_tsumo_p3")?;
        let dora_indicators =
            read_list_i32_any(&df, "dora_indicators")?;
        let agari_han_ids =
            read_list_i32_any(&df, "agari_han_ids")?;

        // ── JSON 列（副露）────────────────────────
        let melds_p0 =
            read_meld_json(&df, "melds_p0_json")?;
        let melds_p1 =
            read_meld_json(&df, "melds_p1_json")?;
        let melds_p2 =
            read_meld_json(&df, "melds_p2_json")?;
        let melds_p3 =
            read_meld_json(&df, "melds_p3_json")?;

        // ── 逐行组装 ──────────────────────────────
        let mut snapshots = Vec::with_capacity(len);
        for i in 0..len {
            snapshots.push(Snapshot {
                game_id: game_id
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                seq: seq[i],
                honba: honba[i],
                oya: oya[i],
                turns: turns[i],
                actor: actor[i],
                hand_before: hand_before[i].clone(),
                river_p0: river_p0
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                river_p1: river_p1
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                river_p2: river_p2
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                river_p3: river_p3
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                river_tsumo_p0: rvt_p0
                    .get(i).cloned().unwrap_or_default(),
                river_tsumo_p1: rvt_p1
                    .get(i).cloned().unwrap_or_default(),
                river_tsumo_p2: rvt_p2
                    .get(i).cloned().unwrap_or_default(),
                river_tsumo_p3: rvt_p3
                    .get(i).cloned().unwrap_or_default(),
                melds_p0: melds_p0
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                melds_p1: melds_p1
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                melds_p2: melds_p2
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                melds_p3: melds_p3
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                scores: scores[i],
                riichi: riichi[i],
                riichi_turn: riichi_turn[i],
                riichi_tile: riichi_tile[i],
                dora_indicators: dora_indicators
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                wall_remaining: wall_remaining[i],
                riichi_sticks: riichi_sticks[i],
                action_type: action_type
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                drawn_tile: drawn_tile[i],
                called_tile: called_tile[i],
                discard_tile: discard_tile[i],
                agari_han_ids: agari_han_ids
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                agari_han: agari_han[i],
                agari_fu: agari_fu[i],
                agari_points: agari_points[i],
                agari_from: agari_from[i],
                is_tsumogiri: is_tsumogiri[i],
                agari_ura_dora: agari_ura_dora
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
                round_end_kind: round_end_kind[i],
                round_winner: round_winner[i],
                round_point_delta: round_point_delta[i],
                round_tenpai_count: round_tenpai_count[i],
            });
        }

        Ok(snapshots)
    }
}

// ── eframe 集成 ───────────────────────────────────────────────

impl eframe::App for VizApp {
    fn update(
        &mut self,
        ctx: &Context,
        _frame: &mut eframe::Frame,
    ) {
        self.tile_assets.ensure_textures(ctx);
        self.handle_keys(ctx);

        // ── 顶部控制栏 ───────────────────────────
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let n_loaded = self.tile_assets.image_count();
                let n_tex = self.tile_assets.texture_count();
                if n_loaded > 0 {
                    ui.label(format!("🀄 {n_loaded}/{n_tex}"));
                } else {
                    ui.colored_label(
                        Color32::YELLOW,
                        "🀄 无牌画 (放PNG到 ../mahjim/)",
                    );
                }
                ui.separator();

                if ui.button("Open parquet...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Parquet", &["parquet"])
                        .pick_file()
                    {
                        self.load_parquet(path);
                    }
                }
                ui.label(&self.path);

                if !self.snapshots.is_empty() {
                    ui.separator();
                    if ui.button("◀ Prev").clicked() {
                        self.idx = self.idx.saturating_sub(1);
                    }
                    ui.label(format!(
                        "{}/{}",
                        self.idx + 1,
                        self.snapshots.len()
                    ));
                    if ui.button("Next ▶").clicked()
                        && self.idx + 1
                            < self.snapshots.len()
                    {
                        self.idx += 1;
                    }
                    ui.separator();
                    ui.label("Jump seq:");
                    ui.text_edit_singleline(
                        &mut self.jump_seq,
                    );
                    if ui.button("Go").clicked() {
                        if let Ok(t) =
                            self.jump_seq.parse::<i32>()
                        {
                            if let Some(pos) = self
                                .snapshots
                                .iter()
                                .position(|s| s.seq >= t)
                            {
                                self.idx = pos;
                            }
                        }
                    }
                }
            });
        });

        // ── 无数据 → 提示 ────────────────────────
        if self.snapshots.is_empty() {
            egui::CentralPanel::default().show(
                ctx,
                |ui| {
                    if let Some(err) = &self.error {
                        ui.colored_label(
                            Color32::RED,
                            err,
                        );
                    } else {
                        ui.label(
                            "No data. Open a snapshots.parquet file.",
                        );
                    }
                },
            );
            return;
        }

        let snap = &self.snapshots[self.idx];
        let assets = if self.tile_assets.has_any() {
            Some(&self.tile_assets)
        } else {
            None
        };

        // 右侧信息面板 - 中文
        egui::SidePanel::right("action_info")
            .resizable(false)
            .min_width(220.0)
            .show(ctx, |ui| {
                let snap = &self.snapshots[self.idx];
                let rt = |s: String| egui::RichText::new(s).size(22.0).color(Color32::BLACK);
                let rtb = |s: String| egui::RichText::new(s).size(22.0).color(Color32::BLACK).strong();
                ui.label(rtb(format!("快照 {}/{}", self.idx + 1, self.snapshots.len())));
                ui.label(rt(format!("序号: {}", snap.seq)));
                ui.separator();
                let oya_cn = ["东", "南", "西", "北"][snap.oya as usize % 4];
                ui.label(rt(format!("本场数: {}    庄家: P{}({oya_cn})", snap.honba, snap.oya)));
                ui.label(rt(format!("供託: {}    余牌: {}", snap.riichi_sticks, snap.wall_remaining)));
                ui.separator();
                ui.label(rt(format!("P0(自家):{:>6}  巡目:{}", snap.scores[0], snap.turns[0])));
                ui.label(rt(format!("P1(下家):{:>6}  巡目:{}", snap.scores[1], snap.turns[1])));
                ui.label(rt(format!("P2(対面):{:>6}  巡目:{}", snap.scores[2], snap.turns[2])));
                ui.label(rt(format!("P3(上家):{:>6}  巡目:{}", snap.scores[3], snap.turns[3])));
                ui.separator();
                let actor = snap.actor as usize;
                let seat_cn = ["自家", "下家", "対面", "上家"][actor];
                let act_cn = crate::viz::tiles::action_label_cn(&snap.action_type);
                ui.label(rt(format!("行动者: P{}({seat_cn})", actor)));
                ui.label(rt(format!("行动: {act_cn}")));
                let tn = |id: i32| if id >= 0 {
                    crate::viz::tiles::tile_name(crate::viz::tiles::instance_to_type(id))
                } else { "-" };
                ui.label(rt(format!("摸牌: {}  叫牌: {}  切牌: {}",
                    tn(snap.drawn_tile), tn(snap.called_tile), tn(snap.discard_tile))));
                ui.separator();
                for i in 0..4 {
                    if snap.riichi[i] {
                        ui.label(rt(format!("P{i}◆立直 巡{} 宣言牌:{}",
                            snap.riichi_turn[i], tn(snap.riichi_tile[i]))));
                    }
                }
                for i in 0..4 {
                    let melds = snap.melds_for(i);
                    if !melds.is_empty() {
                        let desc: Vec<String> = melds.iter().map(|m| {
                            let t_cn = match m.meld_type.as_str() {
                                "chi" => "吃", "pon" => "碰",
                                "daiminkan" => "大明杠", "ankan" => "暗杠",
                                "kakan" => "加杠", _ => &m.meld_type,
                            };
                            if m.from_player >= 0 { format!("{t_cn}(P{})", m.from_player) }
                            else { t_cn.to_string() }
                        }).collect();
                        ui.label(rt(format!("P{i}副露: {}", desc.join(", "))));
                    }
                }
                if snap.action_type == "tsumo" || snap.action_type == "ron" {
                    ui.separator();
                    let agari_cn = if snap.action_type == "tsumo" { "自摸" } else { "荣和" };
                    ui.label(rt(format!("{agari_cn} {}飜{}符 {}点",
                        snap.agari_han, snap.agari_fu, snap.agari_points)));
                    if snap.agari_from >= 0 {
                        ui.label(rt(format!("  放铳者: P{}", snap.agari_from)));
                    }
                    if !snap.agari_ura_dora.is_empty() {
                        let ura: Vec<&str> = snap.agari_ura_dora.iter()
                            .map(|&d| crate::viz::tiles::tile_name(
                                crate::viz::tiles::instance_to_type(d)))
                            .collect();
                        ui.label(rt(format!("  里宝牌: {}", ura.join(", "))));
                    }
                }
                if snap.round_end_kind >= 0 {
                    let end = match snap.round_end_kind { 0=>"流局", 1=>"自摸终局", 2=>"荣和终局", _=>"?" };
                    let d = snap.round_point_delta;
                    ui.separator();
                    ui.label(rt(format!("--- {end} ---")));
                    ui.label(rt(format!("点数变动: [{},{},{},{}]", d[0], d[1], d[2], d[3])));
                    if snap.round_tenpai_count >= 0 {
                        ui.label(rt(format!("听牌人数: {}", snap.round_tenpai_count)));
                    }
                }
            });

        // ── 牌桌 ──────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            board::draw_board(ui, snap, assets);
        });
    }
}

// ── 键盘快捷键 ────────────────────────────────────────────────

impl VizApp {
    fn handle_keys(&mut self, ctx: &Context) {
        let n = self.snapshots.len();
        if n == 0 {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            self.idx = self.idx.saturating_sub(1);
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowRight))
            && self.idx + 1 < n
        {
            self.idx += 1;
        }
        if ctx.input(|i| i.key_pressed(Key::Home)) {
            self.idx = 0;
        }
        if ctx.input(|i| i.key_pressed(Key::End)) {
            self.idx = n.saturating_sub(1);
        }
    }
}

// ── Polars 列读取辅助 ─────────────────────────────────────────

fn read_str_col(
    df: &DataFrame,
    name: &str,
) -> anyhow::Result<Vec<String>> {
    Ok(df
        .column(name)?
        .str()?
        .into_iter()
        .map(|o| o.unwrap_or("").to_string())
        .collect())
}

fn read_i32_col(
    df: &DataFrame,
    name: &str,
) -> anyhow::Result<Vec<i32>> {
    Ok(df
        .column(name)?
        .i32()?
        .into_no_null_iter()
        .collect())
}

fn read_bool_col(df: &DataFrame, name: &str) -> anyhow::Result<Vec<bool>> {
    Ok(df.column(name)?.bool()?.into_no_null_iter().collect())
}

fn read_i8_col(
    df: &DataFrame,
    name: &str,
) -> anyhow::Result<Vec<i8>> {
    Ok(df
        .column(name)?
        .i8()?
        .into_no_null_iter()
        .collect())
}

fn read_list_i32_4(
    df: &DataFrame,
    name: &str,
) -> anyhow::Result<Vec<[i32; 4]>> {
    let s = df.column(name)?;
    let list = s.list()?;
    let mut result = Vec::with_capacity(s.len());
    for row in list.into_iter() {
        let vals: Vec<i32> = row
            .map(|s| {
                s.i32()
                    .map(|c| c.into_no_null_iter().collect())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let mut arr = [0i32; 4];
        for (i, &v) in vals.iter().enumerate().take(4) {
            arr[i] = v;
        }
        result.push(arr);
    }
    Ok(result)
}

fn read_list_bool_4(
    df: &DataFrame,
    name: &str,
) -> anyhow::Result<Vec<[bool; 4]>> {
    let s = df.column(name)?;
    let list = s.list()?;
    let mut result = Vec::with_capacity(s.len());
    for row in list.into_iter() {
        let vals: Vec<bool> = row
            .map(|s| {
                s.bool()
                    .map(|c| c.into_no_null_iter().collect())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let mut arr = [false; 4];
        for (i, &v) in vals.iter().enumerate().take(4) {
            arr[i] = v;
        }
        result.push(arr);
    }
    Ok(result)
}

fn read_list_bool_any(df: &DataFrame, name: &str) -> anyhow::Result<Vec<Vec<bool>>> {
    let s = df.column(name)?;
    let list = s.list()?;
    let mut result = Vec::with_capacity(s.len());
    for row in list.into_iter() {
        let vals: Vec<bool> = row
            .map(|s| s.bool().map(|c| c.into_no_null_iter().collect()).unwrap_or_default())
            .unwrap_or_default();
        result.push(vals);
    }
    Ok(result)
}

fn read_list_i32_any(
    df: &DataFrame,
    name: &str,
) -> anyhow::Result<Vec<Vec<i32>>> {
    let s = df.column(name)?;
    let list = s.list()?;
    let mut result = Vec::with_capacity(s.len());
    for row in list.into_iter() {
        let vals: Vec<i32> = row
            .map(|s| {
                s.i32()
                    .map(|c| c.into_no_null_iter().collect())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        result.push(vals);
    }
    Ok(result)
}

type Hands = Vec<[Vec<i32>; 4]>;

fn read_hand_cols(
    df: &DataFrame,
    prefix: &str,
) -> anyhow::Result<Hands> {
    let mut result: Vec<[Vec<i32>; 4]> =
        Vec::with_capacity(df.height());
    let cols: Vec<Vec<Vec<i32>>> = (0..4)
        .map(|p| {
            read_list_i32_any(
                df,
                &format!("{prefix}_p{p}"),
            )
        })
        .collect::<anyhow::Result<_>>()?;
    for i in 0..df.height() {
        result.push([
            cols[0].get(i).cloned().unwrap_or_default(),
            cols[1].get(i).cloned().unwrap_or_default(),
            cols[2].get(i).cloned().unwrap_or_default(),
            cols[3].get(i).cloned().unwrap_or_default(),
        ]);
    }
    Ok(result)
}

// ── 副露 JSON 解析 ─────────────────────────────────────────────

fn read_meld_json(
    df: &DataFrame,
    name: &str,
) -> anyhow::Result<Vec<Vec<MeldEntry>>> {
    let col = df.column(name)?.str()?;
    let mut result = Vec::with_capacity(col.len());
    for opt in col.into_iter() {
        result.push(parse_meld_json(
            opt.unwrap_or("[]"),
        ));
    }
    Ok(result)
}

/// 简易 meld JSON 解析：
/// `[{"type":"pon","tiles":[x,y,z],"called":c,"from":f,"discard_n":n},...]`
fn parse_meld_json(s: &str) -> Vec<MeldEntry> {
    let s = s.trim();
    if s == "[]" || s.is_empty() {
        return vec![];
    }
    let s = &s[1..s.len() - 1];
    let mut entries = vec![];
    for part in s.split("},{") {
        let part = part
            .trim()
            .trim_start_matches('{')
            .trim_end_matches('}');
        let mut meld_type = String::new();
        let mut tiles = vec![];
        let mut called_tile = 0i32;
        let mut from_player = -1i8;
        let mut discard_n = 0i32;
        let mut called_pos = -1i8;
        let mut current_val = String::new();
        for seg in part.split(',') {
            if seg.contains('[') && !seg.contains(']') {
                // 数组起始：记录 "tiles":[128 并加逗号等待后续元素
                current_val = seg.to_string();
                current_val.push(',');
                continue;
            }
            if !current_val.is_empty() {
                // 累积数组后续元素：补回被 split(',') 吃掉的逗号
                current_val.push(',');
                current_val.push_str(seg);
                if seg.contains(']') {
                    let kv: Vec<&str> = current_val
                        .splitn(2, ':')
                        .collect();
                    if kv.len() >= 2 {
                        let inner = kv[1]
                            .trim()
                            .trim_start_matches('[')
                            .trim_end_matches(']');
                        tiles = inner
                            .split(',')
                            .filter_map(|t| {
                                t.trim().parse::<i32>().ok()
                            })
                            .collect();
                    }
                    current_val.clear();
                }
                continue;
            }
            let kv: Vec<&str> =
                seg.splitn(2, ':').collect();
            if kv.len() >= 2 {
                match kv[0].trim().trim_matches('"') {
                    "type" => {
                        meld_type = kv[1]
                            .trim()
                            .trim_matches('"')
                            .to_string()
                    }
                    "called" => {
                        called_tile = kv[1]
                            .trim()
                            .trim_matches('"')
                            .parse()
                            .unwrap_or(0)
                    }
                    "from" => {
                        from_player = kv[1]
                            .trim()
                            .trim_matches('"')
                            .parse()
                            .unwrap_or(-1)
                    }
                    "discard_n" => {
                        discard_n = kv[1]
                            .trim()
                            .trim_matches('"')
                            .parse()
                            .unwrap_or(0)
                    }
                    "pos" => {
                        called_pos = kv[1]
                            .trim()
                            .trim_matches('"')
                            .parse()
                            .unwrap_or(-1)
                    }
                    _ => {}
                }
            }
        }
        entries.push(MeldEntry {
            meld_type,
            tiles,
            called_tile,
            from_player,
            discard_n,
            called_pos,
        });
    }
    entries
}
