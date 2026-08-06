//! 快照可视化模块。
//!
//! 基于 egui/eframe 的桌面 GUI，渲染麻雀牌桌。
//!
//! # 子模块
//! - [`app`] — [`VizApp`] 状态管理与 eframe 集成
//! - [`board`] — 牌桌布局渲染（正方形区域，4 家手牌/牌河/副露）
//! - [`tiles`] — 牌名映射、颜色、`TileAssets` 图片加载

pub mod app;
pub mod board;
pub mod tiles;
