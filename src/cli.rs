//! CLI 参数定义。
//!
//! 使用 `clap` derive API 定义 `mahjong-analytics` 的两个子命令：
//! `convert` 和 `batch`。每个子命令的 `Args` 结构体包含位置参数
//! 和可选参数，通过 `#[arg]` 属性指定帮助文本。

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// 将 mjlog XML 转换为 Polars DataFrame (Parquet)。
#[derive(Parser)]
#[command(name = "mahjong-analytics")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// 转换单个 mjlog XML 文件为 Parquet 表。
    Convert(ConvertArgs),

    /// 从 houou-logs SQLite 数据库批量转换所有已下载的 XML。
    Batch(BatchArgs),

    /// 并发批量转换（多线程），每局输出一个 parquet 文件。
    ParBatch(ParBatchArgs),
}

// ── convert 参数 ──────────────────────────────────────────────

/// `convert` 子命令的参数。
#[derive(Args)]
pub struct ConvertArgs {
    /// mjlog XML 文件路径。
    ///
    /// 支持直接 XML 或 gzip 压缩（`.xml.gz`）。
    #[arg(value_name = "XML_FILE")]
    pub input: PathBuf,

    /// 输出 Parquet 文件路径。
    ///
    /// 默认 = 输入文件主文件名 + `.parquet`。
    #[arg(short = 'o', long, value_name = "PARQUET")]
    pub output: Option<PathBuf>,
}

// ── batch 参数 ────────────────────────────────────────────────

/// `batch` 子命令的参数。
#[derive(Args)]
pub struct BatchArgs {
    /// houou-logs SQLite 数据库的路径。
    #[arg(value_name = "DB_PATH")]
    pub db_path: PathBuf,

    /// 输出根目录。
    ///
    /// 每局写为一个子目录（以 log ID 命名），每个子目录下
    /// 包含 `snapshots.parquet`。
    #[arg(
        short = 'o',
        long,
        value_name = "DIR",
        default_value = "tables"
    )]
    pub output_dir: PathBuf,
}

// ── par-batch 参数 ────────────────────────────────────────────

#[derive(Args)]
pub struct ParBatchArgs {
    /// houou-logs SQLite 数据库路径。
    #[arg(value_name = "DB_PATH")]
    pub db_path: PathBuf,

    /// 输出目录（每局一个 `{game_id}.parquet` 文件）。
    #[arg(short = 'o', long, value_name = "DIR", default_value = "tables")]
    pub output_dir: PathBuf,
}
