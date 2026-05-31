// ── Block（容器区块）检测 ──
//
// 识别 UI 中的容器型元素：
// - is_block(): 判断裁剪区域是否为内部中空的矩形容器
// - block_recognition(): 在大组件中标记 Block
// - nested_components_detection(): 在灰度图上用 BFS flood fill 找大区块

use image::{GenericImageView, GrayImage};

use crate::component::Component;
use crate::config::Config;

/// 判断一个裁剪区域是否为 Block（内部中空的矩形容器）
pub fn is_block(clip: &GrayImage, threshold: f64) -> bool {
    let (h, w) = (clip.height() as usize, clip.width() as usize);
    let side = 4;

    // 上边 - 向内扫描
    if has_dense_border_rows(clip, 0, side, h, w, threshold) {
        return false;
    }
    // 左边
    if has_dense_border_cols(clip, 0, side, w, h, threshold) {
        return false;
    }
    // 下边 - 从底部向内扫描
    if h >= side + 1
        && has_dense_border_rows(clip, h - side, h, h, w, threshold)
    {
        return false;
    }
    // 右边
    if w >= side + 1
        && has_dense_border_cols(clip, w - side, w, w, h, threshold)
    {
        return false;
    }

    true
}

/// 检查一组水平行中是否有密集的非空白像素（> threshold）
fn has_dense_border_rows(
    clip: &GrayImage,
    start: usize,
    end: usize,
    img_h: usize,
    img_w: usize,
    threshold: f64,
) -> bool {
    let mut blank_count = 0;
    for i in start..end {
        if i >= img_h {
            break;
        }
        let row_sum: u32 = clip
            .view(0, i as u32, img_w as u32, 1)
            .to_image()
            .pixels()
            .map(|p| p[0] as u32)
            .sum();
        if row_sum as f64 / 255.0 > threshold * img_w as f64 {
            blank_count += 1;
        }
    }
    blank_count > 2
}

/// 检查一组垂直列中是否有密集的非空白像素
fn has_dense_border_cols(
    clip: &GrayImage,
    start: usize,
    end: usize,
    img_w: usize,
    img_h: usize,
    threshold: f64,
) -> bool {
    let mut blank_count = 0;
    for i in start..end {
        if i >= img_w {
            break;
        }
        let col_sum: u32 = clip
            .view(i as u32, 0, 1, img_h as u32)
            .to_image()
            .pixels()
            .map(|p| p[0] as u32)
            .sum();
        if col_sum as f64 / 255.0 > threshold * img_h as f64 {
            blank_count += 1;
        }
    }
    blank_count > 2
}

/// 区块识别
///
/// 判断组件是否为容器型区块（内部中空的矩形）。
/// 条件（任一满足即可）：
/// - 高度占比 > block_side_length（宽区块，如横条容器）
/// - 宽度占比 > block_side_length（高区块，如侧边栏）
/// - 面积占比 > block_side_length（大方块）
/// 额外要求：面积占比至少 0.5%，避免细长条噪声误检
pub fn block_recognition(
    binary: &GrayImage,
    comps: &mut [Component],
    block_side_length: f64,
) {
    let (h, w) = (binary.height() as f64, binary.width() as f64);
    let img_area = h * w;
    for comp in comps.iter_mut() {
        let height_ratio = comp.bbox.height() as f64 / h;
        let width_ratio = comp.bbox.width() as f64 / w;
        let area_ratio = comp.bbox.area() as f64 / img_area;

        let is_large_enough = (height_ratio > block_side_length
            || width_ratio > block_side_length
            || area_ratio > block_side_length)
            && area_ratio > 0.005;

        if is_large_enough {
            let clip = comp.clipping(binary, 0);
            if is_block(&clip, 0.15) {
                comp.category = "Block".to_string();
            }
        }
    }
}

/// 嵌套组件检测（在灰度图上用 BFS flood fill 找大区块）
///
/// 以步长 step_h=5, step_v=2 稀疏扫描，跳过大部分像素。
/// BFS flood fill 只从高梯度种子点扩散。
pub fn nested_components_detection(
    gray: &GrayImage,
    config: &Config,
    grad_thresh: u8,
) -> Vec<Component> {
    let (rows, cols) = (gray.height() as u32, gray.width() as u32);
    let mut mask = vec![0u8; ((rows + 2) * (cols + 2)) as usize];
    let step_h = 5usize;
    let step_v = 2usize;

    let grad = crate::preprocess::gray_to_gradient(gray);

    let mut comps = Vec::new();

    for y in (0..rows).step_by(step_h) {
        for x in (0..cols).step_by(step_v) {
            let idx = ((y + 1) * (cols + 2) + (x + 1)) as usize;
            if mask[idx] == 0 && grad.get_pixel(x as u32, y as u32)[0] >= grad_thresh {
                // Flood fill using BFS
                let mut pixels = Vec::new();
                let mut queue = Vec::new();
                queue.push((y, x));

                while let Some((cy, cx)) = queue.pop() {
                    let idx2 = ((cy + 1) * (cols + 2) + (cx + 1)) as usize;
                    if mask[idx2] != 0 {
                        continue;
                    }
                    mask[idx2] = 1;

                    if grad.get_pixel(cx as u32, cy as u32)[0] < grad_thresh {
                        continue;
                    }

                    pixels.push((cy, cx));

                    // 4-connected neighbors
                    if cx > 0 {
                        queue.push((cy, cx - 1));
                    }
                    if cx + 1 < cols {
                        queue.push((cy, cx + 1));
                    }
                    if cy > 0 {
                        queue.push((cy - 1, cx));
                    }
                    if cy + 1 < rows {
                        queue.push((cy + 1, cx));
                    }
                }

                if pixels.len() < 500 {
                    continue;
                }

                let mut comp = Component::new(pixels);

                if comp.bbox.height() < 30 {
                    continue;
                }
                let img_area = rows as i64 * cols as i64;
                if comp.bbox.area() as f64 / img_area as f64 > 0.9 {
                    continue;
                }
                if comp.bbox.area() as f64 / img_area as f64 > 0.7 {
                    comp.redundant = true;
                }

                if comp.is_line(config.line_thickness) {
                    continue;
                }
                if !comp.is_rectangle(
                    config.rec_max_dent_ratio,
                    config.rec_min_evenness,
                    config.rec_corner_skip_ratio,
                ) {
                    continue;
                }

                if comp.bbox.area() < 5000
                    || comp.bbox.height() < 20
                    || comp.bbox.width() < 20
                {
                    continue;
                }

                comps.push(comp);
            }
        }
    }

    comps
}
