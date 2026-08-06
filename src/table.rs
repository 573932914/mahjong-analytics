//! 多表存储抽象。
//!
//! [`ConversionResult`] 封装一个牌谱的多张输出表（`snapshots` /
//! `game_info`），提供 Parquet 读写。

use std::collections::HashMap;
use std::path::Path;

use polars::prelude::*;

/// 一个牌谱的转换结果 — 多张命名 DataFrame 的集合。
///
/// # 表约定
/// - `"snapshots"` — 快照表（每行一次决策，52 列）
/// - `"game_info"` — 牌谱元信息（每局 1 行）
#[derive(Debug)]
pub struct ConversionResult {
    /// 命名表集合。Key = 表名，Value = Polars DataFrame。
    pub tables: HashMap<String, DataFrame>,
}

impl ConversionResult {
    /// 创建一个空结果（无表）。
    pub fn empty() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    /// 表数量。
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    /// 是否无任何表。
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// 将所有表写入指定目录，每张表写为一个 `{name}.parquet` 文件。
    ///
    /// # 参数
    /// - `output_dir`: 目标目录（不存在则自动创建）
    ///
    /// # 返回
    /// - 写出的文件路径列表
    pub fn save(
        &mut self,
        output_dir: &Path,
    ) -> anyhow::Result<Vec<std::path::PathBuf>> {
        std::fs::create_dir_all(output_dir)?;
        let mut written = Vec::new();
        for (name, df) in &mut self.tables {
            let path = output_dir.join(format!("{name}.parquet"));
            let mut f = std::fs::File::create(&path)?;
            ParquetWriter::new(&mut f).finish(df)?;
            written.push(path);
        }
        Ok(written)
    }

    /// 从目录中读取所有 `.parquet` 文件，恢复为 `ConversionResult`。
    ///
    /// 文件名的主干（不含扩展名）作为表名。
    ///
    /// # 参数
    /// - `input_dir`: 包含 `.parquet` 文件的目录
    pub fn load(
        input_dir: &Path,
    ) -> anyhow::Result<Self> {
        let mut tables = HashMap::new();
        for entry in std::fs::read_dir(input_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "parquet")
            {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let mut f = std::fs::File::open(&path)?;
                let df = ParquetReader::new(&mut f).finish()?;
                tables.insert(name, df);
            }
        }
        Ok(Self { tables })
    }
}
