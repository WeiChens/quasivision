// ── 检测模块 ──
//
// 拆分自原 detection.rs（791 行），按职责分为 7 个子模块：
//
//   connected_components.rs   组件检测（CCL 连通区域）
//   line_removal.rs           线条移除
//   merge_filter.rs           合并 + 过滤
//   block.rs                  Block 识别 + 嵌套检测
//   classification.rs         几何规则分类
//   icon_detection.rs         颜色图标检测
//   sub_components.rs         图片内部子组件检测

pub mod block;
pub mod classification;
pub mod connected_components;
pub mod icon_detection;
pub mod line_removal;
pub mod merge_filter;
pub mod sub_components;

pub use block::{block_recognition, is_block, nested_components_detection};
pub use classification::classify_by_geometry;
pub use connected_components::component_detection;
pub use icon_detection::icon_color_detection;
pub use line_removal::remove_lines;
pub use merge_filter::{component_filter, merge_intersected};
pub use sub_components::detect_sub_components;

#[cfg(test)]
mod tests {
    use image::GrayImage;

    use crate::component::Component;
    use crate::config::Config;

    use super::classification::classify_by_geometry;
    use super::connected_components::component_detection;

    #[test]
    fn test_component_detection_empty() {
        let binary = GrayImage::new(100, 100);
        let config = Config::default();
        let (rect, _) = component_detection(&binary, &config, true);
        assert!(rect.is_empty());
    }

    #[test]
    fn test_classify_icon_small_square() {
        let region = (0..24).flat_map(|y| (0..24).map(move |x| (y, x))).collect();
        let mut comp = Component::new(region);
        comp.category = "Compo".to_string();

        let mut comps = vec![comp];
        classify_by_geometry(&mut comps, (100, 100));

        assert_eq!(comps[0].category, "Icon");
    }

    #[test]
    fn test_classify_icon_rectangular() {
        let mut region = Vec::new();
        for y in 0..20 {
            for x in 0..30 {
                region.push((y, x));
            }
        }
        let mut comp = Component::new(region);
        comp.category = "Compo".to_string();

        let mut comps = vec![comp];
        classify_by_geometry(&mut comps, (100, 100));

        assert_eq!(comps[0].category, "Button");
    }

    #[test]
    fn test_classify_icon_too_large() {
        let region = (0..40).flat_map(|y| (0..40).map(move |x| (y, x))).collect();
        let mut comp = Component::new(region);
        comp.category = "Compo".to_string();

        let mut comps = vec![comp];
        classify_by_geometry(&mut comps, (100, 100));

        assert_eq!(comps[0].category, "Image");
    }

    #[test]
    fn test_classify_icon_vs_button() {
        let region = (0..30).flat_map(|y| (0..30).map(move |x| (y, x))).collect();
        let mut comp = Component::new(region);
        comp.category = "Compo".to_string();

        let mut comps = vec![comp];
        classify_by_geometry(&mut comps, (1000, 1000));

        assert_eq!(comps[0].category, "Icon");
    }
}
