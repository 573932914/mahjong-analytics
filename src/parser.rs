//! mjlog XML 解析器——状态机单次遍历。
//!
//! 将天凤 mjlog XML 转换为 [`Snapshot`] 序列。
//!
//! # 核心设计
//! - 单次遍历、零 DOM：基于 quick-xml 流式事件
//! - 全知视角：每条快照记录四家手牌（对手不可见但分析需要）
//! - 和了独立快照：每家和了各生成一行（支持多家ロン）
//! - 杠两段式：杠宣言 → 快照，次巡摸打 → 独立快照
//! - 流局不产生额外快照（最后一手切牌即为终态）
//!
//! # 牌值体系
//! 天凤实例 ID 0-135（34种×4枚），赤牌另有特殊 ID 34/37/40。
//! 快照中保留原始实例 ID，供 viz 端通过
//! [`crate::mahjong::tiles::instance_to_type`] 转换为牌种。

use std::collections::HashMap;
use quick_xml::events::Event;
use quick_xml::Reader;
use crate::snapshot::{GameState, MeldEntry, Snapshot};

// ── 标签解析辅助 ──────────────────────────────────────────────

fn split_tag(tag: &str) -> (char, Option<i32>) {
    if tag.len() < 2 {
        return (tag.chars().next().unwrap_or('?'), None);
    }
    let p = tag.chars().next().unwrap();
    (p, tag[1..].parse::<i32>().ok())
}

fn sort_tiles(t: &mut [i32]) {
    t.sort_unstable();
}

// ── 副露解码 ──────────────────────────────────────────────────

/// 解码天凤副露 packed integer（参照 tenhou0_to_mjai 编码规范）。
/// 返回 `(meld_type, tiles, called_tile, from_player, called_pos)`
///
/// # 入口分派
/// - `m & 0x04` → chi
/// - `m & 0x18` → pon/kakan（内部用 `m & 0x08` 区分 pon/kakan）
/// - `m & 0x20` → 三人麻将北抜き（不支持）
/// - 否则 → kan（`m & 0x03` 区分 ankan=0 / daiminkan≠0）
fn decode_meld(m: u32, who: i8) -> (String, Vec<i32>, i32, i8, i8) {
    if m & 0x0004 != 0 {
        // ── chi ────────────────────────────────────────────
        let from_who = (m & 0x03) as i8;
        let pattern = (m >> 10) & 0x3F;
        let called_idx = (pattern % 3) as i8;
        let pattern = pattern / 3;
        let start = pattern % 7;
        let suit = pattern / 7;
        let base = (suit * 9 + start) as i32;
        // 精确 copy indexes（bits 3-8）
        let copies = [
            ((m >> 3) & 0x03) as i32,
            ((m >> 5) & 0x03) as i32,
            ((m >> 7) & 0x03) as i32,
        ];
        let ids: Vec<i32> = (0..3)
            .map(|i| (base + i as i32) * 4 + copies[i])
            .collect();
        let called = ids[called_idx as usize];
        let from = ((who as i32 + from_who as i32) % 4) as i8;
        ("chi".into(), ids, called, from, called_idx)
    } else if m & 0x0018 != 0 {
        // ── pon / kakan ────────────────────────────────────
        let from_who = (m & 0x03) as i8;
        let pattern = (m >> 9) & 0x7F;
        let called_idx = (pattern % 3) as i8;
        let tile_kind = (pattern / 3) as i32;
        let added_idx = ((m >> 5) & 0x03) as usize;
        let tile_ids: Vec<i32> = (0..4).map(|i| tile_kind * 4 + i as i32).collect();
        if m & 0x0008 != 0 {
            // pon：排除 added_idx，called 在最前
            let all3: Vec<i32> =
                tile_ids.iter().enumerate()
                    .filter(|(i, _)| *i != added_idx)
                    .map(|(_, &t)| t)
                    .collect();
            // called_idx 索引的是排除 added_idx 后的 3 张集合，不是全部 4 张
            let called = all3[called_idx as usize];
            let consumed: Vec<i32> = all3.iter().filter(|&&t| t != called).copied().collect();
            let mut result = vec![called];
            result.extend(consumed);
            let from = ((who as i32 + from_who as i32) % 4) as i8;
            ("pon".into(), result, called, from, called_idx)
        } else {
            // kakan：added_idx 是加进 pon 的那张，放在最前
            let added = tile_ids[added_idx];
            let mut result = vec![added];
            for (i, &t) in tile_ids.iter().enumerate() {
                if i != added_idx { result.push(t); }
            }
            ("kakan".into(), result, added, -1, -1)
        }
    } else if m & 0x0020 != 0 {
        panic!("three-player kita meld is not supported (m={m})")
    } else {
        // ── kan（ankan / daiminkan）────────────────────────
        let from_who = (m & 0x03) as i8;
        let pattern = (m >> 8) & 0xFF;
        let called_idx = (pattern % 4) as i8;
        let tile_kind = (pattern / 4) as i32;
        let tile_ids: Vec<i32> = (0..4).map(|i| tile_kind * 4 + i as i32).collect();
        if from_who == 0 {
            // ankan：全部 4 张来自手牌
            ("ankan".into(), tile_ids, -1, -1, -1)
        } else {
            // daiminkan：called 来自他家，其余 3 张来自手牌
            let called = tile_ids[called_idx as usize];
            let from = ((who as i32 + from_who as i32) % 4) as i8;
            ("daiminkan".into(), tile_ids, called, from, called_idx)
        }
    }
}

// ── 公共入口 ──────────────────────────────────────────────────

pub fn parse_game_xml(xml: &str, game_id: &str) -> anyhow::Result<Vec<Snapshot>> {
    let mut r = Reader::from_str(xml);
    r.config_mut().trim_text(true);
    let mut gs = GameState::default();
    gs.game_id = game_id.to_string();
    let mut buf = Vec::new();
    let mut pending_meld: Option<(String, i32)> = None;

    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                let attrs = read_attrs(e, &r);
                let (pfx, to) = split_tag(&tag);

                match pfx {
                    'g' if tag == "go" => {
                        gs.game_type = attrs.get("type").and_then(|v| v.parse().ok()).unwrap_or(0);
                        gs.lobby = attrs.get("lobby").and_then(|v| v.parse().ok()).unwrap_or(0);
                    }
                    'u' if tag == "un" => {
                        for i in 0..4 { gs.names[i] = attrs.get(&format!("n{i}")).cloned().unwrap_or_default(); }
                        if let Some(d) = attrs.get("dan") { for (i, s) in d.split(',').take(4).enumerate() { gs.dans[i] = s.parse().unwrap_or(0); } }
                        if let Some(rt) = attrs.get("rate") { for (i, s) in rt.split(',').take(4).enumerate() { gs.rates[i] = s.parse().unwrap_or(1500.0); } }
                        if let Some(sx) = attrs.get("sx") { for (i, s) in sx.split(',').take(4).enumerate() { gs.sexes[i] = s.to_string(); } }
                        gs.num_players = if gs.dans[3] == 0 && gs.names[3].is_empty() { 3 } else { 4 };
                    }
                    't' if tag == "taikyoku" => {
                        gs.oya = attrs.get("oya").and_then(|v| v.parse().ok()).unwrap_or(0);
                    }
                    'i' if tag == "init" => {
                        reset_round(&mut gs, &attrs);
                        pending_meld = None;
                    }
                    't' | 'u' | 'v' | 'w' if to.is_some() => {
                        let tile = to.unwrap();
                        let p = match pfx { 't' => 0, 'u' => 1, 'v' => 2, _ => 3 };
                        if gs.next_draw_from_dead_wall { gs.next_draw_from_dead_wall = false; }
                        else { gs.wall_remaining -= 1; }
                        gs.last_draw = tile;
                        gs.actor = p;
                        gs.hands[p as usize].push(tile);
                        sort_tiles(&mut gs.hands[p as usize]);
                    }
                    'd' | 'e' | 'f' | 'g' if to.is_some() => {
                        let tile = to.unwrap();
                        let p = match pfx { 'd' => 0, 'e' => 1, 'f' => 2, _ => 3 };
                        let is_reach = gs.pending_reach_player == p;
                        if is_reach {
                            gs.riichi[p as usize] = true;
                            gs.riichi_seq[p as usize] = gs.seq;
                            gs.riichi_tile[p as usize] = tile;
                        }
                        if let Some((meld_type, called_tile)) = pending_meld.take() {
                            emit_meld_discard_snap(&mut gs, p, tile, &meld_type, called_tile);
                        } else {
                            emit_discard_snap(&mut gs, p, tile);
                        }
                        if is_reach { gs.pending_reach_player = -1; }
                        gs.hands[p as usize].retain(|&t| t != tile);
                        gs.rivers[p as usize].push(tile);
                        gs.river_tsumo[p as usize].push(gs.last_draw == tile);
                        gs.river_seq[p as usize].push(gs.seq);
                        gs.turns[p as usize] += 1;
                        gs.last_draw = -1;
                    }
                    'n' => {
                        let who: i8 = attrs.get("who").and_then(|v| v.parse().ok()).unwrap_or(0);
                        let m: u32 = attrs.get("m").and_then(|v| v.parse().ok()).unwrap_or(0);
                        let (tp, tiles, called, from, called_pos) = decode_meld(m, who);
                        remove_meld_tiles(&mut gs, who, &tp, &tiles, called);
                        sort_tiles(&mut gs.hands[who as usize]);
                        let mut sorted_tiles = tiles.clone();
                        sort_tiles(&mut sorted_tiles);
                        let discard_n = gs.turns[who as usize];
                        gs.melds[who as usize].push(MeldEntry {
                            meld_type: tp.clone(),
                            tiles: sorted_tiles,
                            called_tile: called,
                            from_player: from,
                            discard_n,
                            called_pos,
                        });
                        gs.actor = who;
                        gs.last_draw = if called > 0 { called } else { gs.last_draw };
                        match tp.as_str() {
                            "chi" | "pon" => { pending_meld = Some((tp, called)); }
                            "daiminkan" => {
                                gs.wall_remaining -= 1;
                                gs.next_draw_from_dead_wall = true;
                                emit_kan_snap(&mut gs, who, "daiminkan", called);
                            }
                            "ankan" => {
                                gs.wall_remaining -= 1;
                                gs.next_draw_from_dead_wall = true;
                                emit_kan_snap(&mut gs, who, "ankan", -1);
                            }
                            "kakan" => {
                                gs.wall_remaining -= 1;
                                gs.next_draw_from_dead_wall = true;
                                emit_kan_snap(&mut gs, who, "kakan", called);
                            }
                            _ => {}
                        }
                    }
                    'r' if tag.starts_with("reach") => {
                        let who: i8 = attrs.get("who").and_then(|v| v.parse().ok()).unwrap_or(0);
                        let step: i32 = attrs.get("step").and_then(|v| v.parse().ok()).unwrap_or(1);
                        if step == 1 { gs.pending_reach_player = who; }
                        else if step == 2 { gs.scores[who as usize] -= 10; gs.riichi_sticks += 1; if let Some(t) = attrs.get("ten") { update_scores(&mut gs.scores, t); } }
                    }
                    'd' if tag == "dora" => {
                        if let Some(h) = attrs.get("hai") { if let Ok(t) = h.parse::<i32>() { gs.dora_indicators.push(t); } }
                    }
                    'a' if tag == "agari" => {
                        let ow = attrs.contains_key("owari");
                        emit_agari_snap(&mut gs, &attrs);
                        if ow { break; }
                    }
                    'r' if tag == "ryuukyoku" => { backfill_ryuukyoku(&mut gs, &attrs); }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => { if String::from_utf8_lossy(e.name().as_ref()).to_lowercase() == "mjloggm" { break; } }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("XML parse error: {e}")),
            _ => {}
        }
        buf.clear();
    }
    Ok(std::mem::take(&mut gs.snapshots))
}

fn read_attrs(e: &quick_xml::events::BytesStart, _r: &Reader<&[u8]>) -> HashMap<String, String> {
    e.attributes().filter_map(|a| a.ok()).map(|a| (String::from_utf8_lossy(a.key.as_ref()).to_lowercase(), a.unescape_value().map(|v| v.to_string()).unwrap_or_default())).collect()
}

fn reset_round(gs: &mut GameState, a: &HashMap<String, String>) {
    gs.hands = [vec![], vec![], vec![], vec![]];
    gs.rivers = [vec![], vec![], vec![], vec![]];
    gs.river_tsumo = [vec![], vec![], vec![], vec![]];
    gs.river_seq = [vec![], vec![], vec![], vec![]];
    gs.melds = [vec![], vec![], vec![], vec![]];
    gs.riichi = [false; 4];
    gs.riichi_seq = [-1; 4];
    gs.riichi_tile = [-1; 4];
    gs.dora_indicators = vec![];
    gs.last_draw = -1;
    gs.wall_remaining = 70;
    gs.next_draw_from_dead_wall = false;
    gs.turns = [0; 4];
    gs.pending_reach_player = -1;
    gs.round_start_idx = gs.snapshots.len();
    if let Some(sd) = a.get("seed") {
        let p: Vec<&str> = sd.split(',').collect();
        if p.len() >= 6 {
            gs.round = p[0].parse().unwrap_or(0);
            gs.honba = p[1].parse().unwrap_or(0);
            gs.riichi_sticks = p[2].parse().unwrap_or(0);
            gs.dora_indicators.push(p[5].parse().unwrap_or(0));
        }
    }
    if let Some(t) = a.get("ten") { update_scores(&mut gs.scores, t); }
    gs.oya = a.get("oya").and_then(|v| v.parse().ok()).unwrap_or(0);
    gs.round_start_scores = gs.scores;
    for i in 0..4 {
        if let Some(h) = a.get(&format!("hai{i}")) {
            if !h.is_empty() { gs.hands[i] = h.split(',').filter_map(|s| s.parse().ok()).collect(); sort_tiles(&mut gs.hands[i]); }
        }
    }
}

// ── 快照生成 ──────────────────────────────────────────────────

fn emit_discard_snap(gs: &mut GameState, actor: i8, tile: i32) {
    let at = if gs.pending_reach_player == actor { "reach" }
    else if gs.last_draw == tile { "tsumogiri" }
    else { "discard" };
    let mut snap = capture_snapshot(gs, actor, at, gs.last_draw, -1, tile);
    snap.is_tsumogiri = gs.last_draw == tile;
    gs.snapshots.push(snap);
    gs.seq += 1;
}

fn emit_meld_discard_snap(gs: &mut GameState, actor: i8, tile: i32, meld_type: &str, called_tile: i32) {
    let snap = capture_snapshot(gs, actor, meld_type, -1, called_tile, tile);
    gs.snapshots.push(snap);
    gs.seq += 1;
}

fn emit_kan_snap(gs: &mut GameState, actor: i8, kan_type: &str, kan_tile: i32) {
    let (drawn_tile, called_tile, discard_tile) = match kan_type {
        "ankan" => (gs.last_draw, -1, -1),
        "kakan" => (gs.last_draw, kan_tile, -1),
        "daiminkan" => (-1, kan_tile, -1),
        _ => (-1, -1, -1),
    };
    let snap = capture_snapshot(gs, actor, kan_type, drawn_tile, called_tile, discard_tile);
    gs.snapshots.push(snap);
    gs.seq += 1;
}

fn emit_agari_snap(gs: &mut GameState, a: &HashMap<String, String>) {
    let who: i8 = a.get("who").and_then(|v| v.parse().ok()).unwrap_or(0);
    let from_who: i8 = a.get("fromwho").and_then(|v| v.parse().ok()).unwrap_or(who);
    let machi: i32 = a.get("machi").and_then(|v| v.parse().ok()).unwrap_or(-1);
    let (fu, points) = if let Some(ten) = a.get("ten") {
        let parts: Vec<&str> = ten.split(',').collect();
        (parts.first().and_then(|v| v.parse().ok()).unwrap_or(0),
         parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0))
    } else { (0, 0) };
    let (han_ids, total_han) = if let Some(yaku) = a.get("yaku") {
        let parts: Vec<i32> = yaku.split(',').filter_map(|v| v.parse().ok()).collect();
        let mut ids = Vec::new();
        let mut han_sum = 0i32;
        for chunk in parts.chunks(2) {
            if chunk.len() < 2 || chunk[1] == 0 { break; }
            ids.push(chunk[0]);
            han_sum += chunk[1];
        }
        (ids, han_sum)
    } else { (vec![], 0) };
    let is_tsumo = from_who == who;
    let (action_type, drawn_tile, called_tile, discard_tile) =
        if is_tsumo { ("tsumo", machi, -1, -1) } else { ("ron", -1, machi, -1) };
    let mut snap = capture_snapshot(gs, who, action_type, drawn_tile, called_tile, discard_tile);
    snap.agari_han_ids = han_ids;
    snap.agari_han = total_han;
    snap.agari_fu = fu;
    snap.agari_points = points;
    snap.agari_from = if is_tsumo { -1 } else { from_who };
    if let Some(ura) = a.get("dorahaiura") {
        snap.agari_ura_dora = ura.split(',').filter_map(|v| v.parse::<i32>().ok()).collect();
    }
    let end_kind: i8 = if is_tsumo { 1 } else { 2 };
    if let Some(sc) = a.get("sc") { apply_sc(&mut gs.scores, sc); }
    let final_scores = gs.scores;
    snap.round_end_kind = end_kind;
    snap.round_winner = who;
    snap.round_tenpai_count = -1;
    for i in 0..4 { snap.round_point_delta[i] = final_scores[i] - snap.scores[i]; }
    gs.snapshots.push(snap);
    gs.seq += 1;
    let start = gs.round_start_idx;
    let end = gs.snapshots.len() - 1;
    for s in &mut gs.snapshots[start..end] {
        s.round_end_kind = end_kind;
        s.round_winner = who;
        s.round_tenpai_count = -1;
        for i in 0..4 { s.round_point_delta[i] = final_scores[i] - s.scores[i]; }
    }
}

fn capture_snapshot(gs: &GameState, actor: i8, action_type: &str, drawn_tile: i32, called_tile: i32, discard_tile: i32) -> Snapshot {
    let hand_before: [Vec<i32>; 4] = [gs.hands[0].clone(), gs.hands[1].clone(), gs.hands[2].clone(), gs.hands[3].clone()];
    Snapshot {
        game_id: gs.game_id.clone(), seq: gs.seq, round: gs.round, honba: gs.honba, oya: gs.oya,
        turns: gs.turns, actor, hand_before,
        river_p0: gs.rivers[0].clone(), river_p1: gs.rivers[1].clone(),
        river_p2: gs.rivers[2].clone(), river_p3: gs.rivers[3].clone(),
        river_tsumo_p0: gs.river_tsumo[0].clone(), river_tsumo_p1: gs.river_tsumo[1].clone(),
        river_tsumo_p2: gs.river_tsumo[2].clone(), river_tsumo_p3: gs.river_tsumo[3].clone(),
        melds_p0: gs.melds[0].clone(), melds_p1: gs.melds[1].clone(),
        melds_p2: gs.melds[2].clone(), melds_p3: gs.melds[3].clone(),
        scores: gs.scores, riichi: gs.riichi,
        riichi_turn: gs.riichi_seq, riichi_tile: gs.riichi_tile,
        dora_indicators: gs.dora_indicators.clone(),
        wall_remaining: gs.wall_remaining, riichi_sticks: gs.riichi_sticks,
        action_type: action_type.to_string(), drawn_tile, called_tile, discard_tile,
        round_end_kind: -1, round_winner: -1, round_point_delta: [0; 4], round_tenpai_count: -1,
        agari_han_ids: vec![], agari_han: 0, agari_fu: 0, agari_points: 0, agari_from: -1, agari_ura_dora: vec![],
        is_tsumogiri: false,
    }
}

fn remove_meld_tiles(gs: &mut GameState, who: i8, mt: &str, tiles: &[i32], called: i32) {
    let h = &mut gs.hands[who as usize];
    match mt {
        "chi" | "pon" | "daiminkan" => {
            // called 来自他家，不剔除；其余 consumed 从手牌移除
            for &t in tiles { if t != called { if let Some(p) = h.iter().position(|&x| x == t) { h.remove(p); } } }
        }
        "kakan" => {
            // called = 加进 pon 的那张牌，从手牌移除
            if let Some(p) = h.iter().position(|&x| x == called) { h.remove(p); }
        }
        _ => {
            // ankan: 全部 4 张来自手牌
            for &t in tiles { if let Some(p) = h.iter().position(|&x| x == t) { h.remove(p); } }
        }
    }
}

fn update_scores(scores: &mut [i32; 4], s: &str) {
    for (i, v) in s.split(',').take(4).enumerate() { if let Ok(x) = v.parse::<i32>() { scores[i] = x; } }
}

fn apply_sc(scores: &mut [i32; 4], s: &str) {
    let vals: Vec<i32> = s.split(',').filter_map(|v| v.parse::<i32>().ok()).collect();
    for i in 0..4 { let di = i * 2 + 1; if di < vals.len() { scores[i] += vals[di]; } }
}

fn backfill_ryuukyoku(gs: &mut GameState, a: &HashMap<String, String>) {
    let mut count = 0i8;
    for i in 0..4 { if a.contains_key(&format!("hai{i}")) || gs.riichi[i] { count += 1; } }
    if let Some(sc) = a.get("sc") { apply_sc(&mut gs.scores, sc); }
    let final_scores = gs.scores;
    let start = gs.round_start_idx;
    for s in &mut gs.snapshots[start..] {
        s.round_end_kind = 0; s.round_winner = -1;
        s.round_tenpai_count = count; s.riichi = gs.riichi;
        for i in 0..4 { s.round_point_delta[i] = final_scores[i] - s.scores[i]; }
    }
}
