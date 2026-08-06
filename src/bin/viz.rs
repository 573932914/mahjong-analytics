//! `mahjong-viz` — 快照可视化桌面应用。
//!
//! # 用法
//! ```sh
//! mahjong-viz [snapshots.parquet]   # 可选：启动时直接加载
//! mahjong-viz                        # 启动后通过 File > Open 加载
//! ```
//!
//! # 键盘快捷键
//! - ← →   上一手 / 下一手
//! - Home  第一手（turn 0）
//! - End   最后一手

use std::path::PathBuf;

use mahjong_analytics::viz::app::VizApp;

fn main() -> anyhow::Result<()> {
    // 命令行可选参数：parquet 文件路径
    let args: Vec<String> = std::env::args().collect();
    let file = args.get(1).map(PathBuf::from);

    // 窗口配置：1400×900，标题含中文
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("麻雀牌譜快照可视化"),
        ..Default::default()
    };

    let mut app = VizApp::default();

    // 若命令行指定了文件且存在，自动加载
    if let Some(path) = file {
        if path.exists() {
            app.load_parquet(path);
        }
    }

    // 启动 eframe 主循环
    eframe::run_native(
        "mahjong-viz",
        options,
        Box::new(|cc| {
            // 注册 CJK 字体以支持中文渲染
            let mut fonts = egui::FontDefinitions::default();
            // 尝试 Windows 系统中的中文字体
            for font_path in &[
                "C:\\Windows\\Fonts\\msyh.ttc",       // 微软雅黑
                "C:\\Windows\\Fonts\\simhei.ttf",      // 黑体
                "C:\\Windows\\Fonts\\simsun.ttc",      // 宋体
            ] {
                if std::path::Path::new(font_path).exists() {
                    fonts.font_data.insert(
                        "CJK".to_string(),
                        std::sync::Arc::new(
                            egui::FontData::from_owned(
                                std::fs::read(font_path)
                                    .unwrap_or_default(),
                            ),
                        ),
                    );
                    // 将 CJK 字体作为默认字体的首选回退
                    fonts.families
                        .entry(egui::FontFamily::Proportional)
                        .or_default()
                        .insert(0, "CJK".to_string());
                    fonts.families
                        .entry(egui::FontFamily::Monospace)
                        .or_default()
                        .push("CJK".to_string());
                    eprintln!("[mahjong-viz] 加载中文字体: {font_path}");
                    break;
                }
            }
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}
