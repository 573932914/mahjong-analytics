//! 牌的基本类型。

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

