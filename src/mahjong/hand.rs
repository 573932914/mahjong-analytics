//! 手牌评估：向听数、听牌判定、和牌判定、简易得点计算。
//!
//! # 内部表示
//! 手牌用 34 元素的 `[u8; 34]` 数组表示，索引 = 牌种
//! （0-8=万子, 9-17=筒子, 18-26=索子, 27-33=字牌）。
//! 赤牌合并到普通牌（向听/听牌判定中视为同一牌种）。
//!
//! # 向听数算法
//! 标准"遍历雀头候选 + 递归取面子"求最小向听数。
//! - 14 枚手牌: 4面子1雀头 → 和牌 (向听=-1)
//! - 13 枚手牌: 差 1 枚 → 听牌 (向听=0)
//! - 向听 = (4 - 完成面子数) × 2 - 部分面子数 - 雀头有无
//!
//! # 得点计算
//! 符计算 + 飜数 → 查点数表（简易版，不含全部役种）。

/// 天凤 tile ID → 34 元素数组的索引。
///
/// 赤牌 (34/37/40) 合并到对应普通牌的索引。
fn tile_to_index34(id: i32) -> usize {
    match id {
        0..=8 => id as usize,                     // 1m-9m
        9..=17 => (id - 9) as usize + 9,           // 1p-9p
        18..=26 => (id - 18) as usize + 18,         // 1s-9s
        27..=33 => (id - 27) as usize + 27,         // 东南西北白发中
        34 => 4,   // 赤5m → 索引4 (5m)
        37 => 13,  // 赤5p → 索引13 (5p)
        40 => 22,  // 赤5s → 索引22 (5s)
        _ => 0,
    }
}

/// 34 元素索引 → 代表 tile ID（取该牌种的第一个 ID）。
fn index34_to_id(idx: usize) -> i32 {
    match idx {
        0..=8 => idx as i32,
        9..=17 => (idx - 9) as i32 + 9,
        18..=26 => (idx - 18) as i32 + 18,
        27..=33 => (idx - 27) as i32 + 27,
        _ => 0,
    }
}

/// 手牌 tile ID 列表 → 34 元素计数数组。
///
/// 每枚牌的赤/普通差异被忽略，统一累加到对应索引。
pub fn hand_to_counts(hand: &[i32]) -> [u8; 34] {
    let mut counts = [0u8; 34];
    for &tile in hand {
        let idx = tile_to_index34(tile);
        if idx < 34 {
            counts[idx] += 1;
        }
    }
    counts
}

// ── 向听数计算 ────────────────────────────────────────────────

/// 计算手牌的向听数。
///
/// # 参数
/// - `hand`: 手牌 tile ID 列表（13 或 14 枚）
///
/// # 返回
/// - `-1` = 和牌形
/// - `0` = 听牌
/// - `1`+ = 向听数（差几张听牌）
pub fn shanten(hand: &[i32]) -> i32 {
    let counts = hand_to_counts(hand);
    shanten_from_counts(&counts)
}

/// 从 34 元素计数数组直接计算向听数。
fn shanten_from_counts(counts: &[u8; 34]) -> i32 {
    let total_tiles: u8 = counts.iter().sum();
    let _num_melds_needed = (total_tiles / 3).min(4) as i32;

    let mut best = 8i32; // 初始化为最大向听数+1

    // 尝试每种可能的雀头
    for head in 0..34 {
        if counts[head] >= 2 {
            let mut c = *counts;
            c[head] -= 2; // 取出雀头
            let melds = count_melds_and_tatsu(&c);
            // 向听 = 8 - 2×完成面子 - min(塔子, 剩余需要面子数) - 1(雀头已取)
            let shanten_val =
                8 - 2 * melds.0 - melds.1.min(4 - melds.0) - 1;
            best = best.min(shanten_val);
        }
    }

    // 无雀头的情况（13枚手牌）
    let melds = count_melds_and_tatsu(counts);
    let shanten_val =
        8 - 2 * melds.0 - melds.1.min(4 - melds.0);
    best = best.min(shanten_val);

    // ── 国士无双特殊判定 ────────────────────────
    // 13 种幺九牌各至少 1 枚 + 任意 1 种成对
    let yaochu_idx = [0,8,9,17,18,26,27,28,29,30,31,32,33];
    let yaochu_kinds: i32 = yaochu_idx
        .iter()
        .filter(|&&i| counts[i] > 0)
        .count() as i32;
    let yaochu_pairs: i32 = yaochu_idx
        .iter()
        .filter(|&&i| counts[i] >= 2)
        .count() as i32;
    let kokushi_shanten = if yaochu_pairs > 0 {
        13 - yaochu_kinds - 1 // 已有一种雀头
    } else {
        13 - yaochu_kinds
    };
    best = best.min(kokushi_shanten);

    // ── 七对子特殊判定 ──────────────────────────
    let pairs: i32 =
        (0..34).filter(|&i| counts[i] >= 2).count() as i32;
    let chiitoi_shanten = 6 - pairs;
    best = best.min(chiitoi_shanten);

    best
}

/// 计算完成面子数和塔子数（递归取走最优）。
///
/// # 返回
/// - `(完成面子数, 塔子数)`
///   塔子 = 差 1 枚即可完成的面子候选（两面/边张/嵌张/对子）
fn count_melds_and_tatsu(counts: &[u8; 34]) -> (i32, i32) {
    let mut best_melds = 0i32;
    let mut best_tatsu = 0i32;

    count_recursive(
        counts, 0, 0, 0,
        &mut best_melds, &mut best_tatsu,
    );

    (best_melds, best_tatsu)
}

/// 递归取面子：对每种牌尝试取出刻子或顺子，
/// 剩余牌数 1-2 枚的计为塔子。
///
/// # 参数
/// - `counts`: 当前剩余牌计数
/// - `idx`: 当前处理的牌种索引
/// - `melds`: 已取出的完成面子数
/// - `tatsu`: 已取出的塔子数
/// - `best_melds/best_tatsu`: 全局最优解
fn count_recursive(
    counts: &[u8; 34],
    idx: usize,
    melds: i32,
    tatsu: i32,
    best_melds: &mut i32,
    best_tatsu: &mut i32,
) {
    // 终了条件：处理完所有 34 种牌
    if idx >= 34 {
        let score = melds * 2 + tatsu;
        let best_score = *best_melds * 2 + *best_tatsu;
        if score > best_score
            || (score == best_score && melds > *best_melds)
        {
            *best_melds = melds;
            *best_tatsu = tatsu;
        }
        return;
    }

    if counts[idx] == 0 {
        count_recursive(
            counts, idx + 1, melds, tatsu,
            best_melds, best_tatsu,
        );
        return;
    }

    let mut c = *counts;

    // 尝试取刻子（3 枚相同）
    if c[idx] >= 3 {
        c[idx] -= 3;
        // 尝试同种牌再取一个刻子（如 6 枚同种）
        if c[idx] >= 3 {
            let mut c2 = c;
            c2[idx] -= 3;
            count_recursive(
                &c2, idx, melds + 2, tatsu,
                best_melds, best_tatsu,
            );
        }
        count_recursive(
            &c, idx, melds + 1, tatsu,
            best_melds, best_tatsu,
        );
        c[idx] += 3;
    }

    // 尝试取顺子（仅万/筒/索，且 idx%9 ≤ 6 保证不跨花色）
    if idx < 27 && (idx % 9) <= 6 {
        if c[idx] > 0 && c[idx + 1] > 0 && c[idx + 2] > 0 {
            c[idx] -= 1;
            c[idx + 1] -= 1;
            c[idx + 2] -= 1;
            count_recursive(
                &c, idx, melds + 1, tatsu,
                best_melds, best_tatsu,
            );
            c[idx] += 1;
            c[idx + 1] += 1;
            c[idx + 2] += 1;
        }
    }

    // 剩余 1-2 枚作为塔子
    if c[idx] == 2 {
        count_recursive(
            counts, idx + 1, melds, tatsu + 1,
            best_melds, best_tatsu,
        );
    } else if c[idx] == 1 {
        // 尝试相邻牌组成两面/边张/嵌张塔子
        if idx < 27 && (idx % 9) <= 7 {
            if counts[idx + 1] >= 1 {
                let mut c2 = *counts;
                c2[idx] -= 1;
                c2[idx + 1] -= 1;
                count_recursive(
                    &c2, idx + 1, melds, tatsu + 1,
                    best_melds, best_tatsu,
                );
            } else if idx % 9 <= 6 && counts[idx + 2] >= 1 {
                let mut c2 = *counts;
                c2[idx] -= 1;
                c2[idx + 2] -= 1;
                count_recursive(
                    &c2, idx + 1, melds, tatsu + 1,
                    best_melds, best_tatsu,
                );
            }
        }
        count_recursive(
            counts, idx + 1, melds, tatsu,
            best_melds, best_tatsu,
        );
    } else {
        count_recursive(
            counts, idx + 1, melds, tatsu,
            best_melds, best_tatsu,
        );
    }
}

// ── 听牌判定 ──────────────────────────────────────────────────

/// 判断是否听牌，返回听牌时的待牌列表。
///
/// 对全部 34 种牌逐一尝试添加 1 枚，若向听数降为 -1 则
/// 该牌种为待牌。
///
/// # 参数
/// - `hand`: 13 枚手牌 tile ID 列表
///
/// # 返回
/// - `Some(waiting_tiles)`: 已听牌，`waiting_tiles` 列出所有
///   能和牌的牌（含重复，反映剩余枚数）
/// - `None`: 未听牌
pub fn is_tenpai(hand: &[i32]) -> Option<Vec<i32>> {
    let counts = hand_to_counts(hand);
    let mut waiting = vec![];

    for i in 0..34 {
        if counts[i] < 4 {
            // 该牌种尚有剩余
            let mut c = counts;
            c[i] += 1;
            if shanten_from_counts(&c) < 0 {
                let remaining = 4 - counts[i] as i32;
                for _ in 0..remaining {
                    waiting.push(index34_to_id(i));
                }
            }
        }
    }

    if waiting.is_empty() {
        None
    } else {
        Some(waiting)
    }
}

// ── 和牌判定 ──────────────────────────────────────────────────

/// 判断 14 枚手牌是否构成和牌形。
///
/// 检查三种和牌形：
/// 1. 通常形：4 面子 + 1 雀头
/// 2. 国士无双：13 种幺九牌各 ≥1，任意一种 ≥2
/// 3. 七对子：7 组对子
///
/// # 参数
/// - `hand`: 14 枚手牌 tile ID 列表（含和了牌）
pub fn is_agari(hand: &[i32]) -> bool {
    if hand.len() % 3 != 2 {
        return false; // 和牌形必须是 3n+2 枚
    }
    let counts = hand_to_counts(hand);

    // 通常形判定：遍历所有雀头候选
    for head in 0..34 {
        if counts[head] >= 2 {
            let mut c = counts;
            c[head] -= 2; // 取出雀头
            if can_form_all_melds(&c, 4) {
                return true; // 剩余牌能组成 4 面子
            }
        }
    }

    // 国士无双
    if is_kokushi(&counts) {
        return true;
    }

    // 七对子
    if (0..34).filter(|&i| counts[i] == 2).count() == 7 {
        return true;
    }

    false
}

/// 递归判定剩余牌能否组成指定数量的面子。
///
/// # 参数
/// - `counts`: 当前剩余牌计数（34 元素）
/// - `melds_needed`: 还需取出的面子数
fn can_form_all_melds(counts: &[u8; 34], melds_needed: i32) -> bool {
    if melds_needed == 0 {
        return counts.iter().all(|&c| c == 0);
    }

    // 找第一个非零位置
    let first = match (0..34).find(|&i| counts[i] > 0) {
        Some(i) => i,
        None => return melds_needed == 0,
    };

    let mut c = *counts;

    // 尝试刻子
    if c[first] >= 3 {
        c[first] -= 3;
        if can_form_all_melds(&c, melds_needed - 1) {
            return true;
        }
        c[first] += 3;
    }

    // 尝试顺子（万/筒/索，且不跨花色边界）
    if first < 27 && (first % 9) <= 6 {
        if c[first] > 0 && c[first + 1] > 0 && c[first + 2] > 0 {
            c[first] -= 1;
            c[first + 1] -= 1;
            c[first + 2] -= 1;
            if can_form_all_melds(&c, melds_needed - 1) {
                return true;
            }
        }
    }

    false
}

/// 国士无双形判定：13 种幺九牌各 ≥1，且至少一种 ≥2。
fn is_kokushi(counts: &[u8; 34]) -> bool {
    let yaochu_idx = [0,8,9,17,18,26,27,28,29,30,31,32,33];
    let has_all = yaochu_idx.iter().all(|&i| counts[i] >= 1);
    let has_pair = yaochu_idx.iter().any(|&i| counts[i] >= 2);
    has_all && has_pair
}

// ── 简易得点计算 ──────────────────────────────────────────────

/// 和牌时的得点计算结果。
#[derive(Debug, Clone)]
pub struct AgariScore {
    /// 符数（已按 10 为单位向上取整）。
    pub fu: i32,
    /// 飜数。
    pub han: i32,
    /// 基本点（ロン时 ×4、ツモ时 亲×2·子×1）。
    pub base_points: i32,
    /// 实际点数授受。
    pub payment: Payment,
}

/// 点数授受明细。
#[derive(Debug, Clone)]
pub struct Payment {
    /// ロン和了时放铳者支付额。
    pub ron: i32,
    /// ツモ和了时每子支付额。
    pub tsumo_ko: i32,
    /// ツモ和了时亲支付额（亲ツモ时为 0）。
    pub tsumo_oya: i32,
}

/// 简易得点计算。
///
/// 计算符数 + 飜数 → 查点数表。
///
/// # 参数
/// - `hand`: 和了牌在内的 14 枚手牌
/// - `agari_tile`: 和了牌 tile ID
/// - `is_menzen`: 是否门前清
/// - `is_tsumo`: 是否自摸和了
/// - `is_oya`: 是否亲家
/// - `han`: 飜数（调用方统计役种后传入）
///
/// # 简易版限制
/// 符计算仅包含基本部分（平和·七对子·食い平和未单独处理）。
/// 役种判定由调用方完成，飜数通过 `han` 参数汇总传入。
pub fn calc_score(
    hand: &[i32],
    _agari_tile: i32,
    is_menzen: bool,
    is_tsumo: bool,
    is_oya: bool,
    han: i32,
) -> AgariScore {
    let counts = hand_to_counts(hand);

    // ── 符计算 ──────────────────────────────────
    let mut fu = 20; // 基本符（和了底符）

    // 门前ロン +10 符
    if is_menzen && !is_tsumo {
        fu += 10;
    }

    // ツモ和了 +2 符
    if is_tsumo {
        fu += 2;
    }

    // 役牌雀头 +2 符
    for &i in &[27, 28, 29, 30, 31, 32, 33] {
        if counts[i] >= 2 {
            fu += 2;
            break;
        }
    }

    // 刻子/槓子的加符（中张: 2/4符, 幺九: 4/8符, 槓×2）
    for i in 0..34 {
        if counts[i] >= 3 {
            let is_yaochu = i == 0 || i == 8 || i == 9
                || i == 17 || i == 18 || i == 26
                || i >= 27;
            fu += if counts[i] == 4 {
                if is_yaochu { 16 } else { 8 }
            } else {
                if is_yaochu { 4 } else { 2 }
            };
        }
    }

    // 符数以 10 为单位向上取整
    fu = ((fu + 9) / 10) * 10;
    if fu < 20 {
        fu = 20;
    }

    // ── 点数表查表 ──────────────────────────────
    let base = match (fu, han) {
        (_, 5) | (_, 4) if han >= 5 => 2000,             // 满贯
        (_, 6) | (_, 7) => 3000,                          // 跳满
        (_, 8) | (_, 9) | (_, 10) => 4000,                // 倍满
        (_, 11) | (_, 12) => 6000,                        // 三倍满
        _ if han >= 13 => 8000,                            // 役满
        _ => {
            let b = fu * (1 << (2 + han)); // fu × 2^(2+han)
            if b > 2000 { 2000 } else { b }
        }
    };

    // ── 实际支付额 ──────────────────────────────
    let payment = if is_oya {
        Payment {
            ron: base * 6,         // 亲ロン = 基本点×6
            tsumo_oya: 0,           // 亲ツモ时亲无支出
            tsumo_ko: base * 2,     // 每子支付基本点×2
        }
    } else {
        Payment {
            ron: base * 4,         // 子ロン = 基本点×4
            tsumo_oya: base * 2,   // 亲ツモ时亲支付基本点×2
            tsumo_ko: base,        // 每子支付基本点×1
        }
    };

    AgariScore {
        fu,
        han,
        base_points: base,
        payment,
    }
}
