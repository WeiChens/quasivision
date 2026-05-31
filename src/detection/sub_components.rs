// ── 子组件检测 ──
//
// 在 Image 类型组件内部检测子组件（如图片内部的按钮）。
// 通过裁剪 + 反转二值图 + 重新做连通区域检测实现。

use image::GrayImage;

use crate::component::Component;
use crate::config::Config;
use crate::detection::connected_components::component_detection;
use crate::preprocess;

/// 在组件内部检测子组件（如图片内部的按钮）
pub fn detect_sub_components(
    comps: &[Component],
    binary: &GrayImage,
    config: &Config,
) -> Vec<Component> {
    let mut sub_comps = Vec::new();

    for comp in comps {
        if comp.category != "Image" {
            continue;
        }

        let clip = comp.clipping(binary, 0);
        let clip_rev = preprocess::reverse_binary(&clip);

        let (sub_rect, _) = component_detection(&clip_rev, config, true);

        for mut sub in sub_rect {
            sub.to_relative_position(comp.bbox.col_min, comp.bbox.row_min);
            if sub.bbox.area() < comp.bbox.area() / 2
                && sub.bbox.height() > 15
                && sub.bbox.width() > 15
            {
                sub_comps.push(sub);
            }
        }
    }

    sub_comps
}
