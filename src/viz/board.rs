use std::collections::HashSet;
use std::f32::consts::PI;

use egui::{Color32, CornerRadius, Pos2, Stroke, Ui, Vec2};
use egui::epaint::Mesh;

use crate::mahjong;
use crate::snapshot::Snapshot;
use crate::viz::tiles::{self, TileAssets};

const TILE_W: f32 = 38.0;
const TILE_H: f32 = 52.0;
const RIVER_COLS: usize = 6;
const GAP: f32 = 2.0;

pub fn draw_board(ui: &mut Ui, snap: &Snapshot, assets: Option<&TileAssets>) {
    let av = ui.available_size();
    let side = av.x.min(av.y) - 16.0;
    let board = egui::Rect::from_center_size(
        ui.available_rect_before_wrap().center(), Vec2::new(side, side));
    ui.painter().rect_filled(board, CornerRadius::same(8), Color32::from_rgb(10, 90, 40));
    ui.painter().rect_stroke(board, CornerRadius::same(8),
        Stroke::new(2.0_f32, Color32::from_rgb(0, 60, 20)), egui::StrokeKind::Middle);

    let stolen = collect_stolen(snap);
    let cx = board.center().x;
    let cy = board.center().y;
    let panel_side = side * 0.34;
    let panel = egui::Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(panel_side, panel_side));
    draw_centre_panel(ui, snap, panel, assets);

    let base_y = board.bottom() - TILE_H - 8.0;
    let panel_bot = panel.bottom();
    for (idx, angle) in [(0, 0.0), (1, -PI/2.0), (2, PI), (3, PI/2.0)] {
        draw_player(ui, snap, idx, cx, cy, base_y, panel_bot, angle, assets, &stolen);
    }
}

fn collect_stolen(snap: &Snapshot) -> HashSet<(usize, i32)> {
    let mut s = HashSet::new();
    for p in 0..4 {
        for meld in snap.melds_for(p).iter() {
            if meld.from_player >= 0 && meld.called_tile > 0 {
                s.insert((meld.from_player as usize, meld.called_tile));
            }
        }
    }
    s
}

fn draw_centre_panel(ui: &mut Ui, snap: &Snapshot, panel: egui::Rect, assets: Option<&TileAssets>) {
    let p = panel;
    ui.painter().rect_filled(p, CornerRadius::same(6), Color32::from_rgb(15, 50, 25));
    ui.painter().rect_stroke(p, CornerRadius::same(6),
        Stroke::new(2.0_f32, Color32::from_rgb(80, 150, 40)), egui::StrokeKind::Middle);

    // 宝牌指示牌在风盘正中心, 正常大小
    let n = snap.dora_indicators.len().max(1) as f32;
    let total_w = n * TILE_W + (n - 1.0) * GAP;
    let mut dx = p.center().x - total_w / 2.0;
    let dy = p.center().y - TILE_H / 2.0;
    for &d in &snap.dora_indicators {
        draw_tile(ui, Pos2::new(dx, dy), d, TILE_W, TILE_H, 0.0, 0.0, 0.0, 0.0, assets, Color32::WHITE, false, false);
        dx += TILE_W + GAP;
    }
}

fn draw_player(
    ui: &mut Ui, snap: &Snapshot, idx: usize,
    rot_cx: f32, rot_cy: f32, base_y: f32, panel_edge_y: f32,
    angle: f32, assets: Option<&TileAssets>, stolen: &HashSet<(usize, i32)>,
) {
    let hand = &snap.hand_before[idx];
    let river = snap.river_for(idx);
    let is_actor = idx as i8 == snap.actor;
    let ox = rot_cx - (hand.len().max(5) as f32 * TILE_W) / 2.0;
    let oy = base_y;

    draw_hand(ui, hand, snap.drawn_tile, is_actor, ox, oy, rot_cx, rot_cy, angle, assets);
    draw_river(ui, river, snap.river_tsumo_for(idx), snap, idx, ox, panel_edge_y, rot_cx, rot_cy, angle, assets, stolen);
    draw_melds(ui, snap.melds_for(idx), idx, ox, oy, rot_cx, rot_cy, angle, assets);
}

fn draw_hand(ui: &mut Ui, hand: &[i32], drawn: i32, is_actor: bool,
    ox: f32, oy: f32, rot_cx: f32, rot_cy: f32, angle: f32, assets: Option<&TileAssets>,
) {
    let (sorted, dr) = if is_actor && drawn >= 0 {
        mahjong::tiles::sort_hand_with_draw(hand, drawn)
    } else { (mahjong::tiles::sort_hand(hand), -1) };
    let mut display = sorted.clone();
    if dr >= 0 && !display.contains(&dr) { display.push(dr); }
    let mut lx = 0.0f32;
    for (i, &id) in display.iter().enumerate() {
        if dr >= 0 && id == dr && i == display.len() - 1 { lx += 12.0; }
        draw_tile(ui, Pos2::new(ox+lx, oy), id, TILE_W, TILE_H, rot_cx, rot_cy, angle, 0.0, assets, Color32::WHITE, false, false);
        lx += TILE_W;
    }
}

fn draw_river(
    ui: &mut Ui, river: &[i32], river_tsumo: &[bool], snap: &Snapshot, player_idx: usize,
    _ox: f32, panel_edge_y: f32,
    rot_cx: f32, rot_cy: f32, angle: f32,
    assets: Option<&TileAssets>, stolen: &HashSet<(usize, i32)>,
) {
    if river.is_empty() { return; }

    let mut stolen_indices = HashSet::new();
    for &(from_p, inst_id) in stolen.iter() {
        if from_p == player_idx {
            if let Some(pos) = river.iter().position(|&t| t == inst_id) {
                stolen_indices.insert(pos);
            }
        }
    }

    let river_w = RIVER_COLS as f32 * TILE_W + (RIVER_COLS - 1) as f32 * GAP;
    let rx = rot_cx - river_w / 2.0;
    let ry = panel_edge_y + 4.0;
    let mut row_shift: Vec<f32> = vec![0.0; river.len() / RIVER_COLS + 1];

    for (i, &tile) in river.iter().enumerate() {
        let col = i % RIVER_COLS;
        let row = i / RIVER_COLS;
        let is_stolen = stolen_indices.contains(&i);
        let is_riichi = tile == snap.riichi_tile[player_idx] && snap.riichi_tile[player_idx] >= 0;
        // 摸切标记来自牌河持久化数据
        let is_tsumo = river_tsumo.get(i).copied().unwrap_or(false);

        let x = rx + col as f32 * (TILE_W + GAP) + row_shift[row];
        let y = ry + row as f32 * (TILE_H + GAP);
        let local_rot = if is_riichi { -PI / 2.0 } else { 0.0 };
        let x_adj = if is_riichi { (TILE_H - TILE_W) / 2.0 } else { 0.0 };
        if is_riichi { row_shift[row] += TILE_H - TILE_W + GAP; }

        // 摸切灰色掩膜 (~75% opacity)
        let tint = if is_tsumo {
            Color32::from_rgba_premultiplied(100, 100, 100, 190)
        } else { Color32::WHITE };

        draw_tile(ui, Pos2::new(x + x_adj, y), tile, TILE_W, TILE_H,
            rot_cx, rot_cy, angle, local_rot, assets, tint, is_stolen, false);
    }
}

fn draw_melds(
    ui: &mut Ui, melds: &[crate::snapshot::MeldEntry], player_idx: usize,
    ox: f32, oy: f32, rot_cx: f32, rot_cy: f32, angle: f32,
    assets: Option<&TileAssets>,
) {
    if melds.is_empty() { return; }
    let meld_start_y = oy - TILE_H - 8.0;
    let mut mx = ox - TILE_W; // 从手牌左侧开始, 稍向左偏

    for meld in melds.iter() {
        let n = meld.tiles.len();

        // 暗杠: [牌背][牌][牌][牌背]
        if meld.meld_type == "ankan" && n == 4 {
            let layout = [-1, meld.tiles[0], meld.tiles[1], -1];
            for (j, &t) in layout.iter().enumerate() {
                let tx = mx + j as f32 * (TILE_W + GAP);
                let is_b = t == -1;
                draw_tile(ui, Pos2::new(tx, meld_start_y), if is_b { 0 } else { t },
                    TILE_W, TILE_H, rot_cx, rot_cy, angle, 0.0, assets, Color32::WHITE, false, is_b);
            }
            mx += 4.0 * TILE_W + 16.;
            continue;
        }

        if n != 3 { mx += n as f32 * TILE_W + 16.; continue; }

        let has_call = meld.from_player >= 0;
        let display_pos: usize = if has_call {
            match (meld.from_player + 4 - player_idx as i8) % 4 {
                1 => 2, 2 => 1, 3 => 0, _ => 1,
            }
        } else { 1 };

        let mut display: [i32; 3] = [-1; 3];
        {
            let others: Vec<i32> = meld.tiles.iter().copied()
                .filter(|&t| !has_call || t != meld.called_tile).collect();
            let mut oi = 0;
            for p in 0..3 {
                if p == display_pos && has_call { display[p] = meld.called_tile; }
                else if oi < others.len() { display[p] = others[oi]; oi += 1; }
            }
        }

        let ext = (TILE_H - TILE_W) / 2.0;
        for (j, &t) in display.iter().enumerate() {
            if t < 0 { continue; }
            let is_called = has_call && t == meld.called_tile;
            let mut tx = mx + j as f32 * (TILE_W + GAP);
            if has_call {
                if is_called { tx += ext; }
                else if j < display_pos { tx -= ext; }
                else { tx += ext; }
            }
            let local_rot = if is_called { -PI / 2.0 } else { 0.0 };
            draw_tile(ui, Pos2::new(tx, meld_start_y), t, TILE_W, TILE_H,
                rot_cx, rot_cy, angle, local_rot, assets, Color32::WHITE, false, false);
        }

        mx += 3.0 * TILE_W + 16.;
    }
}

// ── 牌面绘制 (border/stolen 在旋转前渲染) ────────

pub fn draw_tile(
    ui: &mut Ui, pos: Pos2, instance_id: i32, w: f32, h: f32,
    rot_cx: f32, rot_cy: f32, global_angle: f32, local_angle: f32,
    assets: Option<&TileAssets>, tint: Color32, stolen: bool, is_back: bool,
) {
    let tcx = pos.x + w / 2.0;
    let tcy = pos.y + h / 2.0;
    let (tcx, tcy) = rotate_pt(tcx, tcy, rot_cx, rot_cy, global_angle);

    let tex = if is_back {
        assets.and_then(|a| a.back_texture())
    } else {
        let type_id = tiles::instance_to_type(instance_id);
        assets.and_then(|a| a.texture(type_id))
    };

    if let Some(tex) = tex {
        // 红色边框(被盗牌): 独立 mesh, 先于牌面绘制
        if stolen {
            let bw = w + 8.0; let bh = h + 8.0;
            let br = egui::emath::Rect::from_center_size(egui::pos2(tcx, tcy), Vec2::new(bw, bh));
            let mut bm = Mesh::with_texture(tex.id());
            bm.add_rect_with_uv(br, egui::emath::Rect::from_min_max(egui::pos2(0.,0.), egui::pos2(1.,1.)), Color32::RED);
            if global_angle.abs() > 0.001 { rotate_mesh(&mut bm, tcx, tcy, global_angle); }
            if local_angle.abs() > 0.001 { rotate_mesh(&mut bm, tcx, tcy, local_angle); }
            ui.painter().add(egui::Shape::mesh(bm));
        }

        let rect = egui::emath::Rect::from_center_size(egui::pos2(tcx, tcy), Vec2::new(w, h));
        let mut mesh = Mesh::with_texture(tex.id());
        mesh.add_rect_with_uv(rect, egui::emath::Rect::from_min_max(egui::pos2(0.,0.), egui::pos2(1.,1.)), tint);
        if global_angle.abs() > 0.001 { rotate_mesh(&mut mesh, tcx, tcy, global_angle); }
        if local_angle.abs() > 0.001 { rotate_mesh(&mut mesh, tcx, tcy, local_angle); }
        ui.painter().add(egui::Shape::mesh(mesh));
        return;
    }
    // text fallback
    let rect = egui::Rect::from_center_size(egui::pos2(tcx, tcy), Vec2::new(w, h));
    if stolen {
        let br = egui::Rect::from_center_size(egui::pos2(tcx, tcy), Vec2::new(w+4., h+4.));
        ui.painter().rect_filled(br, CornerRadius::same(2), Color32::RED);
    }
    if is_back {
        ui.painter().rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(20, 60, 120));
        ui.painter().rect_stroke(rect, CornerRadius::same(2), Stroke::new(0.5_f32, Color32::BLACK), egui::StrokeKind::Middle);
        return;
    }
    let type_id = tiles::instance_to_type(instance_id);
    let base = tiles::tile_color(type_id);
    let color = if tint == Color32::WHITE { base } else {
        Color32::from_rgb((base.r() as u16 * tint.r() as u16 / 255) as u8,
            (base.g() as u16 * tint.g() as u16 / 255) as u8,
            (base.b() as u16 * tint.b() as u16 / 255) as u8)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(2), color);
    ui.painter().rect_stroke(rect, CornerRadius::same(2),
        Stroke::new(0.5_f32, Color32::BLACK), egui::StrokeKind::Middle);
    let name = tiles::tile_name(type_id);
    let fs = if w < 30.0 { 8.0 } else { 12.0 };
    let lbl = if tiles::is_red_dora(type_id) {
        egui::RichText::new(name).color(Color32::RED).strong().font(egui::FontId::monospace(fs))
    } else { egui::RichText::new(name).color(Color32::WHITE).font(egui::FontId::monospace(fs)) };
    ui.put(rect, egui::Label::new(lbl));
}

fn rotate_mesh(mesh: &mut Mesh, cx: f32, cy: f32, angle: f32) {
    let (ca, sa) = (angle.cos(), angle.sin());
    for v in &mut mesh.vertices {
        let dx = v.pos.x - cx; let dy = v.pos.y - cy;
        v.pos = egui::pos2(cx + dx*ca - dy*sa, cy + dx*sa + dy*ca);
    }
}

fn rotate_pt(x: f32, y: f32, cx: f32, cy: f32, a: f32) -> (f32, f32) {
    if a.abs() < 0.001 { return (x, y); }
    let dx = x - cx; let dy = y - cy;
    let (ca, sa) = (a.cos(), a.sin());
    (cx + dx*ca - dy*sa, cy + dx*sa + dy*ca)
}
