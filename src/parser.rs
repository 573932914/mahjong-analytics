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

/// 解码天凤副露 packed integer。
/// 返回 `(meld_type, tiles, called_tile, from_player, called_pos)`
fn decode_meld(m: u32, who: i8) -> (String, Vec<i32>, i32, i8, i8) {
    if m & 0x0004 != 0 {
        let p = ((m >> 10) & 0x3F) as i32;
        let r = p % 3;
        let p = p / 3;
        let s = p / 7;
        let n0 = p % 7;
        let ids: Vec<i32> = (0..3).map(|i| (s * 9 + n0 + i) * 4).collect();
        let c = ids[r as usize];
        ("chi".into(), ids, c, ((who as i32 + 3) % 4) as i8, r as i8)
    } else if m & 0x0018 != 0 {
        let p = ((m >> 9) & 0x7F) as i32;
        let r = p % 3;
        let p = p / 3;
        let s = p / 9;
        let n = p % 9;
        let tid = s * 9 + n;
        if m & 0x0010 != 0 {
            ("kakan".into(), (0..4).map(|i| tid * 4 + i).collect(), 0, -1, -1)
        } else {
            let ts: Vec<i32> = (0..3).map(|i| tid * 4 + i).collect();
            let c = ts[r as usize];
            let f = ((who as i32 + 1 + r) % 4) as i8;
            ("pon".into(), ts, c, f, r as i8)
        }
    } else {
        let p = ((m >> 8) & 0xFF) as i32;
        let p = p / 4;
        let s = p / 9;
        let n = p % 9;
        let tid = s * 9 + n;
        ("ankan".into(), (0..4).map(|i| tid * 4 + i).collect(), 0, -1, -1)
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
                        let (tp, mut tiles, called_guess, _from_guess, called_pos) = decode_meld(m, who);
                        // decode 的 called 只是副本 0-2；实际被牌可能是副本 3。
                        // 从所有牌河中按全局弃牌 seq 找最近的那张作为真实被叫牌。
                        let type_id = called_guess / 4;
                        let mut called = called_guess;
                        let mut from = _from_guess;
                        let mut best_seq = -1i32;
                        for fp in 0..4 {
                            if fp == who as usize { continue; }
                            for (pos, &t) in gs.rivers[fp].iter().enumerate() {
                                if t / 4 == type_id && gs.river_seq[fp][pos] > best_seq {
                                    best_seq = gs.river_seq[fp][pos];
                                    called = t;
                                    from = fp as i8;
                                }
                            }
                        }
                        // 替换所有猜測 ID 为手牌中的真实实例（避免重复取同一实例）
                        let mut hand_copy = gs.hands[who as usize].clone();
                        for t in &mut tiles {
                            if *t == called_guess { *t = called; continue; }
                            let t_type = *t / 4;
                            if let Some(pos) = hand_copy.iter().position(|&h| h / 4 == t_type) {
                                *t = hand_copy.remove(pos);
                            }
                        }
                        let meld_type = if tp == "pon"
                            && gs.hands[who as usize].iter().filter(|&&t| t / 4 == called / 4).count() >= 3
                        { "daiminkan".to_string() } else { tp.clone() };
                        remove_meld_tiles(&mut gs, who, &meld_type, &tiles, called);
                        sort_tiles(&mut tiles);
                        let discard_n = gs.turns[who as usize];
                        gs.melds[who as usize].push(MeldEntry {
                            meld_type: meld_type.clone(),
                            tiles: tiles.clone(),
                            called_tile: called,
                            from_player: from,
                            discard_n,
                            called_pos,
                        });
                        gs.actor = who;
                        gs.last_draw = if called > 0 { called } else { gs.last_draw };
                        match meld_type.as_str() {
                            "chi" | "pon" => { pending_meld = Some((meld_type, called)); }
                            "daiminkan" | "shominkan" => {
                                gs.wall_remaining -= 1;
                                gs.next_draw_from_dead_wall = true;
                                emit_kan_snap(&mut gs, who, &meld_type, called);
                            }
                            "ankan" => {
                                gs.wall_remaining -= 1;
                                gs.next_draw_from_dead_wall = true;
                                emit_kan_snap(&mut gs, who, "ankan", tiles[0]);
                            }
                            "kakan" => {
                                gs.wall_remaining -= 1;
                                gs.next_draw_from_dead_wall = true;
                                emit_kan_snap(&mut gs, who, "kakan", tiles[3]);
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
        "ankan" => (gs.last_draw, -1, kan_tile),
        "kakan" => (gs.last_draw, -1, kan_tile),
        "daiminkan" | "shominkan" => (-1, kan_tile, -1),
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
        game_id: gs.game_id.clone(), seq: gs.seq, honba: gs.honba, oya: gs.oya,
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
        "chi" | "pon" | "daiminkan" | "shominkan" => {
            for &t in tiles { if t != called { if let Some(p) = h.iter().position(|&x| x == t) { h.remove(p); } } }
        }
        "kakan" => { if let Some(p) = h.iter().position(|&x| x == tiles[0]) { h.remove(p); } }
        _ => { for &t in tiles { if let Some(p) = h.iter().position(|&x| x == t) { h.remove(p); } } }
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
