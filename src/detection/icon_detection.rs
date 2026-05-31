// ── 颜色图标检测 ──
//
// 主流程的 Sobel 梯度对同亮度但不同色的 icon 不敏感，
// 此函数直接用 RGB 颜色差异做二值化，专门捕捉颜色边缘，
// 经多轮过滤去除噪声和伪影后返回可靠的 Icon 候选。

use image::{GrayImage, RgbImage};
use rayon::prelude::*;

use crate::component::Component;
use crate::config::Config;
use crate::detection::connected_components::component_detection;

/// 图标专用检测：基于颜色差异的二值化 + 低阈值连通区域检测
///
/// 分 5 轮过滤：
/// 1. 构建颜色差异二值图
/// 2. 低阈值连通区域检测
/// 3. 保留小方形候选，排除与已有组件重叠的
/// 4. 新图标之间去重（保留面积大的）
/// 5. 圆角伪影过滤（检查对角方向是否均匀）
pub fn icon_color_detection(
    rgb: &RgbImage,
    existing: &[Component],
    config: &Config,
) -> Vec<Component> {
    let (img_h, img_w) = (rgb.height(), rgb.width());

    // ── 1. 构建颜色差异二值图（并行处理行） ──
    let mut binary = GrayImage::new(img_w, img_h);
    let w = img_w as usize;
    let buffer = binary.as_mut();
    buffer.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        if y == 0 || y >= img_h as usize - 1 {
            return;
        }
        for x in 1..img_w - 1 {
            let c = rgb.get_pixel(x as u32, y as u32);
            let mut max_diff = 0u8;
            // 展开4邻域循环（减少分支和循环开销）
            let n = rgb.get_pixel(x as u32, (y - 1) as u32);
            max_diff = max_diff.max(channel_max_diff(c, n));

            let n = rgb.get_pixel(x as u32, (y + 1) as u32);
            max_diff = max_diff.max(channel_max_diff(c, n));

            let n = rgb.get_pixel((x - 1) as u32, y as u32);
            max_diff = max_diff.max(channel_max_diff(c, n));

            let n = rgb.get_pixel((x + 1) as u32, y as u32);
            max_diff = max_diff.max(channel_max_diff(c, n));

            if max_diff > 4 {
                row[x as usize] = 255;
            }
        }
    });

    // ── 2. 连通区域检测 ──
    let mut icon_cfg = config.clone();
    icon_cfg.obj_min_area = 40;
    let (new_comps, _) = component_detection(&binary, &icon_cfg, false);

    // ── 3. 第一轮过滤 ──
    let candidates = filter_candidates(new_comps, existing, img_h as i32, img_w as i32);

    // ── 4. 第二轮去重 ──
    let final_icons = dedup_candidates(candidates);

    // ── 5. 第三轮过滤：圆角伪影 ──
    filter_corner_artifacts(final_icons, existing, rgb, img_h as i32, img_w as i32)
}

/// 计算两个像素 RGB 三个通道的最大差值
#[inline]
fn channel_max_diff(a: &image::Rgb<u8>, b: &image::Rgb<u8>) -> u8 {
    let dr = (a[0] as i16 - b[0] as i16).unsigned_abs() as u8;
    let dg = (a[1] as i16 - b[1] as i16).unsigned_abs() as u8;
    let db = (a[2] as i16 - b[2] as i16).unsigned_abs() as u8;
    dr.max(dg).max(db)
}

/// 第一轮过滤：保留小的方形候选，排除与已有组件重叠的
fn filter_candidates(
    comps: Vec<Component>,
    existing: &[Component],
    _img_h: i32,
    _img_w: i32,
) -> Vec<Component> {
    comps
        .into_iter()
        .filter(|c| {
            let cw = c.bbox.width() as f64;
            let ch = c.bbox.height() as f64;
            let ratio = cw / ch;
            (0.7..=1.4).contains(&ratio)
                && ch >= 18.0
                && ch <= 48.0
                && cw >= 18.0
                && cw <= 48.0
                && c.bbox.area() >= 324
        })
        .filter(|c| !overlaps_existing(c, existing))
        .collect()
}

/// 检查候选是否与已有组件重叠（相交/包含/重复检测/圆角伪影）
fn overlaps_existing(c: &Component, existing: &[Component]) -> bool {
    existing.iter().any(|e| {
        let rel = c.bbox.relation(&e.bbox);
        if rel == 2 || rel == 1 {
            return true;
        }
        if rel == -1 {
            // 检查 IoU > 0.5 → 重复检测
            let (c1, r1, c2, r2) = c.bbox.to_tuple();
            let (c3, r3, c4, r4) = e.bbox.to_tuple();
            let iw = (c2.min(c4) - c1.max(c3)).max(0);
            let ih = (r2.min(r4) - r1.max(r3)).max(0);
            let inter = iw as i64 * ih as i64;
            if inter > 0 {
                let union = c.bbox.area() + e.bbox.area() - inter;
                let iou = inter as f64 / union as f64;
                if iou > 0.5 {
                    return true;
                }
            }
            // 圆角伪影过滤
            if is_corner_artifact(c, e) {
                return true;
            }
        }
        false
    })
}

/// 判断候选是否位于父容器角落（圆角产生的伪影）
fn is_corner_artifact(candidate: &Component, container: &Component) -> bool {
    const CORNER_MARGIN: i32 = 6;
    let (c1, r1, c2, r2) = candidate.bbox.to_tuple();
    let (c3, r3, c4, r4) = container.bbox.to_tuple();

    let dist_left = c1 - c3;
    let dist_top = r1 - r3;
    let dist_right = c4 - c2;
    let dist_bottom = r4 - r2;

    (dist_left <= CORNER_MARGIN && dist_top <= CORNER_MARGIN)
        || (dist_right <= CORNER_MARGIN && dist_top <= CORNER_MARGIN)
        || (dist_left <= CORNER_MARGIN && dist_bottom <= CORNER_MARGIN)
        || (dist_right <= CORNER_MARGIN && dist_bottom <= CORNER_MARGIN)
}

/// 第二轮去重：候选之间互不重叠，保留面积大的
fn dedup_candidates(candidates: Vec<Component>) -> Vec<Component> {
    let mut final_icons: Vec<Component> = Vec::new();
    for cand in candidates {
        let mut dup = false;
        for existing_idx in 0..final_icons.len() {
            let rel = cand.bbox.relation(&final_icons[existing_idx].bbox);
            if rel == -1 || rel == 1 || rel == 2 {
                dup = true;
                if cand.bbox.area() > final_icons[existing_idx].bbox.area() {
                    let _ = std::mem::replace(&mut final_icons[existing_idx], cand.clone());
                }
                break;
            }
        }
        if !dup {
            final_icons.push(cand);
        }
    }
    final_icons
}

/// 第三轮过滤：对未被任何父容器包含的小图标，检查对角方向均匀性
fn filter_corner_artifacts(
    mut icons: Vec<Component>,
    existing: &[Component],
    rgb: &RgbImage,
    img_h: i32,
    img_w: i32,
) -> Vec<Component> {
    const ARTIFACT_MAX_SIZE: i32 = 22;
    const INNER_OFFSET: i32 = 8;
    const SAMPLE_SIZE: i32 = 5;
    const VARIANCE_THRESHOLD: f64 = 12.0;

    icons.retain(|icon| {
        let (x1, y1, x2, y2) = icon.bbox.to_tuple();
        let w = x2 - x1;
        let h = y2 - y1;
        if w > ARTIFACT_MAX_SIZE || h > ARTIFACT_MAX_SIZE {
            return true;
        }

        let contained = existing.iter().any(|e| icon.bbox.relation(&e.bbox) == -1);
        if contained {
            return true;
        }

        let uniform_count = count_uniform_directions(
            x1, y1, x2, y2, rgb, img_w, img_h, INNER_OFFSET, SAMPLE_SIZE, VARIANCE_THRESHOLD,
        );

        if uniform_count >= 2 {
            println!(
                "  [IconDetect] Corner artifact filtered: ({},{}) {}×{} (uniform directions: {})",
                x1,
                y1,
                x2 - x1,
                y2 - y1,
                uniform_count
            );
        }
        uniform_count < 2
    });

    icons
}

/// 从 icon 的 4 个角向对角方向采样，计算均匀方向的数量
fn count_uniform_directions(
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    rgb: &RgbImage,
    img_w: i32,
    img_h: i32,
    offset: i32,
    sample: i32,
    variance_threshold: f64,
) -> usize {
    let corners = [
        (x1 + offset, y1 + offset),
        (x2 - offset - sample, y1 + offset),
        (x1 + offset, y2 - offset - sample),
        (x2 - offset - sample, y2 - offset - sample),
    ];

    let mut uniform_count = 0;
    for &(sx, sy) in &corners {
        if sx < 0
            || sy < 0
            || (sx + sample) > img_w
            || (sy + sample) > img_h
        {
            continue;
        }

        let variance = sample_variance(rgb, sx, sy, sample);
        if variance < variance_threshold {
            uniform_count += 1;
        }
    }
    uniform_count
}

/// 计算 sample×sample 区域的灰度方差
fn sample_variance(rgb: &RgbImage, sx: i32, sy: i32, sample: i32) -> f64 {
    let mut pixels = Vec::with_capacity((sample * sample) as usize);
    for dy in 0..sample {
        for dx in 0..sample {
            let px = rgb.get_pixel((sx + dx) as u32, (sy + dy) as u32);
            let gray = 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
            pixels.push(gray);
        }
    }
    let mean = pixels.iter().sum::<f64>() / pixels.len() as f64;
    pixels.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / pixels.len() as f64
}
