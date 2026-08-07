"""各 round 下每种牌的放铳率。

放铳率 = 该 round 下该牌作为放铳牌的次数 / 该 round 总放铳次数。
输出：控制台摘要 + deal_in_rate.txt（可读表格）+ deal_in_rate.csv（机器可读）。

用法：
    uv run python python/deal_in_rate.py
"""

import polars as pl
from common import load_snapshots, Timer, AnalysisResult, COL

# ── 牌名映射：instance_id → 牌名（赤牌归并到对应 5） ─────────────

_TILE_NAMES = [
    "1m","2m","3m","4m","5m","6m","7m","8m","9m",
    "1p","2p","3p","4p","5p","6p","7p","8p","9p",
    "1s","2s","3s","4s","5s","6s","7s","8s","9s",
    "東","南","西","北","白","發","中",
]


def _instance_to_name(instance_id: int) -> str:
    """天凤实例 ID → 牌名，赤5归并到普通5。"""
    if instance_id < 0:
        return "?"
    if instance_id == 34:      # 赤5m
        return "5m"
    if instance_id == 37:      # 赤5p
        return "5p"
    if instance_id == 40:      # 赤5s
        return "5s"
    tid = instance_id // 4
    return _TILE_NAMES[tid] if tid < len(_TILE_NAMES) else f"?{instance_id}"


def main():
    result = AnalysisResult("deal_in_rate", ["txt", "csv"])
    result.start()

    # ── 加载 + 筛选 ─────────────────────────────────
    with result.time("加载+筛选") as t:
        df = load_snapshots()
        ron = df.filter(
            (pl.col(COL["action_type"]) == "ron")
            & (pl.col(COL["called_tile"]) >= 0)
        )

    # ── 聚合 ────────────────────────────────────────
    with result.time("聚合+收集") as t:
        stats = (
            ron.with_columns(
                pl.col(COL["called_tile"])
                .map_elements(_instance_to_name, return_dtype=pl.String)
                .alias("tile")
            )
            .group_by([COL["round"], "tile"])
            .agg(pl.len().alias("deal_in_count"))
        )

        round_totals = (
            ron.group_by(COL["round"])
            .agg(pl.len().alias("round_total"))
        )

        full = (
            stats.join(round_totals, on=COL["round"])
            .with_columns(
                (pl.col("deal_in_count") * 100.0 / pl.col("round_total"))
                .alias("rate_pct")
            )
            .sort([COL["round"], "rate_pct"], descending=[False, True])
            .collect()
        )

        total_ron = round_totals.select(pl.col("round_total").sum()).collect().item()

    # ── 控制台摘要 ──────────────────────────────────
    rounds = sorted(full[COL["round"]].unique().to_list())
    print("\n各 round 放铳率 Top-5 牌")
    print("=" * 70)

    for rnd in rounds:
        rd = full.filter(pl.col(COL["round"]) == rnd)
        total_r = rd["round_total"].item(0)
        bakaze = ["東","南","西","北"][rnd // 4] if rnd // 4 < 4 else "?"
        print(f"\n── {bakaze}{rnd % 4 + 1}局 (round={rnd})  — 总放铳 {total_r:,} 次 ──")
        for row in rd.head(5).iter_rows():
            _, tile_name_str, cnt, _, rate = row
            bar = "█" * max(1, int(rate * 5))
            print(f"  {tile_name_str:>4s}  {rate:5.2f}%  {bar}  ({cnt:>8,} 次)")

    # ── 输出文件 ────────────────────────────────────
    result.write_txt(
        "各 round 放铳率（按牌种）\n"
        "放铳率 = 该牌放铳次数 / 该 round 总放铳次数 × 100%",
        full.select([COL["round"], "tile", "deal_in_count", "round_total", "rate_pct"])
    )
    result.write_csv(full.select(
        [COL["round"], "tile", "deal_in_count", "round_total",
         pl.col("rate_pct").alias("rate_%")]
    ))

    # ── 全局 Top-10 ─────────────────────────────────
    print("\n" + "=" * 70)
    print("全 round 加权平均放铳率 Top-10")
    print("=" * 70)
    global_all = full["deal_in_count"].sum()
    global_stats = (
        full.group_by("tile")
        .agg(pl.col("deal_in_count").sum())
        .with_columns((pl.col("deal_in_count") * 100.0 / global_all).alias("rate_pct"))
        .sort("rate_pct", descending=True)
    )
    for row in global_stats.head(10).iter_rows():
        nm, cnt, rate = row
        bar = "█" * max(1, int(rate * 10))
        print(f"  {nm:>4s}  {rate:5.2f}%  {bar}  ({cnt:>8,} 次)")

    result.finish({
        "ron_events": total_ron,
        "result_rows": full.height,
        "unique_tiles": full["tile"].n_unique(),
    })


if __name__ == "__main__":
    main()
