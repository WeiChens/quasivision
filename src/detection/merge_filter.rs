// ── 组件合并与过滤 ──
//
// 合并相交的连通区域，按面积和宽高比过滤无效组件。

use crate::component::Component;

/// 迭代合并相交的组件（对应原版 merge_intersected_corner）
pub fn merge_intersected(
    comps: &[Component],
    merge_contained: bool,
    max_gap: (i32, i32),
) -> Vec<Component> {
    let mut changed = true;
    let mut result = comps.to_vec();

    while changed {
        changed = false;
        let mut temp: Vec<Component> = Vec::new();

        for comp_a in &result {
            let mut merged = false;
            let mut current = comp_a.clone();

            for comp_b in &mut temp {
                let rel = current.relation(comp_b, max_gap);
                // 条件：1) b 包含 a; 2) 相交; 3) (merge_contained && a 被 b 包含)
                if rel == 1 || rel == 2 || (merge_contained && rel == -1) {
                    comp_b.merge(&current);
                    current = comp_b.clone();
                    merged = true;
                    changed = true;
                    break;
                }
            }

            if !merged {
                temp.push(current);
            }
        }

        result = temp;
    }

    result
}

/// 过滤组件：按最小面积和宽高比
pub fn component_filter(
    comps: &[Component],
    min_area: i64,
    img_shape: (u32, u32),
) -> Vec<Component> {
    let max_height = img_shape.0 as f64 * 0.8;
    comps
        .iter()
        .filter(|c| {
            if c.bbox.area() < min_area {
                return false;
            }
            if c.bbox.height() as f64 > max_height {
                return false;
            }
            let ratio_h = c.bbox.width() as f64 / c.bbox.height() as f64;
            let ratio_w = c.bbox.height() as f64 / c.bbox.width() as f64;
            if ratio_h > 50.0 || ratio_w > 40.0 {
                return false;
            }
            if (c.bbox.height().min(c.bbox.width()) < 8) && ratio_h.max(ratio_w) > 10.0 {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}
