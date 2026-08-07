"""副露类型分布：吃/碰/明槓/暗槓/加槓 按場風统计。

用法：
    uv run python python/meld_stats.py
"""

import json

import polars as pl
from common import load_snapshots, BAKAZE_NAMES, COL


def main():
    df = load_snapshots()
    df = df.with_columns(bakaze=pl.col(COL["round"]) // 4)

    # 只取副露 class 的行（chi/pon/daiminkan/ankan/kakan）
    meld_actions = df.filter(
        pl.col(COL["action_type"]).is_in(
            ["chi", "pon", "daiminkan", "ankan", "kakan"]
        )
    )

    print("=" * 60)
    print("副露類型別 統計（按場風）")
    print("=" * 60)

    stats = (
        meld_actions.group_by(["bakaze", COL["action_type"]])
        .agg(pl.len().alias("count"))
        .sort(["bakaze", COL["action_type"]])
        .collect()
    )

    # 宽表展示
    for bz in sorted(stats["bakaze"].unique().to_list()):
        name = BAKAZE_NAMES.get(bz, f"場{bz}")
        row = stats.filter(pl.col("bakaze") == bz)
        print(f"\n{name}場:")
        for r in row.iter_rows():
            _, at, cnt = r
            print(f"  {at:>10s}: {cnt:>10,}")

    # ── actor 与 meld 关系：谁最爱副露？ ──
    print("\n" + "=" * 60)
    print("各玩家副露率（actor 分布）")
    print("=" * 60)

    actor_stats = (
        meld_actions.group_by("actor")
        .agg(pl.len().alias("count"))
        .sort("actor")
        .collect()
    )

    total_melds = actor_stats["count"].sum()
    for row in actor_stats.iter_rows():
        actor, cnt = row
        pct = cnt / total_melds * 100
        wind = ["自東", "自南", "自西", "自北"][actor]
        print(f"  P{actor} ({wind}): {cnt:>10,} ({pct:.1f}%)")

    print()


if __name__ == "__main__":
    main()
