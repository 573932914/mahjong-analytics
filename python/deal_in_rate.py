"""各 round 下每种牌的放铳率。

放铳率 = 该 round 下该牌作为放铳牌的次数 / 该 round 总放铳次数。
输出：控制台摘要 + deal_in_rate.txt（可读表格）+ deal_in_rate.csv（机器可读）。

用法：
    uv run python python/deal_in_rate.py
"""

import polars as pl
from common import load_snapshots, COL

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

# ── 配置 ─────────────────────────────────────────────────────────

OUT_TXT = "python/deal_in_rate.txt"
OUT_CSV = "python/deal_in_rate.csv"


def main():
    df = load_snapshots()

    # 只取放铳行（荣和），called_tile 即放铳牌
    ron = df.filter(
        (pl.col(COL["action_type"]) == "ron") & (pl.col(COL["called_tile"]) >= 0)
    )

    # ── 按 round + 牌名统计 ───────────────────────
    stats = (
        ron.with_columns(
            pl.col(COL["called_tile"])
            .map_elements(_instance_to_name, return_dtype=pl.String)
            .alias("tile")
        )
        .group_by([COL["round"], "tile"])
        .agg(pl.len().alias("deal_in_count"))
        .sort([COL["round"], "tile"])
    )

    # ── 每 round 的放铳总数 ───────────────────────
    round_totals = (
        ron.group_by(COL["round"])
        .agg(pl.len().alias("round_total"))
        .sort(COL["round"])
    )

    # ── 合并计算比率 ──────────────────────────────
    result = (
        stats.join(round_totals, on=COL["round"])
        .with_columns(
            (pl.col("deal_in_count") * 100.0 / pl.col("round_total")).alias("rate_pct")
        )
        .sort([COL["round"], "rate_pct"], descending=[False, True])
        .collect()
    )

    # ═════════════════════════════════════════════════════════════
    # 输出 1: 控制台摘要 — 每 round 最危险的 5 张牌
    # ═════════════════════════════════════════════════════════════
    rounds = sorted(result[COL["round"]].unique().to_list())

    print("=" * 70)
    print("各 round 放铳率 Top-5 牌（放铳率 = 该牌放铳数 / 该 round 总放铳数）")
    print("=" * 70)

    for rnd in rounds:
        rd = result.filter(pl.col(COL["round"]) == rnd)
        total = rd["round_total"].item(0)
        bakaze = ["東", "南", "西", "北"][rnd // 4] if rnd // 4 < 4 else "?"
        print(f"\n── {bakaze}{rnd % 4 + 1}局 (round={rnd})  — 总放铳 {total:,} 次 ──")
        for row in rd.head(5).iter_rows():
            _, tile_name_str, cnt, total_r, rate = row
            bar = "█" * max(1, int(rate * 5))
            print(f"  {tile_name_str:>4s}  {rate:5.2f}%  {bar}  ({cnt:>8,} 次)")

    print()

    # ═════════════════════════════════════════════════════════════
    # 输出 2: 文本表格 deal_in_rate.txt
    # ═════════════════════════════════════════════════════════════
    with open(OUT_TXT, "w", encoding="utf-8") as f:
        f.write("各 round 放铳率（按牌种）\n")
        f.write("放铳率 = 该牌放铳次数 / 该 round 总放铳次数 × 100%\n")
        f.write("=" * 80 + "\n\n")

        for rnd in rounds:
            rd = result.filter(pl.col(COL["round"]) == rnd)
            total = rd["round_total"].item(0)
            bakaze = ["東", "南", "西", "北"][rnd // 4] if rnd // 4 < 4 else "?"
            f.write(f"── {bakaze}{rnd % 4 + 1}局 (round={rnd})  — 总放铳 {total:,} 次 ──\n")
            f.write(f"{'牌':>4s}  {'放铳率':>7s}  {'次数':>8s}\n")
            f.write("-" * 30 + "\n")
            for row in rd.iter_rows():
                _, tile_name_str, cnt, total_r, rate = row
                f.write(f"{tile_name_str:>4s}  {rate:6.2f}%  {cnt:>8,}\n")
            f.write("\n")

    print(f"文本表格已写入 → {OUT_TXT}")

    # ═════════════════════════════════════════════════════════════
    # 输出 3: CSV（便于 Excel / Python 再分析）
    # ═════════════════════════════════════════════════════════════
    csv_df = result.select([
        COL["round"], "tile", "deal_in_count", "round_total", "rate_pct"
    ])
    csv_df = csv_df.rename({"rate_pct": "rate_%"})
    csv_df.write_csv(OUT_CSV)
    print(f"CSV 已写入 → {OUT_CSV}")

    # ── 全局最危险的牌（全 round 加权平均） ──
    print("\n" + "=" * 70)
    print("全 round 加权平均放铳率 Top-10")
    print("=" * 70)
    global_stats = (
        result.group_by("tile")
        .agg(
            pl.col("deal_in_count").sum(),
            pl.col("round_total").first(),  # dummy, not used
        )
        .with_columns(
            (
                pl.col("deal_in_count") * 100.0 / pl.col("deal_in_count").sum()
            ).alias("rate_pct")
        )
        .sort("rate_pct", descending=True)
    )
    for row in global_stats.head(10).iter_rows():
        tile_name_str, cnt, _, rate = row
        bar = "█" * max(1, int(rate * 10))
        print(f"  {tile_name_str:>4s}  {rate:5.2f}%  {bar}  ({cnt:>8,} 次)")


if __name__ == "__main__":
    main()
