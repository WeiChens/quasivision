// ── 几何规则分类 ──
//
// 基于几何特征对组件进行规则分类，替代 CNN 分类器。
// 规则按优先级排序，匹配即返回。

use crate::component::Component;

/// 基于几何特征对组件进行规则分类
///
/// 规则（按优先级）：
/// - Block: 已由 block_recognition 标记的**中空容器**
/// - Image: 面积占比大（>8%）且宽高比不太极端
/// - Block: **Section 容器**（宽度 >50% 图片宽度，或高度 >50% 图片高度）
/// - Icon:  小尺寸方形元素（≤48px，宽高比接近1:1）
/// - Button: 面积适中，宽高比在 0.5~3.0 之间
/// - Text: 高度很小且很宽
/// - Compo: 默认为通用组件
pub fn classify_by_geometry(comps: &mut [Component], img_shape: (u32, u32)) {
    let (img_h, img_w) = (img_shape.0 as f64, img_shape.1 as f64);
    let img_area = img_h * img_w;

    for comp in comps.iter_mut() {
        // 跳过已经分类的（如 Block）
        if comp.category != "Compo" {
            continue;
        }

        let w = comp.bbox.width() as f64;
        let h = comp.bbox.height() as f64;
        let area = comp.bbox.area() as f64;
        let ratio = w / h;
        let area_ratio = area / img_area;

        // Image: 面积大，宽高比不太极端
        if area_ratio > 0.08 && (0.3..=5.0).contains(&ratio) {
            comp.category = "Image".to_string();
            continue;
        }

        // Section Block: 占满屏幕宽度或高度的分栏/容器
        if (w / img_w > 0.5 && h / img_h > 0.03)
            || (h / img_h > 0.5 && w / img_w > 0.03)
        {
            comp.category = "Block".to_string();
            continue;
        }

        // Icon: 小尺寸方形元素
        if (0.7..=1.4).contains(&ratio) && h <= 48.0 && w <= 48.0 {
            comp.category = "Icon".to_string();
            continue;
        }

        // Button: 面积适中，宽高比接近 1:1 ~ 3:1
        if (0.5..=3.0).contains(&ratio)
            && (20.0..=80.0).contains(&h)
            && (20.0..=200.0).contains(&w)
            && area / (w * h) > 0.8
        {
            comp.category = "Button".to_string();
            continue;
        }

        // Text bar: 高度很小，宽度很大
        if h / img_h < 0.025 && ratio > 3.0 {
            comp.category = "Text".to_string();
            continue;
        }

        // 其余保持 "Compo"
    }
}
