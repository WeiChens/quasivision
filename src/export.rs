use std::path::Path;

use ab_glyph::{FontArc, PxScale};
use image::{DynamicImage, RgbImage, Rgb};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use serde_json;

use crate::element::{Element, OutputResult, CompactElement, AiOutput, TreeOutput, elements_to_text};
use crate::object_detector::DetectionNode;

/// 加载系统字体用于文本渲染
fn load_detection_font() -> Option<FontArc> {
    // 各平台字体目录
    let font_dirs: &[&str] = if cfg!(target_os = "windows") {
        &[r"C:\Windows\Fonts"]
    } else if cfg!(target_os = "macos") {
        &["/System/Library/Fonts", "/Library/Fonts", "~/Library/Fonts"]
    } else {
        &["/usr/share/fonts", "/usr/local/share/fonts", "~/.fonts"]
    };

    // 首选英文字体（跨平台通用）+ 中文字体备选
    #[cfg(target_os = "windows")]
    let preferred = ["arial.ttf", "msyh.ttc", "simsun.ttc", "msyhbd.ttf"];
    #[cfg(target_os = "macos")]
    let preferred = ["Helvetica.ttc", "PingFang.ttc", "STHeiti Light.ttc", "Arial Unicode.ttf"];
    #[cfg(target_os = "linux")]
    let preferred = ["DejaVuSans.ttf", "NotoSansCJK-Regular.ttc", "WenQuanYiMicroHei.ttf", "LiberationSans-Regular.ttf"];

    // 扫描各字体目录
    for dir_str in font_dirs {
        let dir = Path::new(dir_str);
        if !dir.exists() {
            continue;
        }
        // 先试首选字体
        for name in &preferred {
            let path = dir.join(name);
            if path.exists() {
                if let Ok(data) = std::fs::read(&path) {
                    if let Ok(font) = FontArc::try_from_vec(data) {
                        return Some(font);
                    }
                }
            }
        }
        // 再扫目录下任意 .ttf/.ttc
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "ttf" || ext == "ttc" {
                    if let Ok(data) = std::fs::read(&p) {
                        if let Ok(font) = FontArc::try_from_vec(data) {
                            return Some(font);
                        }
                    }
                }
            }
        }
    }

    None
}

/// 递归展平检测树为列表（所有层级）
fn flatten_detections(nodes: &[DetectionNode]) -> Vec<DetectionNode> {
    let mut result = Vec::new();
    for node in nodes {
        result.push(DetectionNode {
            class_name: node.class_name.clone(),
            confidence: node.confidence,
            bbox: node.bbox,
            children: Vec::new(),
        });
        result.extend(flatten_detections(&node.children));
    }
    result
}

/// 写入文件（自动创建父目录）
fn write_output(path: &str, data: &str) -> std::io::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, data)
}

/// 将元素列表导出为 JSON 文件
pub fn save_json(elements: &[Element], img_shape: (u32, u32), output_path: &str) -> anyhow::Result<()> {
    let result = OutputResult {
        comps: elements.to_vec(),
        img_shape,
    };

    let json_str = serde_json::to_string_pretty(&result)?;
    write_output(output_path, &json_str)?;
    println!("[Export] JSON saved to: {}", output_path);
    Ok(())
}

/// 保存压缩格式 JSON（短键名、拍平结构、去掉内部字段）
pub fn save_compact_json(elements: &[Element], _img_shape: (u32, u32), output_path: &str) -> anyhow::Result<()> {
    let compact: Vec<CompactElement> = elements.iter().map(CompactElement::from).collect();
    let json_str = serde_json::to_string(&compact)?;
    let byte_len = json_str.len();
    write_output(output_path, &json_str)?;
    println!("[Export] Compact JSON saved to: {} ({} bytes)", output_path, byte_len);
    Ok(())
}

/// 保存 AI 优化格式（坐标归一化到 0-1000）
pub fn save_ai_json(elements: &[Element], img_shape: (u32, u32), output_path: &str) -> anyhow::Result<()> {
    let ai = AiOutput::from_elements(elements, img_shape);
    let json_str = serde_json::to_string(&ai)?;
    let byte_len = json_str.len();
    write_output(output_path, &json_str)?;
    println!("[Export] AI-ready JSON saved to: {} ({} bytes)", output_path, byte_len);
    Ok(())
}

/// 保存纯文本表示
pub fn save_text_summary(elements: &[Element], img_shape: (u32, u32), output_path: &str) -> anyhow::Result<()> {
    let text = elements_to_text(elements, img_shape);
    write_output(output_path, &text)?;
    println!("[Export] Text summary saved to: {} ({} bytes, {} chars)", output_path, text.len(), text.chars().count());
    Ok(())
}

// ── 树形输出 ──

pub fn save_tree_json(elements: &[Element], img_shape: (u32, u32), output_path: &str) -> anyhow::Result<()> {
    let tree = TreeOutput::from_elements(elements, img_shape);
    let json_str = serde_json::to_string_pretty(&tree)?;
    let byte_len = json_str.len();
    write_output(output_path, &json_str)?;
    println!("[Export] Tree JSON saved to: {} ({} bytes)", output_path, byte_len);
    Ok(())
}

pub fn save_tree_text(elements: &[Element], img_shape: (u32, u32), output_path: &str) -> anyhow::Result<()> {
    let tree = TreeOutput::from_elements(elements, img_shape);
    let text = tree.to_text();
    write_output(output_path, &text)?;
    println!("[Export] Tree text saved to: {} ({} bytes, {} chars)",
        output_path, text.len(), text.chars().count());
    Ok(())
}

// ── 物体检测树形输出 ──

pub fn save_detection_tree_json(
    roots: &[DetectionNode],
    img_shape: (u32, u32),
    output_path: &str,
) -> anyhow::Result<()> {
    let output = serde_json::json!({
        "img_shape": [img_shape.1, img_shape.0],
        "count": count_all(roots),
        "objects": roots,
    });
    let json_str = serde_json::to_string_pretty(&output)?;
    write_output(output_path, &json_str)?;
    println!(
        "[Export] Detection tree JSON saved to: {} ({} bytes, {} objects)",
        output_path, json_str.len(), count_all(roots)
    );
    Ok(())
}

fn count_all(nodes: &[DetectionNode]) -> usize {
    nodes.iter().map(|n| 1 + count_all(&n.children)).sum()
}

pub fn save_detection_tree_text(
    roots: &[DetectionNode],
    img_shape: (u32, u32),
    output_path: &str,
) -> anyhow::Result<()> {
    if roots.is_empty() {
        let text = format!("Objects ({}×{}):\n  (none detected)", img_shape.1, img_shape.0);
        write_output(output_path, &text)?;
        println!("[Export] Detection tree text saved to: {} (no objects)", output_path);
        return Ok(());
    }

    let total = count_all(roots);
    let mut lines = Vec::new();
    lines.push(format!("Objects ({}×{}) — {} found:", img_shape.1, img_shape.0, total));

    fn render_node(node: &DetectionNode, prefix: &str, is_last: bool, lines: &mut Vec<String>) {
        let connector = if is_last { "└─ " } else { "├─ " };
        let x = node.bbox.x_min.round() as i32;
        let y = node.bbox.y_min.round() as i32;
        let w = (node.bbox.x_max - node.bbox.x_min).round() as i32;
        let h = (node.bbox.y_max - node.bbox.y_min).round() as i32;
        let pct = (node.confidence * 100.0).round() as u32;
        lines.push(format!("{}{}[{:>3},{:>3} {:>3}×{:>3}] {} ({}%)", prefix, connector, x, y, w, h, node.class_name, pct));
        let child_prefix = if is_last { "   " } else { "│  " };
        let new_prefix = format!("{}{}", prefix, child_prefix);
        let count = node.children.len();
        for (i, child) in node.children.iter().enumerate() {
            render_node(child, &new_prefix, i == count - 1, lines);
        }
    }

    let root_count = roots.len();
    for (i, root) in roots.iter().enumerate() {
        render_node(root, "", i == root_count - 1, &mut lines);
    }

    let text = lines.join("\n");
    write_output(output_path, &text)?;
    println!("[Export] Detection tree text saved to: {} ({} bytes, {} chars)", output_path, text.len(), text.chars().count());
    Ok(())
}

// ── 元素可视化 ──

pub fn draw_elements(img: &DynamicImage, elements: &[Element]) -> RgbImage {
    let mut rgb = img.to_rgb8();

    let color_map: Vec<(&str, Rgb<u8>)> = vec![
        ("Text", Rgb([0, 0, 255])),
        ("Compo", Rgb([255, 0, 255])),
        ("Block", Rgb([0, 255, 0])),
        ("Image", Rgb([255, 0, 0])),
        ("Button", Rgb([0, 200, 0])),
        ("Icon", Rgb([255, 165, 0])),
        ("Text Content", Rgb([255, 0, 255])),
        ("Noise", Rgb([6, 6, 255])),
    ];

    fn put_safe(rgb: &mut RgbImage, x: i32, y: i32, color: Rgb<u8>) {
        if x >= 0 && (x as u32) < rgb.width() && y >= 0 && (y as u32) < rgb.height() {
            rgb.put_pixel(x as u32, y as u32, color);
        }
    }

    fn draw_rect(rgb: &mut RgbImage, c1: i32, r1: i32, c2: i32, r2: i32, color: Rgb<u8>) {
        for x in c1..=c2 {
            put_safe(rgb, x, r1, color);
            put_safe(rgb, x, r1 + 1, color);
            put_safe(rgb, x, r2, color);
            put_safe(rgb, x, r2 - 1, color);
        }
        for y in r1..=r2 {
            put_safe(rgb, c1, y, color);
            put_safe(rgb, c1 + 1, y, color);
            put_safe(rgb, c2, y, color);
            put_safe(rgb, c2 - 1, y, color);
        }
    }

    for ele in elements {
        let color = color_map.iter()
            .find(|(name, _)| *name == ele.class)
            .map(|(_, c)| *c)
            .unwrap_or(Rgb([0, 255, 0]));
        let (c1, r1, c2, r2) = ele.put_bbox();
        draw_rect(&mut rgb, c1, r1, c2, r2, color);
    }

    rgb
}

pub fn save_visualization(img: &DynamicImage, elements: &[Element], output_path: &str) -> anyhow::Result<()> {
    let vis = draw_elements(img, elements);
    if let Some(parent) = Path::new(output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    vis.save(output_path)?;
    println!("[Export] Visualization saved to: {}", output_path);
    Ok(())
}

// ── 物体检测可视化（所有层级，系统字体渲染标签） ──

/// 在图片上绘制物体检测结果（所有层级）
///
/// - 所有检测框用 2px 彩色边框绘制
/// - 框内右上角用系统字体渲染"类别 置信度%"标签（空间够且不遮挡则画）
pub fn draw_object_detections(img: &DynamicImage, roots: &[DetectionNode]) -> RgbImage {
    let mut rgb = img.to_rgb8();

    // 展平所有节点
    let all_nodes = flatten_detections(roots);
    if all_nodes.is_empty() {
        return rgb;
    }

    // 加载字体
    let font = load_detection_font();
    if font.is_none() {
        eprintln!("  [WARN] No Chinese font found — detection labels will use fallback");
    }

    const COLORS: &[Rgb<u8>] = &[
        Rgb([255, 0, 0]),     Rgb([0, 180, 0]),
        Rgb([0, 100, 255]),   Rgb([255, 165, 0]),
        Rgb([180, 0, 255]),   Rgb([0, 200, 200]),
        Rgb([255, 0, 180]),   Rgb([200, 200, 0]),
    ];

    struct BoxInfo { x1: i32, y1: i32, x2: i32, y2: i32, label: String, color: Rgb<u8> }

    let boxes: Vec<BoxInfo> = all_nodes.iter().enumerate().map(|(i, node)| {
        let x1 = node.bbox.x_min.round() as i32;
        let y1 = node.bbox.y_min.round() as i32;
        let x2 = node.bbox.x_max.round() as i32;
        let y2 = node.bbox.y_max.round() as i32;
        let pct = (node.confidence * 100.0).round() as u32;
        BoxInfo {
            x1, y1, x2, y2,
            label: format!("{} {}%", node.class_name, pct),
            color: COLORS[i % COLORS.len()],
        }
    }).collect();

    // 第一遍：画所有框（2px 粗）
    for info in &boxes {
        let w = (info.x2 - info.x1).max(0) as u32;
        let h = (info.y2 - info.y1).max(0) as u32;
        if w < 2 || h < 2 { continue; }
        let r = Rect::at(info.x1, info.y1).of_size(w, h);
        draw_hollow_rect_mut(&mut rgb, r, info.color);
        let r2 = Rect::at(info.x1 + 1, info.y1 + 1).of_size(w.saturating_sub(2), h.saturating_sub(2));
        draw_hollow_rect_mut(&mut rgb, r2, info.color);
    }

    // 第二遍：画标签（框内右上角）
    if let Some(ref f) = font {
        let scale = PxScale::from(13.0);
        let black = Rgb([0, 0, 0]);

        for info in &boxes {
            let tw = imageproc::drawing::text_size(scale, f, &info.label).0 as i32;
            let th = 16;
            let pad = 3;

            // 右上角位置
            let lx1 = info.x2 - tw - pad;
            let ly1 = info.y1 + 2;
            let lx2 = info.x2 - 2;
            let ly2 = ly1 + th;

            if lx1 <= info.x1 || ly2 >= info.y2 { continue; }

            // 检查是否遮挡其他框（>30% 标签面积重叠则跳过）
            let mut blocked = false;
            for other in &boxes {
                if other.x1 == info.x1 && other.y1 == info.y1 { continue; }
                let ox = lx1.max(other.x1);
                let oy = ly1.max(other.y1);
                let ox2 = lx2.min(other.x2);
                let oy2 = ly2.min(other.y2);
                if ox < ox2 && oy < oy2 {
                    let inter = (ox2 - ox) as i64 * (oy2 - oy) as i64;
                    let label_area = (lx2 - lx1) as i64 * (ly2 - ly1) as i64;
                    if label_area > 0 && (inter * 100 / label_area) > 30 {
                        blocked = true;
                        break;
                    }
                }
            }
            if blocked { continue; }

            // 白色背景
            let bg_r = Rect::at(lx1, ly1).of_size((lx2 - lx1) as u32, (ly2 - ly1) as u32);
            draw_filled_rect_mut(&mut rgb, bg_r, Rgb([255, 255, 255]));
            // 标签边框
            draw_hollow_rect_mut(&mut rgb, bg_r, info.color);
            // 文字
            draw_text_mut(&mut rgb, black, lx1 + 2, ly1, scale, f, &info.label);
        }
    } else {
        // 无字体 → 画小色块代替标签
        for info in &boxes {
            let s = 8;
            let ix1 = info.x2 - s - 2;
            let iy1 = info.y1 + 2;
            let ir = Rect::at(ix1, iy1).of_size(s as u32, s as u32);
            draw_filled_rect_mut(&mut rgb, ir, info.color);
        }
    }

    rgb
}

pub fn save_object_detection_visualization(
    img: &DynamicImage,
    roots: &[DetectionNode],
    output_path: &str,
) -> anyhow::Result<()> {
    let vis = draw_object_detections(img, roots);
    if let Some(parent) = Path::new(output_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    vis.save(output_path)?;
    println!("[Export] Object detection visualization saved to: {}", output_path);
    Ok(())
}
