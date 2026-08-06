//! 牌的基本类型与理牌辅助。
//!
//! 基于天凤内部 tile ID (0-40) 进行花色判定与手牌排序。

/// 牌的花色（种类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suit {
    /// 万子
    Manzu,
    /// 筒子
    Pinzu,
    /// 索子
    Souzu,
    /// 字牌（风牌 + 三元牌）
    Jihai,
}

/// 牌的排序键（理牌用）。
///
/// 排序优先级：万子 → 筒子 → 索子 → 字牌，
/// 同花色内数字升序，赤牌排在同数字普通牌之后。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TileKey {
    /// 花色排序优先级（0=万, 1=筒, 2=索, 3=字）。
    pub suit_order: u8,
    /// 数字（1-9，字牌为 1-7：东南西北白发中）。
    pub number: u8,
    /// 是否为赤牌（true=赤，排在普通牌之后）。
    pub is_red: bool,
}

impl TileKey {
    /// 天凤 tile ID → 排序键。
    ///
    /// # 示例
    /// ```ignore
    /// TileKey::from_id(0)  → TileKey { suit:0, number:1, red:false }  // 1m
    /// TileKey::from_id(34) → TileKey { suit:0, number:5, red:true  }  // 赤5m
    /// ```
    pub fn from_id(id: i32) -> Self {
        match id {
            // 万子 0-8 + 赤5m=34
            0..=8 => Self {
                suit_order: 0,
                number: (id as u8) + 1,
                is_red: false,
            },
            34 => Self {
                suit_order: 0,
                number: 5,
                is_red: true,
            },

            // 筒子 9-17 + 赤5p=37
            9..=17 => Self {
                suit_order: 1,
                number: (id as u8) - 8,
                is_red: false,
            },
            37 => Self {
                suit_order: 1,
                number: 5,
                is_red: true,
            },

            // 索子 18-26 + 赤5s=40
            18..=26 => Self {
                suit_order: 2,
                number: (id as u8) - 17,
                is_red: false,
            },
            40 => Self {
                suit_order: 2,
                number: 5,
                is_red: true,
            },

            // 字牌 27-33：东南西北白发中
            27..=33 => Self {
                suit_order: 3,
                number: (id as u8) - 26,
                is_red: false,
            },

            _ => Self {
                suit_order: 4,
                number: 0,
                is_red: false,
            },
        }
    }

    /// 天凤 tile ID → 花色。
    pub fn suit(id: i32) -> Suit {
        match id {
            0..=8 | 34 => Suit::Manzu,
            9..=17 | 37 => Suit::Pinzu,
            18..=26 | 40 => Suit::Souzu,
            _ => Suit::Jihai,
        }
    }
}

/// 对手牌进行理牌（排序）。
///
/// 排序规则：万→筒→索→字，同花色数字升序，
/// 赤牌排在同数字普通牌之后。
///
/// # 参数
/// - `hand`: 待排序的手牌 tile ID 列表
///
/// # 返回
/// - 排序后的新 `Vec<i32>`
pub fn sort_hand(hand: &[i32]) -> Vec<i32> {
    let mut sorted: Vec<i32> = hand.to_vec();
    sorted.sort_by_key(|&id| TileKey::from_id(id));
    sorted
}

/// 摸牌后用于手牌展示：理牌并将摸牌分离到末尾。
///
/// # 参数
/// - `hand`: 手牌 tile ID 列表（含摸牌，共 14 枚）
/// - `drawn`: 刚摸到的牌的 tile ID
///
/// # 返回
/// - `(理牌后的13枚, 摸牌)` — 可视化中理牌手牌在左，
///   摸牌在右端留一定间距单独绘制。
pub fn sort_hand_with_draw(hand: &[i32], drawn: i32) -> (Vec<i32>, i32) {
    let mut without_draw: Vec<i32> = hand
        .iter()
        .copied()
        .filter(|&t| t != drawn)
        .collect();
    without_draw.sort_by_key(|&id| TileKey::from_id(id));
    (without_draw, drawn)
}
