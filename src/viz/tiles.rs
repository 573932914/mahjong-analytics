//! 牌 ID 映射、显示辅助、牌画图片加载。
//!
//! # 两层渲染策略
//! 1. **图片模式**：从 `../mahjim/` 加载 PNG 牌画 → 注册为 egui 纹理
//! 2. **文字模式**（回落）：彩色矩形 + 牌名 Unicode 文本
//!
//! # 天凤 Tile ID 编码
//! 0-8=万子, 9-17=筒子, 18-26=索子, 27-30=风牌, 31-33=三元牌,
//! 34=赤5m, 37=赤5p, 40=赤5s

use std::collections::HashMap;
use std::path::PathBuf;

// ── 牌名 / 颜色 / 标签 ────────────────────────────────────────

/// 天凤实例 ID → 牌种 ID（÷4 去实例编号）。
///
/// 天凤内部用 0-135 区分 34 种 × 4 枚 = 136 张。
/// 牌种 ID = `instance_id / 4`。
pub fn instance_to_type(instance_id: i32) -> i32 {
    instance_id / 4
}

/// 牌种 ID → 人可读牌名。
///
/// 必须先经过 `instance_to_type` 转换。
/// `type_id` = 0-8 万子, 9-17 筒子, 18-26 索子, 27-33 字牌。
pub fn tile_name(type_id: i32) -> &'static str {
    match type_id {
        0 => "1m", 1 => "2m", 2 => "3m", 3 => "4m",
        4 => "5m", 5 => "6m", 6 => "7m", 7 => "8m", 8 => "9m",
        9 => "1p", 10 => "2p", 11 => "3p", 12 => "4p",
        13 => "5p", 14 => "6p", 15 => "7p", 16 => "8p", 17 => "9p",
        18 => "1s", 19 => "2s", 20 => "3s", 21 => "4s",
        22 => "5s", 23 => "6s", 24 => "7s", 25 => "8s", 26 => "9s",
        27 => "E", 28 => "S", 29 => "W", 30 => "N",
        31 => "Wh", 32 => "Gr", 33 => "Rd",
        34 => "5mr", 37 => "5pr", 40 => "5sr",
        _ => "??",
    }
}

/// 牌面背景色（按花色区分，用于文字回落模式）。
///
/// 万子=暗红, 筒子=暗蓝, 索子=暗绿, 风=暗灰, 三元=白/绿/红。
pub fn tile_color(id: i32) -> egui::Color32 {
    match id {
        0..=8 | 34 => egui::Color32::from_rgb(180, 60, 40),
        9..=17 | 37 => egui::Color32::from_rgb(30, 70, 160),
        18..=26 | 40 => egui::Color32::from_rgb(20, 130, 60),
        27..=30 => egui::Color32::from_rgb(60, 60, 60),
        31 => egui::Color32::from_rgb(220, 220, 210),
        32 => egui::Color32::from_rgb(30, 140, 30),
        33 => egui::Color32::from_rgb(180, 30, 30),
        _ => egui::Color32::from_rgb(128, 128, 128),
    }
}

/// 该 tile ID 是否为赤宝牌。
pub fn is_red_dora(id: i32) -> bool {
    matches!(id, 34 | 37 | 40)
}

/// 行动类型 → 中文标签。
pub fn action_label_cn(s: &str) -> &str {
    match s {
        "tsumogiri" => "摸切",
        "discard" => "手出",
        "reach" => "立直",
        "chi" => "吃",
        "pon" => "碰",
        "daiminkan" => "大明杠",
        "shominkan" => "小明杠",
        "ankan" => "暗杠",
        "kakan" => "加杠",
        "tsumo" => "自摸",
        "ron" => "荣和",
        _ => s,
    }
}

/// 行动类型 → ASCII 短标签（egui 默认字体无 CJK）。
pub fn action_label(s: &str) -> &str {
    match s {
        "tsumogiri" => "tsumo-giri",
        "discard" => "tedashi",
        "reach" => "REACH",
        "chi" => "CHI",
        "pon" => "PON",
        "daiminkan" => "Daiminkan",
        "shominkan" => "Shominkan",
        "ankan" => "Ankan",
        "kakan" => "Kakan",
        "tsumo" => "TSUMO AGARI",
        "ron" => "RON AGARI",
        _ => s,
    }
}

/// 玩家座位标签（中文 + 当前行动者箭头标记）。
///
/// # 参数
/// - `i`: 座位号（0=自家, 1=下家, 2=対面, 3=上家）
/// - `actor`: 当前行动者（-1 = 无）
pub fn seat_label(i: usize, actor: i8) -> String {
    let base = match i {
        0 => "P0(Self)",
        1 => "P1(Right)",
        2 => "P2(Across)",
        _ => "P3(Left)",
    };
    if i as i8 == actor {
        format!("*{base}")
    } else {
        base.to_string()
    }
}

// ── 牌画文件映射 ──────────────────────────────────────────────

/// Tenhou tile ID → mahjim 素材文件名。
///
/// mahjim 项目使用语义化命名（`1mjp.png`, `東jp.png`），
/// 而不按天凤内部编号。
///
/// # 映射表
/// | Tenhou ID | mahjim 文件 |
/// |-----------|------------|
/// | 0-8       | 1mjp~9mjp  |
/// | 9-17      | 1p~9p      |
/// | 18        | 1sjp       |
/// | 19-26     | 2s~9s      |
/// | 27-30     | 東/南/西/北jp |
/// | 31-33     | 白/发/中jp  |
/// | 34,37,40  | 5mjp/5p/5s (暂无专用赤宝图片) |
fn tile_filename(id: i32) -> String {
    match id {
        0 => "1mjp.png".into(),
        1 => "2mjp.png".into(),
        2 => "3mjp.png".into(),
        3 => "4mjp.png".into(),
        4 => "5mjp.png".into(),
        5 => "6mjp.png".into(),
        6 => "7mjp.png".into(),
        7 => "8mjp.png".into(),
        8 => "9mjp.png".into(),
        9 => "1p.png".into(),
        10 => "2p.png".into(),
        11 => "3p.png".into(),
        12 => "4p.png".into(),
        13 => "5p.png".into(),
        14 => "6p.png".into(),
        15 => "7p.png".into(),
        16 => "8p.png".into(),
        17 => "9p.png".into(),
        18 => "1sjp.png".into(),
        19 => "2s.png".into(),
        20 => "3s.png".into(),
        21 => "4s.png".into(),
        22 => "5s.png".into(),
        23 => "6s.png".into(),
        24 => "7s.png".into(),
        25 => "8s.png".into(),
        26 => "9s.png".into(),
        27 => "东jp.png".into(),  // 注意: 素材用简体"东"而非日文"東"
        28 => "南jp.png".into(),
        29 => "西jp.png".into(),
        30 => "北jp.png".into(),
        31 => "白jp.png".into(),
        32 => "发jp.png".into(),
        33 => "中jp.png".into(),
        34 => "5mjp.png".into(), // 赤5m：暂用普通5m图片
        37 => "5p.png".into(),   // 赤5p：暂用普通5p图片
        40 => "5s.png".into(),   // 赤5s：暂用普通5s图片
        _ => format!("{id}.png"),
    }
}

// ── 图片资产 ──────────────────────────────────────────────────

/// 牌背纹理的特殊 ID。
pub const BACK_TILE_ID: i32 = -1;

/// 缓存的牌画图片资产。
pub struct TileAssets {
    images: HashMap<i32, egui::ColorImage>,
    textures: HashMap<i32, egui::TextureHandle>,
    tried_load: bool,
}

impl TileAssets {
    /// 创建空的资产容器。
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            textures: HashMap::new(),
            tried_load: false,
        }
    }

    /// 扫描目录加载牌画 PNG。
    ///
    /// # 搜索路径（优先级从高到低）
    /// 1. 用户指定的 `base_dir`
    /// 2. `../../mahjim/mahjim-master/assets/files/`（解压后的 mahjim）
    /// 3. `../../mahjim/`（扁平结构）
    /// 4. `../mahjim/`（exe 同级）
    ///
    /// # 说明
    /// 调用一次后设置 `tried_load = true`，后续调用无效。
    pub fn discover(&mut self, base_dir: Option<PathBuf>) {
        if self.tried_load {
            return;
        }
        self.tried_load = true;

        // 构建搜索目录列表
        let search_dirs: Vec<PathBuf> = if let Some(d) = base_dir {
            vec![d]
        } else {
            let exe = std::env::current_exe().unwrap_or_default();
            let exe_dir =
                exe.parent().unwrap_or(std::path::Path::new("."));
            vec![
                // 解压后的 mahjim 标准结构
                exe_dir
                    .join("..")
                    .join("..")
                    .join("..")
                    .join("mahjim")
                    .join("mahjim-master")
                    .join("assets")
                    .join("files"),
                exe_dir.join("..").join("..").join("mahjim"),
                exe_dir.join("..").join("mahjim"),
            ]
        };

        // 找第一个包含 `1mjp.png` 的目录
        let mut found_dir = None;
        for d in &search_dirs {
            if d.join("1mjp.png").exists() {
                found_dir = Some(d.clone());
                break;
            }
        }

        let dir = match found_dir {
            Some(d) => {
                eprintln!("[mahjong-viz] 找到牌画目录: {}", d.display());
                d
            }
            None => {
                eprintln!(
                    "[mahjong-viz] 未找到牌画目录，搜索路径: {search_dirs:?}"
                );
                return;
            }
        };

        // 逐一加载 0-40 号 tile
        let mut loaded = 0usize;
        let mut missing = vec![];
        for id in 0..=40i32 {
            let name = tile_filename(id);
            let path = dir.join(&name);
            if path.exists() {
                let decoded = std::fs::read(&path)
                    .ok()
                    .and_then(|buf| {
                        image::load_from_memory(&buf).ok()
                    });
                if let Some(img) = decoded {
                    let rgba = img.to_rgba8();
                    let size = [
                        rgba.width() as usize,
                        rgba.height() as usize,
                    ];
                    let pixels = rgba.into_raw();
                    let color_image =
                        egui::ColorImage::from_rgba_unmultiplied(
                            size, &pixels,
                        );
                    self.images.insert(id, color_image);
                    loaded += 1;
                }
            } else {
                missing.push(id);
            }
        }
        // 加载牌背 (blue.png)
        let back_path = dir.join("blue.png");
        if back_path.exists() {
            if let Ok(buf) = std::fs::read(&back_path) {
                if let Ok(img) = image::load_from_memory(&buf) {
                    let rgba = img.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw());
                    self.images.insert(BACK_TILE_ID, color_image);
                    loaded += 1;
                }
            }
        }

        eprintln!("[mahjong-viz] 牌画加载: {loaded}/42 张成功");
        if !missing.is_empty() {
            eprintln!("[mahjong-viz] 缺少牌画 tile ID: {missing:?}");
            eprintln!("[mahjong-viz] 缺少的文件名: {:?}",
                missing.iter().map(|&id| (id, tile_filename(id))).collect::<Vec<_>>());
        }
    }

    /// 将已加载的像素数据注册为 egui 纹理。
    ///
    /// 需要在每帧调用（内部幂等：已注册的不重复注册）。
    ///
    /// # 参数
    /// - `ctx`: egui 上下文（提供纹理管理）
    pub fn ensure_textures(&mut self, ctx: &egui::Context) {
        let ids: Vec<i32> =
            self.images.keys().copied().collect();
        for id in ids {
            if self.textures.contains_key(&id) {
                continue;
            }
            let img = self.images.get(&id).unwrap().clone();
            let tex = ctx.load_texture(
                format!("tile_{id}"),
                img,
                egui::TextureOptions::LINEAR,
            );
            self.textures.insert(id, tex);
        }
    }

    /// 获取指定 tile ID 的纹理句柄（如果已加载）。
    ///
    /// # 返回
    /// - `Some(&TextureHandle)` — 图片已加载
    /// - `None` — 图片未找到或尚未注册
    pub fn texture(
        &self,
        id: i32,
    ) -> Option<&egui::TextureHandle> {
        self.textures.get(&id)
    }

    /// 是否有任何图片资产已加载。
    pub fn has_any(&self) -> bool {
        !self.images.is_empty()
    }

    /// 已从磁盘加载的图片数。
    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    /// 已注册为纹理的图片数。
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// 获取牌背纹理。
    pub fn back_texture(&self) -> Option<&egui::TextureHandle> {
        self.textures.get(&BACK_TILE_ID)
    }
}
