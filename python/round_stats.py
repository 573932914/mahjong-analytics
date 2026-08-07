"""各場風的基本统计：行数、平均巡目、和了率。

用法：
    uv run python python/round_stats.py
"""

import polars as pl
from common import load_snapshots, BAKAZE_NAMES, COL


def main():
    df = load_snapshots()

    # 添加衍生列
    df = df.with_columns(bakaze=pl.col(COL["round"]) // 4)

    print("=" * 60)
    print("場風別 基本統計")
    print("=" * 60)

    stats = (
        df.group_by("bakaze")
        .agg(
            pl.len().alias("snapshots"),
            # 各玩家的平均巡目（action_type in discard/tsumogiri/reach）
            pl.col("turns")
            .list.get(pl.col("actor"))
            .filter(
                pl.col(COL["action_type"]).is_in(["discard", "tsumogiri", "reach"])
            )
            .mean()
            .alias("avg_turn"),
            # 和了 snapshots
            pl.col(COL["action_type"])
            .is_in(["tsumo", "ron"])
            .sum()
            .alias("agari_count"),
            # 流局率
            (pl.col(COL["round_end_kind"]) == 0)
            .sum()
            .alias("ryuukyoku_count"),
        )
        .sort("bakaze")
        .collect()
    )

    for row in stats.iter_rows():
        bz, snaps, avg_turn, agari, ryu = row
        name = BAKAZE_NAMES.get(bz, f"場{bz}")
        total = snaps or 1  # avoid div0
        agari_pct = agari / total * 100
        ryu_pct = ryu / total * 100
        print(
            f"\n{name}場 ({bz}): {snaps:>12,} snapshots"
            f"\n  平均巡目:   {avg_turn:.1f}"
            f"\n  和了 snapshot 占比: {agari_pct:.1f}%"
            f"\n  流局标记行占比:     {ryu_pct:.1f}%"
        )

    print()


if __name__ == "__main__":
    main()
