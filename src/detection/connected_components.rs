// ── 连通区域检测 ──
//
// 在二值图上执行 CCL（Connected Component Labeling），
// 返回 Component 列表，每个 Component 包含区域像素和边界信息。

use image::{GrayImage, Luma};
use imageproc::region_labelling::connected_components;
use imageproc::region_labelling::Connectivity;

use crate::component::Component;
use crate::config::Config;

/// 在二值图上进行连通区域检测，返回 Component 列表
///
/// 模拟原版 Python 的 component_detection() 函数。
///
/// - `rect_detect=true`：区分矩形/非矩形，返回 (comps_rect, comps_nonrect)
/// - `rect_detect=false`：所有组件混在一起返回
pub fn component_detection(
    binary: &GrayImage,
    config: &Config,
    rect_detect: bool,
) -> (Vec<Component>, Vec<Component>) {
    let (rows, cols) = (binary.height() as u32, binary.width() as u32);

    // 使用 connected_components 标记连通区域
    let labels = connected_components(binary, Connectivity::Eight, Luma([0u8]));

    // 收集每个标签对应的像素
    let mut label_pixels: Vec<Vec<(u32, u32)>> = Vec::new();
    label_pixels.push(vec![]); // label 0 是背景，占位

    for y in 0..rows {
        for x in 0..cols {
            let label = labels.get_pixel(x, y)[0];
            if label > 0 {
                let idx = label as usize;
                if idx >= label_pixels.len() {
                    label_pixels.push(vec![]);
                }
                label_pixels[idx].push((y, x)); // (row, col)
            }
        }
    }

    let mut comps_all = Vec::new();
    let mut comps_rect = Vec::new();
    let mut comps_nonrect = Vec::new();

    for (label, pixels) in label_pixels.iter().enumerate() {
        if label == 0 || pixels.len() < config.obj_min_area as usize {
            continue;
        }

        let mut comp = Component::new(pixels.clone());

        if comp.bbox.width() <= 3 || comp.bbox.height() <= 3 {
            continue;
        }

        if rect_detect {
            if comp.is_rectangle(
                config.rec_max_dent_ratio,
                config.rec_min_evenness,
                config.rec_corner_skip_ratio,
            ) {
                comps_rect.push(comp);
            } else {
                comps_nonrect.push(comp);
            }
        } else {
            comps_all.push(comp);
        }
    }

    if rect_detect {
        (comps_rect, comps_nonrect)
    } else {
        (comps_all, vec![])
    }
}
