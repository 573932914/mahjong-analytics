//! `mahjong-analytics` CLI 入口。
//!
//! 子命令分发到 `cli` 模块中定义的各个 handler。
//!
//! # 可用子命令
//! - `convert <XML_FILE> -o <OUTPUT.parquet>` — 单个 XML 转 Parquet
//! - `batch <DB_PATH> -o <DIR>` — 从 SQLite 批量转换

mod cli;

use anyhow::Context;
use clap::Parser;
use cli::Command;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    match cli.command {
        // ── convert ──────────────────────────────
        Command::Convert(args) => {
            let output = args.output.unwrap_or_else(|| {
                let stem = args
                    .input
                    .file_stem()
                    .unwrap_or_default();
                std::path::Path::new(stem)
                    .with_extension("parquet")
            });

            mahjong_analytics::convert_one(&args.input, &output)
                .with_context(|| "convert failed")?;
        }

        // ── batch ────────────────────────────────
        Command::Batch(args) => {
            mahjong_analytics::convert_batch(
                &args.db_path,
                &args.output_dir,
            )
            .with_context(|| "batch convert failed")?;
        }

        // ── par-batch ─────────────────────────────
        Command::ParBatch(args) => {
            mahjong_analytics::convert_batch_parallel(
                &args.db_path,
                &args.output_dir,
            )
            .with_context(|| "parallel batch convert failed")?;
        }
    }

    Ok(())
}
