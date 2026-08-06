//! 快照类型与游戏状态追踪
//!
//! 定义牌谱解析过程中累积的游戏状态 [`GameState`]，以及在每个决策点
//! 产出的完整局面快照 [`Snapshot`]。每一条快照 = 某个玩家的一次
//! 决策（切牌 / 鸣牌 / 立直 / 杠 / 和了），携带全知视角下的手牌、
//! 牌河、副露、场上状态和结果标签。
//!
//! # 设计原则
//! - 快照级数据集：每行 = 一个决策时刻，适合列式聚合分析
//! - 牌河为裸 tile ID 列表，被盗牌留在原位置（通过副露追踪鸣牌关系）
//! - 和了快照独立记录（含番种/符/点数），支持多人和牌各自一行
//! - 流局不产生额外快照（最后一手切牌即为该局终态）
//! - tenpai 仅最后一手快照记录
//! - turns 为四人各自的打牌计数（庄家第一打 = 1 巡）

// ── 副露条目 ──────────────────────────────────────────────────

/// 一个已亮明的副露（吃/碰/杠）及其关联信息。
#[derive(Debug, Clone, Default)]
pub struct MeldEntry {
    /// 副露类型：`"chi"` | `"pon"` | `"daiminkan"` | `"shominkan"` |
    /// `"ankan"` | `"kakan"`。
    pub meld_type: String,
    /// 副露中包含的所有牌的 tenhou 实例 ID（已排序）。
    pub tiles: Vec<i32>,
    /// 被叫走的那张牌的 tile ID。暗杠 / 加杠时为 0。
    pub called_tile: i32,
    /// 被叫牌的来源玩家（-1 = 自家/暗杠/加杠，0-3 = 从他家河里叫来）。
    pub from_player: i8,
    /// 副露成立时，副露家已经打过的牌数（即 turns[who] 的当前值）。
    /// 例如玩家第 3 次打牌前吃了上家的牌，则 discard_n = 2。
    pub discard_n: i32,
    /// 被叫牌在副露展示中的位置（0=左, 1=中, 2=右, -1=无被叫牌）。
    pub called_pos: i8,
}

// ── 快照 ──────────────────────────────────────────────────────

/// 一粒决策快照——分析表的核心记录。
///
/// 每行 = 某个玩家的一次决策时刻（切牌 / 鸣牌 / 立直 / 杠 / 和了），
/// 携带此刻的全知视角局面。
#[derive(Debug, Clone)]
pub struct Snapshot {
    // ── 定位信息 ────────────────────────────────
    /// 牌谱唯一 ID（天凤日志 ID），用于溯源但对统计无直接意义。
    pub game_id: String,
    /// 全局快照序号，跨局单调递增（0, 1, 2, ...）。仅作联合主键，
    /// 分析时请用 `turns[actor]` 表示操作者巡目。
    pub seq: i32,
    /// 本場数（连庄次数）。
    pub honba: i32,
    /// 当前庄家（0-3）。
    pub oya: i8,
    /// 四家各自打过的牌数（庄家第一打 = 1）。
    /// `turns[actor]` 即为当前行动者的巡目。
    pub turns: [i32; 4],
    /// 本次决策者（0-3）。
    pub actor: i8,

    // ── 全知手牌（仅决策前）────────────────────
    /// 行动**前** 4 人手牌，各自已排序。
    pub hand_before: [Vec<i32>; 4],

    // ── 牌河（纯 tile 列表，被盗牌留在原位置）───
    /// 各玩家牌河（按弃牌顺序，不含被盗标记）。
    pub river_p0: Vec<i32>,
    pub river_p1: Vec<i32>,
    pub river_p2: Vec<i32>,
    pub river_p3: Vec<i32>,
    /// 牌河中每张牌是否为摸切（与 river 一一对应）。
    pub river_tsumo_p0: Vec<bool>,
    pub river_tsumo_p1: Vec<bool>,
    pub river_tsumo_p2: Vec<bool>,
    pub river_tsumo_p3: Vec<bool>,

    // ── 副露 ────────────────────────────────────
    /// 各玩家已亮副露列表。
    pub melds_p0: Vec<MeldEntry>,
    pub melds_p1: Vec<MeldEntry>,
    pub melds_p2: Vec<MeldEntry>,
    pub melds_p3: Vec<MeldEntry>,

    // ── 场上状态 ────────────────────────────────
    /// 4 人当前点数（×100）。
    pub scores: [i32; 4],
    /// 4 人是否已立直。
    pub riichi: [bool; 4],
    /// 4 人立直时的全局巡目（-1 = 未立直）。方便快速检索立直时机。
    pub riichi_turn: [i32; 4],
    /// 4 人立直宣言牌（-1 = 未立直）。
    pub riichi_tile: [i32; 4],
    /// 当前宝牌指示牌列表（每杠一次多一枚）。
    pub dora_indicators: Vec<i32>,
    /// 牌山剩余可摸牌数（不含王牌 14 枚的通常山）。
    /// 初始 70（136-14-52），每摸一张或杠一次减 1。
    pub wall_remaining: i32,
    /// 当前供托立直棒数（未结算，归属下一个和了者）。
    pub riichi_sticks: i32,

    // ── 决策内容 ────────────────────────────────
    /// 行动类型：`"tsumogiri"` | `"discard"` | `"reach"` |
    /// `"chi"` | `"pon"` | `"daiminkan"` | `"shominkan"` |
    /// `"ankan"` | `"kakan"` | `"tsumo"` | `"ron"`。
    pub action_type: String,
    /// 刚摸到的牌。鸣牌时为被叫的牌（chi/pon/daiminkan/shominkan）。
    /// 和了（tsumo）时为和了牌。-1 = 无。
    pub drawn_tile: i32,
    /// 被叫的牌（chi/pon/daiminkan/shominkan/ron）。
    /// -1 = 无（自家摸打/暗杠/加杠/自摸和了）。
    pub called_tile: i32,
    /// 切出的牌。tsumogiri 时与 drawn_tile 相同。
    /// 杠时 = 杠的牌种代表 tile。和了时 = -1。
    /// -1 = 无切牌（大明杠/加杠/和了等）。
    pub discard_tile: i32,
    /// 是否为摸切（摸到的牌直接切出，drawn == discard）。true=摸切, false=手出。
    pub is_tsumogiri: bool,

    // ── 局末信息（每行有效，冗余到整局所有快照）─
    /// 终局类型：-1=进行中, 0=流局, 1=自摸和了, 2=荣和。
    pub round_end_kind: i8,
    /// 和了者（P0-3）。-1=流局/进行中。
    pub round_winner: i8,
    /// 局末点数移动 vs 本局起始（正=获得，负=失去，×100，含供託/本場）。
    /// 进行中=[0,0,0,0]。
    pub round_point_delta: [i32; 4],
    /// 局末听牌人数（-1=进行中/和了, 0-4=仅牌山枯竭流局时有效）。
    pub round_tenpai_count: i8,

    // ── 和了结果（仅 action_type = tsumo / ron 时有效）──
    /// 和了役种 ID 列表（天凤编码）。
    pub agari_han_ids: Vec<i32>,
    /// 总飜数。
    pub agari_han: i32,
    /// 符数。
    pub agari_fu: i32,
    /// 手牌飜数点数（不含供託/本場加成）。供託/本場另见 round_point_delta。
    pub agari_points: i32,
    /// 放铳者（ron 时）。-1 = 自摸。
    pub agari_from: i8,
    /// 里宝牌指示牌列表（仅立直和了时有效，其余为空）。
    pub agari_ura_dora: Vec<i32>,
}

// ── 游戏状态（parser 内部使用）────────────────────────────────

/// 解析一局牌谱过程中累积的完整游戏状态。
///
/// 每遇到 `<INIT>` 时调用 `reset_round` 重置局级字段，
/// 牌局级字段（如玩家名、段位）则保留。
#[derive(Debug, Clone)]
pub(crate) struct GameState {
    // ── 牌局级（整局不变）─────────────────────────
    pub game_id: String,
    pub game_type: i32,
    pub lobby: i32,
    pub names: [String; 4],
    pub dans: [i32; 4],
    pub rates: [f64; 4],
    pub sexes: [String; 4],
    pub num_players: i32,

    // ── 局级（每局重置）─────────────────────────
    pub honba: i32,
    pub riichi_sticks: i32,
    pub scores: [i32; 4],
    /// 本局起始点数（INIT ten），用于计算 round_point_delta。
    pub round_start_scores: [i32; 4],
    pub oya: i8,
    pub dora_indicators: Vec<i32>,

    // ── 每玩家状态 ─────────────────────────────
    pub hands: [Vec<i32>; 4],
    /// 牌河：仅 tile ID，按弃牌顺序。
    pub rivers: [Vec<i32>; 4],
    /// 牌河中每张牌的摸切标记。
    pub river_tsumo: [Vec<bool>; 4],
    /// 牌河中每张牌弃出时的全局 seq（仅解析时用，不入快照）。
    pub river_seq: [Vec<i32>; 4],
    pub melds: [Vec<MeldEntry>; 4],
    pub riichi: [bool; 4],
    /// 各玩家立直时的全局 seq（-1 = 未立直）。
    pub riichi_seq: [i32; 4],
    /// 各玩家立直宣言牌（-1 = 未立直）。
    pub riichi_tile: [i32; 4],

    // ── 当前动作 ───────────────────────────────
    /// 全局快照序号（跨局单调递增）。
    pub seq: i32,
    /// 四家各自打过的牌数。turn[oya] 初始 0，庄家首次弃牌后变 1。
    pub turns: [i32; 4],
    /// 当前行动者（刚摸牌或刚鸣牌的玩家，0-3）。
    pub actor: i8,
    /// 最近一次摸到的牌（-1 = 无）。
    pub last_draw: i32,
    /// 下一次弃牌为立直宣言牌的玩家（-1 = 无待处理的立直声明）。
    pub pending_reach_player: i8,

    // ── 牌山 ──────────────────────────────────
    /// 通常山剩余可摸牌数（初始 70 = 136-14-52）。
    pub wall_remaining: i32,
    /// 下一次 T/U/V/W 摸牌是否来自王牌（杠後补充）。
    pub next_draw_from_dead_wall: bool,

    // ── 快照累积 ───────────────────────────────
    pub snapshots: Vec<Snapshot>,
    /// 当前局首张快照在 snapshots 中的索引（用于局末回填整局）。
    pub round_start_idx: usize,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            game_id: String::new(),
            game_type: 0,
            lobby: 0,
            names: Default::default(),
            dans: [0; 4],
            rates: [1500.0; 4],
            sexes: Default::default(),
            num_players: 4,
            honba: 0,
            riichi_sticks: 0,
            scores: [250; 4],
            round_start_scores: [250; 4],
            oya: 0,
            dora_indicators: vec![],
            hands: [vec![], vec![], vec![], vec![]],
            rivers: [vec![], vec![], vec![], vec![]],
            river_tsumo: [vec![], vec![], vec![], vec![]],
            river_seq: [vec![], vec![], vec![], vec![]],
            melds: [vec![], vec![], vec![], vec![]],
            riichi: [false; 4],
            riichi_seq: [-1; 4],
            riichi_tile: [-1; 4],
            seq: 0,
            turns: [0; 4],
            actor: 0,
            last_draw: -1,
            pending_reach_player: -1,
            wall_remaining: 70,
            next_draw_from_dead_wall: false,
            snapshots: vec![],
            round_start_idx: 0,
        }
    }
}

// ── Snapshot 便捷访问 ──────────────────────────────────────────

impl Snapshot {
    /// 获取指定玩家的摸切标记引用（与牌河一一对应）。
    pub fn river_tsumo_for(&self, idx: usize) -> &Vec<bool> {
        match idx {
            0 => &self.river_tsumo_p0,
            1 => &self.river_tsumo_p1,
            2 => &self.river_tsumo_p2,
            _ => &self.river_tsumo_p3,
        }
    }

    /// 获取指定玩家的牌河引用。
    pub fn river_for(&self, idx: usize) -> &Vec<i32> {
        match idx {
            0 => &self.river_p0,
            1 => &self.river_p1,
            2 => &self.river_p2,
            _ => &self.river_p3,
        }
    }

    /// 获取指定玩家的副露引用。
    pub fn melds_for(&self, idx: usize) -> &Vec<MeldEntry> {
        match idx {
            0 => &self.melds_p0,
            1 => &self.melds_p1,
            2 => &self.melds_p2,
            _ => &self.melds_p3,
        }
    }
}
