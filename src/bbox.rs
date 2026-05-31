use serde::{Deserialize, Serialize};

/// 边界框：列/行最小~最大坐标
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Bbox {
    pub col_min: i32,
    pub row_min: i32,
    pub col_max: i32,
    pub row_max: i32,
}

impl Bbox {
    pub fn new(col_min: i32, row_min: i32, col_max: i32, row_max: i32) -> Self {
        let width = (col_max - col_min).max(0);
        let height = (row_max - row_min).max(0);
        Self {
            col_min,
            row_min,
            col_max: col_min + width,
            row_max: row_min + height,
        }
    }

    pub fn width(&self) -> i32 {
        self.col_max - self.col_min
    }

    pub fn height(&self) -> i32 {
        self.row_max - self.row_min
    }

    pub fn area(&self) -> i64 {
        self.width() as i64 * self.height() as i64
    }

    pub fn to_tuple(&self) -> (i32, i32, i32, i32) {
        (self.col_min, self.row_min, self.col_max, self.row_max)
    }

    /// 判断两个 Bbox 的关系:
    /// -1: a 包含在 b 中 (a in b)
    ///  0: 不相交
    ///  1: b 包含在 a 中 (b in a)
    ///  2: 相交（有重叠区域）
    pub fn relation(&self, other: &Bbox) -> i32 {
        let (c1, r1, c2, r2) = self.to_tuple();
        let (c3, r3, c4, r4) = other.to_tuple();

        // a in b
        if c1 >= c3 && r1 >= r3 && c2 <= c4 && r2 <= r4 {
            return -1;
        }
        // b in a
        if c1 <= c3 && r1 <= r3 && c2 >= c4 && r2 >= r4 {
            return 1;
        }
        // 不相交
        if c2 <= c3 || c1 >= c4 || r2 <= r3 || r1 >= r4 {
            return 0;
        }
        // 相交
        2
    }

    /// 带偏置的 IoU 关系计算
    pub fn relation_with_bias(&self, other: &Bbox, bias: (i32, i32)) -> i32 {
        let (bias_col, bias_row) = bias;

        let inter_col_min = self.col_min.max(other.col_min) - bias_col;
        let inter_row_min = self.row_min.max(other.row_min) - bias_row;
        let inter_col_max = self.col_max.min(other.col_max);
        let inter_row_max = self.row_max.min(other.row_max);

        let w = (inter_col_max - inter_col_min).max(0);
        let h = (inter_row_max - inter_row_min).max(0);
        let inter = w as i64 * h as i64;

        if inter == 0 {
            return 0;
        }

        let area_a = self.area();
        let area_b = other.area();
        let union = area_a + area_b - inter;

        let iou = if union > 0 { inter as f64 / union as f64 } else { 0.0 };
        let ioa = inter as f64 / area_a as f64;
        let iob = inter as f64 / area_b as f64;

        // a in b (a contained in b)
        if ioa >= 1.0 {
            return -1;
        }
        // b in a
        if iob >= 1.0 {
            return 1;
        }
        // intersecting
        if iou >= 0.02 || iob > 0.2 || ioa > 0.2 {
            return 2;
        }
        0
    }

    /// 合并两个相交的 bbox
    pub fn merge(&self, other: &Bbox) -> Bbox {
        Bbox::new(
            self.col_min.min(other.col_min),
            self.row_min.min(other.row_min),
            self.col_max.max(other.col_max),
            self.row_max.max(other.row_max),
        )
    }

    /// 在图像边界内添加 padding
    pub fn padding(&self, img_h: i32, img_w: i32, pad: i32) -> Bbox {
        Bbox::new(
            (self.col_min - pad).max(0),
            (self.row_min - pad).max(0),
            (self.col_max + pad).min(img_w),
            (self.row_max + pad).min(img_h),
        )
    }

    /// 转换为相对位置（基于 base 坐标偏移）
    pub fn to_relative(&mut self, col_min_base: i32, row_min_base: i32) {
        self.col_min += col_min_base;
        self.col_max += col_min_base;
        self.row_min += row_min_base;
        self.row_max += row_min_base;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_contained() {
        let a = Bbox::new(10, 10, 50, 50);
        let b = Bbox::new(0, 0, 100, 100);
        assert_eq!(a.relation(&b), -1);
        assert_eq!(b.relation(&a), 1);
    }

    #[test]
    fn test_relation_disjoint() {
        let a = Bbox::new(0, 0, 10, 10);
        let b = Bbox::new(20, 20, 30, 30);
        assert_eq!(a.relation(&b), 0);
    }

    #[test]
    fn test_relation_intersect() {
        let a = Bbox::new(0, 0, 20, 20);
        let b = Bbox::new(10, 10, 30, 30);
        assert_eq!(a.relation(&b), 2);
    }

    #[test]
    fn test_merge() {
        let a = Bbox::new(0, 0, 10, 10);
        let b = Bbox::new(5, 5, 15, 15);
        let m = a.merge(&b);
        assert_eq!(m.to_tuple(), (0, 0, 15, 15));
    }
}
