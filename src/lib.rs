//! `mahjong-analytics` — 麻雀牌谱分析与可视化工具
//!
//! 将天凤 mjlog XML 牌谱转换为 Polars DataFrame，存储为 Parquet。
//! 提供 CLI（`mahjong-analytics`）和 GUI 可视化（`mahjong-viz`）。
//!
//! # 模块结构
//! - [`convert`] — XML → DataFrame 主转换流水线
//! - [`parser`] — mjlog XML 解析器 → [`Snapshot`] 序列
//! - [`snapshot`] — 核心数据类型（[`Snapshot`], [`GameState`] 等）
//! - [`cli`] — CLI 参数定义（clap derive）
//! - [`viz`] — GUI 可视化（egui/eframe 牌桌渲染）

pub mod cli;
pub mod convert;
pub mod mahjong;
pub mod parser;
pub mod snapshot;
pub mod viz;

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use polars::prelude::*;
use rayon::prelude::*;

// ── 公共 API ──────────────────────────────────────────────────

/// 转换单个 mjlog XML 文件 → 输出 `snapshots.parquet`。
///
/// # 参数
/// - `input`: mjlog XML 文件路径（`.xml` 或 `.xml.gz`）
/// - `output`: 输出 `.parquet` 文件路径
///
/// # 返回
/// - `Ok(())` 并打印写出信息
/// - `Err` 解析或 I/O 错误
pub fn convert_one(
    input: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    let mut df = convert::convert_xml_file(input)
        .with_context(|| format!("failed to convert {}", input.display()))?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(output)?;
    ParquetWriter::new(&mut f).finish(&mut df)?;
    eprintln!(
        "Wrote {} rows × {} cols → {}",
        df.height(),
        df.width(),
        output.display()
    );
    Ok(())
}

/// 批量转换：从 houou-logs SQLite 数据库读取所有已处理的日志，
/// 转为 Parquet 文件。
///
/// # 参数
/// - `db_path`: houou-logs 的 SQLite 数据库路径
/// - `output_dir`: 输出根目录，每局一个子目录（以 log ID 命名）
///
/// # 流程
/// 1. 连接 SQLite
/// 2. 查询 `is_processed=1 AND was_error=0` 的日志
/// 3. 逐条 gzip 解压 → XML 解析 → 快照转换 → Parquet 写出
pub fn convert_batch(
    db_path: &Path,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("failed to open db {}", db_path.display()))?;

    let mut stmt = conn.prepare(
        "SELECT id, log FROM logs \
         WHERE is_processed = 1 AND was_error = 0 \
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let compressed: Vec<u8> = row.get(1)?;
        Ok((id, compressed))
    })?;

    let mut count = 0usize;
    for row in rows {
        let (id, compressed) = row?;

        let xml = {
            let mut decoder =
                flate2::read::GzDecoder::new(&compressed[..]);
            let mut s = String::new();
            std::io::Read::read_to_string(&mut decoder, &mut s)
                .with_context(|| {
                    format!("failed to decompress log {id}")
                })?;
            s
        };

        let mut df = convert::convert_xml(&xml, &id)
            .with_context(|| format!("failed to parse log {id}"))?;

        let log_dir = output_dir.join(&id);
        std::fs::create_dir_all(&log_dir)?;
        let out_path = log_dir.join("snapshots.parquet");
        let mut f = std::fs::File::create(&out_path)?;
        ParquetWriter::new(&mut f).finish(&mut df)?;

        count += 1;
    }

    eprintln!(
        "Batch-converted {count} log(s) to {}",
        output_dir.display()
    );
    Ok(())
}

/// 并发批量转换：流式读取 SQLite + 分块并行解析 + 合并为单个 Parquet。
pub fn convert_batch_parallel(
    db_path: &Path,
    output_dir: &Path,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("failed to open db {}", db_path.display()))?;

    eprintln!("流式读取并并发转换...");
    let mut stmt = conn.prepare(
        "SELECT id, log FROM logs WHERE is_processed = 1 AND was_error = 0",
    )?;
    let row_iter = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;

    let count = AtomicUsize::new(0);
    let start = std::time::Instant::now();
    let chunk_size = 500usize;

    // 流式读取行 → 分块 → 并行处理
    let mut chunk: Vec<(String, Vec<u8>)> = Vec::with_capacity(chunk_size);
    for row in row_iter {
        let (id, compressed) = row?;
        chunk.push((id, compressed));
        if chunk.len() >= chunk_size {
            process_chunk(&chunk, output_dir, &count, &start)?;
            chunk.clear();
        }
    }
    if !chunk.is_empty() {
        process_chunk(&chunk, output_dir, &count, &start)?;
    }

    let total = count.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64();
    eprintln!(
        "解析完成 {total} 条日志 ({:.1}s, {:.0} logs/s)",
        elapsed, total as f64 / elapsed
    );

    // 先删除旧合并文件
    let merged_path = output_dir.join("snapshots_all.parquet");
    if merged_path.exists() {
        std::fs::remove_file(&merged_path)?;
        eprintln!("已删除旧合并文件");
    }

    // 合并为单个文件 (使用 glob — 大量文件时建议用 Python 合并)
    eprintln!("合并为单个 parquet (流式)...");
    let glob = format!("{}/*.parquet", output_dir.display());
    match LazyFrame::scan_parquet(&glob, Default::default()) {
        Ok(lf) => {
            let _ = lf.sink_parquet(
                SinkTarget::Path(std::sync::Arc::new(merged_path.clone())),
                ParquetWriteOptions::default(),
                None,
                SinkOptions::default(),
            )?;
            eprintln!("合并完成 → {}", merged_path.display());
        }
        Err(e) => {
            eprintln!("合并失败: {e}");
            eprintln!("请用 Python 手动合并:");
            eprintln!("  uv run python -c \"import polars as pl; pl.scan_parquet('{}/*.parquet').sink_parquet('{}')\"",
                output_dir.display(), merged_path.display());
        }
    }

    Ok(())
}

/// 并行处理一个 chunk
fn process_chunk(
    chunk: &[(String, Vec<u8>)],
    output_dir: &Path,
    count: &AtomicUsize,
    start: &std::time::Instant,
) -> anyhow::Result<()> {
    chunk.par_iter().try_for_each(|(id, compressed)| {
        let xml = {
            let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
            let mut s = String::new();
            std::io::Read::read_to_string(&mut decoder, &mut s)?;
            s
        };
        let mut df = convert::convert_xml(&xml, id)?;
        let out_path = output_dir.join(format!("{id}.parquet"));
        let mut f = std::fs::File::create(&out_path)?;
        ParquetWriter::new(&mut f).finish(&mut df)?;
        Ok::<_, anyhow::Error>(())
    })?;

    let n = count.fetch_add(chunk.len(), Ordering::Relaxed) + chunk.len();
    let elapsed = start.elapsed().as_secs_f64();
    let rate = n as f64 / elapsed;
    eprintln!("  {n} logs  {rate:.0} logs/s",);
    Ok(())
}
