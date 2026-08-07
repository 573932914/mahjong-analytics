"""公共工具：数据加载、牌名映射、常量字段。

用法：
    from common import load_snapshots, tile_name, BAKAZE_NAMES
"""

import polars as pl

# ── 数据路径 ──────────────────────────────────────────────────────

DATA_DIR = "E:/tables"
MERGED = f"{DATA_DIR}/snapshots_all.parquet"

# ── 場風 / 自風 ──────────────────────────────────────────────────

BAKAZE_NAMES = {0: "東", 1: "南", 2: "西", 3: "北"}
JIKAZE_NAMES = {0: "東", 1: "南", 2: "西", 3: "北"}


def load_snapshots(glob: str | None = None) -> pl.LazyFrame:
    """加载快照数据。默认读合并文件，传 glob 可读散文件。

    >>> df = load_snapshots()
    >>> df = load_snapshots("E:/tables/202101*.parquet")
    """
    path = glob or MERGED
    return pl.scan_parquet(path)


def load_sample(n: int = 100_000) -> pl.DataFrame:
    """加载 n 行样本（用于快速试验）。"""
    return pl.scan_parquet(MERGED).head(n).collect()


# ── 牌名映射 ─────────────────────────────────────────────────────

_TILE_NAMES = [
    "1m", "2m", "3m", "4m", "5m", "6m", "7m", "8m", "9m",
    "1p", "2p", "3p", "4p", "5p", "6p", "7p", "8p", "9p",
    "1s", "2s", "3s", "4s", "5s", "6s", "7s", "8s", "9s",
    "東", "南", "西", "北", "白", "發", "中",
]


def instance_to_type(instance_id: int) -> int:
    """天凤实例 ID → 牌种 ID (0-33)。"""
    return instance_id // 4


def tile_name(instance_id: int) -> str:
    """天凤实例 ID → 牌名（如 '5m', '東', '赤5m'）。

    >>> tile_name(16)    # 4*4=16 = 5m copy 0
    '5m'
    >>> tile_name(34)    # 赤5m
    '赤5m'
    """
    if instance_id == 34:
        return "赤5m"
    if instance_id == 37:
        return "赤5p"
    if instance_id == 40:
        return "赤5s"
    tid = instance_to_type(instance_id)
    return _TILE_NAMES[tid] if tid < len(_TILE_NAMES) else f"?{instance_id}"


def tile_kind(instance_id: int) -> int:
    """天凤实例 ID → 牌种 ID（赤牌映射到对应 5）。

    16 → 4 (5m), 34 → 4 (赤5m → 5m)
    """
    if instance_id == 34:
        return 4  # 赤5m → 5m
    if instance_id == 37:
        return 13  # 赤5p → 5p
    if instance_id == 40:
        return 22  # 赤5s → 5s
    return instance_id // 4


# ── 役种名称 ─────────────────────────────────────────────────────

YAKU_NAMES: dict[int, str] = {
    0: "門前清自摸和", 1: "立直", 2: "一発", 3: "槍槓",
    4: "嶺上開花", 5: "海底摸月", 6: "河底撈魚",
    7: "平和", 8: "断幺九", 9: "一盃口",
    10: "自風 東", 11: "自風 南", 12: "自風 西", 13: "自風 北",
    14: "場風 東", 15: "場風 南", 16: "場風 西", 17: "場風 北",
    18: "役牌 白", 19: "役牌 發", 20: "役牌 中",
    21: "混全帯么九", 22: "一気通貫", 23: "三色同順",
    24: "三色同刻", 25: "三槓子", 26: "対々和",
    27: "七対子", 28: "混老頭", 29: "小三元",
    30: "混一色", 31: "純全帯么九", 32: "二盃口",
    33: "清一色", 34: "一変",
    35: "純チャン", 36: "人和",
    37: "一変抜き", 38: "三連刻", 39: "四連刻",
    40: "数え役満？",
    51: "ドラ", 52: "裏ドラ", 53: "赤ドラ",
    54: "抜きドラ",
}


def yaku_name(han_id: int) -> str:
    """役种 ID → 名称。"""
    return YAKU_NAMES.get(han_id, f"unknown({han_id})")


# ── 列引用快捷方式 ──────────────────────────────────────────────

COL = {
    "game_id": "game_id",
    "seq": "seq",
    "round": "round",
    "honba": "honba",
    "oya": "oya",
    "turns": "turns",
    "actor": "actor",
    "action_type": "action_type",
    "drawn_tile": "drawn_tile",
    "called_tile": "called_tile",
    "discard_tile": "discard_tile",
    "is_tsumogiri": "is_tsumogiri",
    "scores": "scores",
    "riichi": "riichi",
    "riichi_turn": "riichi_turn",
    "riichi_tile": "riichi_tile",
    "dora_indicators": "dora_indicators",
    "wall_remaining": "wall_remaining",
    "riichi_sticks": "riichi_sticks",
    "agari_han": "agari_han",
    "agari_fu": "agari_fu",
    "agari_points": "agari_points",
    "agari_from": "agari_from",
    "agari_han_ids": "agari_han_ids",
    "agari_ura_dora": "agari_ura_dora",
    "round_end_kind": "round_end_kind",
    "round_winner": "round_winner",
    "round_point_delta": "round_point_delta",
    "round_tenpai_count": "round_tenpai_count",
    # hand
    "hand_p0": "hand_before_p0",
    "hand_p1": "hand_before_p1",
    "hand_p2": "hand_before_p2",
    "hand_p3": "hand_before_p3",
    # river
    "river_p0": "river_p0",
    "river_p1": "river_p1",
    "river_p2": "river_p2",
    "river_p3": "river_p3",
    "river_tsumo_p0": "river_tsumo_p0",
    "river_tsumo_p1": "river_tsumo_p1",
    "river_tsumo_p2": "river_tsumo_p2",
    "river_tsumo_p3": "river_tsumo_p3",
    # melds
    "melds_p0": "melds_p0_json",
    "melds_p1": "melds_p1_json",
    "melds_p2": "melds_p2_json",
    "melds_p3": "melds_p3_json",
}


def hand_col(p: int) -> str:
    """玩家 p (0-3) 的手牌列名。"""
    return f"hand_before_p{p}"


def river_col(p: int) -> str:
    """玩家 p (0-3) 的牌河列名。"""
    return f"river_p{p}"


def meld_col(p: int) -> str:
    """玩家 p (0-3) 的副露列名。"""
    return f"melds_p{p}_json"
