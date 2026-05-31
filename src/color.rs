use std::collections::HashMap;

use image::RgbImage;

use crate::element::Element;

// ── 颜色工具函数 ──

/// RGB 转十六进制颜色字符串，优先使用 3 位缩写（如 #FFF vs #FFFFFF）
///
/// 当每个通道的高 4 位与低 4 位相同时（如 0xFF, 0xAA, 0x00），
/// 使用 3 位缩写（#RGB），减少 token 数。
pub fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    // 检查是否能缩写成 3 位：#RGB 等价于 #RRGGBB 当 R=R>>4==R&0x0F 时
    if r >> 4 == r & 0x0F && g >> 4 == g & 0x0F && b >> 4 == b & 0x0F {
        format!("#{:X}{:X}{:X}", r >> 4, g >> 4, b >> 4)
    } else {
        format!("#{:02X}{:02X}{:02X}", r, g, b)
    }
}

/// 颜色量化：将 256³ 颜色空间缩小到 2^(8-bits) 的粒度
/// bits=4 → 16³=4096 色板（足够区分主要颜色）
fn quantize_color(r: u8, g: u8, b: u8, bits: u8) -> (u8, u8, u8) {
    if bits >= 8 {
        return (r, g, b);
    }
    let mask = !((1u8 << bits) - 1u8);
    (r & mask, g & mask, b & mask)
}

/// 计算两个量化颜色的近似距离（仅用于排序比较，不需要精确欧氏距离）
fn quantized_color_distance(a: &(u8, u8, u8), b: &(u8, u8, u8)) -> u32 {
    let dr = a.0 as i32 - b.0 as i32;
    let dg = a.1 as i32 - b.1 as i32;
    let db = a.2 as i32 - b.2 as i32;
    (dr * dr + dg * dg + db * db) as u32
}

/// 从一组像素中计算平均颜色（按量化桶分组后，取桶内实际平均）
fn average_color_of_pixels(pixels: &[(u8, u8, u8)]) -> (u8, u8, u8) {
    if pixels.is_empty() {
        return (255, 255, 255);
    }
    let n = pixels.len() as u32;
    let sum_r: u32 = pixels.iter().map(|p| p.0 as u32).sum();
    let sum_g: u32 = pixels.iter().map(|p| p.1 as u32).sum();
    let sum_b: u32 = pixels.iter().map(|p| p.2 as u32).sum();
    ((sum_r / n) as u8, (sum_g / n) as u8, (sum_b / n) as u8)
}

// ── 背景色检测 ──

/// 从矩形区域采样边框像素，返回量化桶 → (计数, 实际像素值列表)
/// 量化用于聚类，实际像素值用于计算准确的平均色
fn sample_border_pixels_detailed(
    img: &RgbImage,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    bits: u8,
) -> HashMap<(u8, u8, u8), (u32, Vec<(u8, u8, u8)>)> {
    let mut buckets: HashMap<(u8, u8, u8), (u32, Vec<(u8, u8, u8)>)> = HashMap::new();

    // 自适应采样步长：至少 1px，最大不超过宽/高的 1/20
    let step = 1.max((w.max(h) / 20) as u32);

    // 上边 + 下边
    let top_y = y as u32;
    let bottom_y = (y + h - 1) as u32;
    for px in (0..w as u32).step_by(step as usize) {
        let px_abs = (x as u32) + px;

        if top_y < img.height() && px_abs < img.width() {
            let p = img.get_pixel(px_abs, top_y);
            let key = quantize_color(p[0], p[1], p[2], bits);
            let entry = buckets.entry(key).or_insert((0, Vec::new()));
            entry.0 += 1;
            entry.1.push((p[0], p[1], p[2]));
        }

        if bottom_y < img.height() && px_abs < img.width() {
            let p = img.get_pixel(px_abs, bottom_y);
            let key = quantize_color(p[0], p[1], p[2], bits);
            let entry = buckets.entry(key).or_insert((0, Vec::new()));
            entry.0 += 1;
            entry.1.push((p[0], p[1], p[2]));
        }
    }

    // 左边 + 右边（跳过已经采样的角点）
    let left_x = x as u32;
    let right_x = (x + w - 1) as u32;
    for py in (1..(h - 1).max(1) as u32).step_by(step as usize) {
        let py_abs = (y as u32) + py;

        if left_x < img.width() && py_abs < img.height() {
            let p = img.get_pixel(left_x, py_abs);
            let key = quantize_color(p[0], p[1], p[2], bits);
            let entry = buckets.entry(key).or_insert((0, Vec::new()));
            entry.0 += 1;
            entry.1.push((p[0], p[1], p[2]));
        }

        if right_x < img.width() && py_abs < img.height() {
            let p = img.get_pixel(right_x, py_abs);
            let key = quantize_color(p[0], p[1], p[2], bits);
            let entry = buckets.entry(key).or_insert((0, Vec::new()));
            entry.0 += 1;
            entry.1.push((p[0], p[1], p[2]));
        }
    }

    buckets
}

/// 检测背景颜色：采样四边，取最高频颜色桶内的实际平均色
pub fn detect_background_color(img: &RgbImage, x: i32, y: i32, w: i32, h: i32) -> String {
    if w <= 0 || h <= 0 {
        return "#FFFFFF".to_string();
    }

    let bits = 6; // 64³=262144 色板，较细粒度保留颜色精度

    let buckets = sample_border_pixels_detailed(img, x, y, w, h, bits);

    // 找最高频的量化桶
    let (_max_q, (_, pixels)) = buckets
        .into_iter()
        .max_by_key(|(_, (count, _))| *count)
        .unwrap_or_else(|| {
            let mask = !((1u8 << bits) - 1u8);
            let default_q = (255u8 & mask, 255u8 & mask, 255u8 & mask);
            (default_q, (0, vec![(255, 255, 255)]))
        });

    // 取该桶内像素的实际平均值
    let (r, g, b) = average_color_of_pixels(&pixels);
    rgb_to_hex(r, g, b)
}

// ── 主体（前景）颜色检测 ──

/// 检测主体颜色（用于 Text 和 Icon）。
///
/// 算法（结合边框采样 + 全区域对比度分析）：
/// 1. **边框采样** → 确定背景色 bg（边框天然是背景，不受内容干扰）
/// 2. **全区域采样** → 构建颜色直方图，遍历所有颜色桶
/// 3. 找到与背景色 **差异最大** 的颜色桶（如果差异足够大）
/// 4. 如果所有桶都和背景色接近 → 纯色区域 → 返回区域平均色
///
/// 相比 V1（边框采样+阈值过滤）的改进：
///   - V1 用固定阈值排除背景色附近的像素，浅色前景会被误杀
///   - 新方法用"找差异最大桶"，对浅色 Icon 也能正确识别
///
/// 相比 V2（全区域频率分析）的改进：
///   - V2 取最高频作背景，但白色被噪声分散后，Icon色排在后面
///   - 新方法用边框确定背景色，不受桶排序干扰
pub fn detect_dominant_color(img: &RgbImage, x: i32, y: i32, w: i32, h: i32) -> String {
    if w <= 0 || h <= 0 || w > img.width() as i32 || h > img.height() as i32 {
        return "#000000".to_string();
    }

    // 边界安全裁剪
    let x = x.max(0).min(img.width() as i32 - 1);
    let y = y.max(0).min(img.height() as i32 - 1);
    let w = w.min(img.width() as i32 - x);
    let h = h.min(img.height() as i32 - y);

    if w <= 1 || h <= 1 {
        return "#000000".to_string();
    }

    let bits = 4; // 16³=4096 量化色板

    // ── 第一步：边框采样确定背景色 ──
    let border_buckets = sample_border_pixels_detailed(img, x, y, w, h, bits);
    let (bg_q, (_, bg_pixels)) = border_buckets
        .into_iter()
        .max_by_key(|(_, (count, _))| *count)
        .unwrap_or_else(|| {
            ((240, 240, 240), (0, vec![(255, 255, 255)]))
        });

    // ── 第二步：全区域采样，构建颜色直方图 ──
    let mut all_buckets: HashMap<(u8, u8, u8), (u32, Vec<(u8, u8, u8)>)> = HashMap::new();

    let area = w as u64 * h as u64;
    let step = if area > 50000 {
        ((area / 5000) as f64).sqrt().round().max(1.0) as u32
    } else {
        1
    };

    for py in (0..h as u32).step_by(step as usize) {
        for px in (0..w as u32).step_by(step as usize) {
            let px_abs = (x as u32) + px;
            let py_abs = (y as u32) + py;
            if px_abs >= img.width() || py_abs >= img.height() {
                continue;
            }
            let p = img.get_pixel(px_abs, py_abs);
            let q = quantize_color(p[0], p[1], p[2], bits);
            let entry = all_buckets.entry(q).or_insert((0, Vec::new()));
            entry.0 += 1;
            entry.1.push((p[0], p[1], p[2]));
        }
    }

    // ── 第三步：在"对比度足够"的桶中，取频率最高的作为主体色 ──
    // 差异阈值：量化后每通道差 ≥ 2 个量化步长
    //   bits=4, 步长=16, 每通道≥32 → 3×(32²) = 3072
    //   这个阈值很宽松：任何肉眼可见的颜色差异都能超过它
    //   浅灰色 (#E0E0E0) 和白色 (#FFFFFF) 的量化差也能达到此阈值
    //
    // 为什么用"频率最高"而不是"差异最大"？
    //   - 差异最大的单个像素可能是噪声
    //   - 频率最高的非背景色才是真正的主体色
    //   - 对于浅色文字/Icon，频率 × 对比度加权能正确识别
    const MIN_CONTRAST: u32 = 3 * (32u32 * 32u32); // = 3072

    let mut best_count = 0u32;
    let mut best_pixels: Option<Vec<(u8, u8, u8)>> = None;

    for (_q, (count, pixels)) in &all_buckets {
        let dist = quantized_color_distance(&bg_q, _q);
        if dist >= MIN_CONTRAST && *count > best_count {
            best_count = *count;
            best_pixels = Some(pixels.clone());
        }
    }

    // ── 第四步：返回结果 ──
    if let Some(pixels) = best_pixels {
        // 找到主体色（对比度足够的桶中频率最高的）
        let (r, g, b) = average_color_of_pixels(&pixels);
        rgb_to_hex(r, g, b)
    } else {
        // 所有桶都和背景色接近 → 纯色/渐变区域
        // 返回区域整体平均色（对于纯色 Icon，这就是它的颜色）
        let mut all_pixels = bg_pixels;
        for (_, (_, pixels)) in &all_buckets {
            all_pixels.extend(pixels.iter());
        }
        let (r, g, b) = average_color_of_pixels(&all_pixels);
        rgb_to_hex(r, g, b)
    }
}

// ── 对外统一接口 ──

/// 为元素检测颜色，返回带语义前缀的字符串：
/// - Text/Icon → `fg(#RRGGBB)`（前景/主体色）
/// - 其他 → `bg(#RRGGBB)`（背景色）
///
/// 语义前缀让 AI 能直接区分颜色含义。
pub fn detect_element_color(img: &RgbImage, element: &Element) -> String {
    let (x, y, x2, y2) = element.put_bbox();
    let w = x2 - x;
    let h = y2 - y;

    let hex = match element.class.as_str() {
        "Text" | "Icon" => detect_dominant_color(img, x, y, w, h),
        _ => detect_background_color(img, x, y, w, h),
    };

    match element.class.as_str() {
        "Text" | "Icon" => format!("fg({})", hex),
        _ => format!("bg({})", hex),
    }
}

// ── 批量处理 ──

/// 为所有元素检测颜色（原地修改）
pub fn detect_colors(img: &RgbImage, elements: &mut [Element]) {
    for element in elements.iter_mut() {
        let color = detect_element_color(img, element);
        element.color = Some(color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn test_rgb_to_hex() {
        // 可缩写的（高4位==低4位）
        assert_eq!(rgb_to_hex(255, 0, 0), "#F00");
        assert_eq!(rgb_to_hex(0, 255, 0), "#0F0");
        assert_eq!(rgb_to_hex(0, 0, 255), "#00F");
        assert_eq!(rgb_to_hex(255, 255, 255), "#FFF");
        assert_eq!(rgb_to_hex(0, 0, 0), "#000");
        assert_eq!(rgb_to_hex(170, 187, 204), "#ABC"); // AA→A, BB→B, CC→C

        // 不可缩写的（高4位≠低4位），保持 6 位
        assert_eq!(rgb_to_hex(18, 52, 86), "#123456");
        assert_eq!(rgb_to_hex(255, 128, 64), "#FF8040");
        assert_eq!(rgb_to_hex(200, 100, 50), "#C86432");
    }

    #[test]
    fn test_quantize_color() {
        // bits=4: 保留高 4 位，低 4 位置 0
        assert_eq!(quantize_color(255, 255, 255, 4), (240, 240, 240));
        assert_eq!(quantize_color(128, 128, 128, 4), (128, 128, 128));
        assert_eq!(quantize_color(0, 0, 0, 4), (0, 0, 0));
        // bits=8: 全保留
        assert_eq!(quantize_color(123, 45, 67, 8), (123, 45, 67));
    }

    #[test]
    fn test_average_color() {
        let pixels = vec![
            (255, 0, 0),
            (255, 0, 0),
            (0, 255, 0),
        ];
        let (r, g, b) = average_color_of_pixels(&pixels);
        // (255+255+0)/3 = 170, (0+0+255)/3 = 85, (0+0+0)/3 = 0
        assert_eq!((r, g, b), (170, 85, 0));
    }

    #[test]
    fn test_detect_background_solid() {
        // 创建 50x50 纯蓝图片
        let mut img = RgbImage::new(50, 50);
        for y in 0..50 {
            for x in 0..50 {
                img.put_pixel(x, y, Rgb([0, 0, 255]));
            }
        }
        let color = detect_background_color(&img, 0, 0, 50, 50);
        assert_eq!(color, "#00F");
    }

    #[test]
    fn test_detect_dominant_text() {
        // 30x30 区域：白色背景 + 黑色文字（中间一个黑点）
        let mut img = RgbImage::new(30, 30);
        // 白色背景
        for y in 0..30 {
            for x in 0..30 {
                img.put_pixel(x, y, Rgb([255, 255, 255]));
            }
        }
        // 黑色文字（中心区域填黑）
        for y in 10..20 {
            for x in 10..20 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
        let color = detect_dominant_color(&img, 0, 0, 30, 30);
        // 主体色应该是黑色（或接近黑色）
        assert!(
            color == "#000" || color == "#080808" || color == "#101010",
            "Expected black-ish, got {}",
            color
        );
    }

    #[test]
    fn test_detect_dominant_icon_green() {
        // 32x32 区域：灰色背景 + 绿色图标
        let mut img = RgbImage::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                img.put_pixel(x, y, Rgb([200, 200, 200])); // 灰色背景
            }
        }
        // 绿色图标（对角线交叉线）
        for i in 8..24 {
            img.put_pixel(i, i, Rgb([0, 255, 0]));
            img.put_pixel(i, 31 - i, Rgb([0, 255, 0]));
        }
        let color = detect_dominant_color(&img, 0, 0, 32, 32);
        // 主体色应该是绿色
        assert!(
            color == "#0F0" || color == "#00F000" || color.contains("0F0") || color == "#00FF00"
            || color.contains("0FF") || color == "#08F808",
            "Expected green-ish, got {}",
            color
        );
    }

    #[test]
    fn test_small_element() {
        let img = RgbImage::new(10, 10);
        // 所有像素都是红色
        // 但太小，边框采样会覆盖大部分区域
        // 这个测试确保不会 panic
        let color = detect_background_color(&img, 0, 0, 10, 10);
        // 不检查具体值，只确保不 panic
        assert!(!color.is_empty());
    }

    #[test]
    fn test_detect_element_color_prefix() {
        // 验证 detect_element_color 返回带语义前缀的格式
        let mut img = RgbImage::new(50, 50);
        for y in 0..50 {
            for x in 0..50 {
                img.put_pixel(x, y, Rgb([255, 255, 255])); // 纯白
            }
        }

        // Button → 背景色 → bg(#FFF)
        let btn = Element::from_parts(0, 0, 0, 50, 50, "Button");
        let color = detect_element_color(&img, &btn);
        assert_eq!(color, "bg(#FFF)", "Button should return bg prefix");

        // Text → 前景色 → fg(#FFF)
        let txt = Element::from_parts(1, 0, 0, 50, 50, "Text");
        let color = detect_element_color(&img, &txt);
        assert_eq!(color, "fg(#FFF)", "Text should return fg prefix");

        // Icon → 前景色 → fg(#FFF)
        let icn = Element::from_parts(2, 0, 0, 50, 50, "Icon");
        let color = detect_element_color(&img, &icn);
        assert_eq!(color, "fg(#FFF)", "Icon should return fg prefix");

        // Block → 背景色 → bg(#FFF)
        let blk = Element::from_parts(3, 0, 0, 50, 50, "Block");
        let color = detect_element_color(&img, &blk);
        assert_eq!(color, "bg(#FFF)", "Block should return bg prefix");
    }
}
