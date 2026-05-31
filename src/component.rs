use std::collections::HashMap;

use image::GenericImageView;

use crate::bbox::Bbox;

/// 组件：由一个连通区域构成
#[derive(Debug, Clone)]
pub struct Component {
    /// 区域像素点集合 (row, col)
    pub region: Vec<(u32, u32)>,
    /// 边界： [上, 下, 左, 右] 各方向每行/列的边界点
    /// boundary[0] = 上边界: Vec<(col, row_min_at_that_col)>
    /// boundary[1] = 下边界: Vec<(col, row_max_at_that_col)>
    /// boundary[2] = 左边界: Vec<(row, col_min_at_that_row)>
    /// boundary[3] = 右边界: Vec<(row, col_max_at_that_row)>
    pub boundary: Vec<Vec<(u32, u32)>>,
    pub bbox: Bbox,

    pub category: String,
    pub rect: Option<bool>,
    pub line: Option<bool>,
    pub redundant: bool,
}

impl Component {
    pub fn new(region: Vec<(u32, u32)>) -> Self {
        let bbox = Self::compute_bbox(&region);
        let boundary = Self::compute_boundary(&region);
        Self {
            region,
            boundary,
            bbox,
            category: "Compo".to_string(),
            rect: None,
            line: None,
            redundant: false,
        }
    }

    /// 从区域像素计算最小边界框
    fn compute_bbox(region: &[(u32, u32)]) -> Bbox {
        let mut col_min = u32::MAX;
        let mut row_min = u32::MAX;
        let mut col_max = u32::MIN;
        let mut row_max = u32::MIN;

        for &(r, c) in region {
            if c < col_min { col_min = c; }
            if r < row_min { row_min = r; }
            if c > col_max { col_max = c; }
            if r > row_max { row_max = r; }
        }

        Bbox::new(col_min as i32, row_min as i32, col_max as i32 + 1, row_max as i32 + 1)
    }

    /// 计算边界（与原始 Python 逻辑一致）
    /// 返回 [top, bottom, left, right]
    fn compute_boundary(region: &[(u32, u32)]) -> Vec<Vec<(u32, u32)>> {
        let mut top: HashMap<u32, u32> = HashMap::new();    // col -> min row
        let mut bottom: HashMap<u32, u32> = HashMap::new(); // col -> max row
        let mut left: HashMap<u32, u32> = HashMap::new();   // row -> min col
        let mut right: HashMap<u32, u32> = HashMap::new();  // row -> max col

        for &(r, c) in region {
            top.entry(c)
                .and_modify(|v| *v = (*v).min(r))
                .or_insert(r);
            bottom.entry(c)
                .and_modify(|v| *v = (*v).max(r))
                .or_insert(r);
            left.entry(r)
                .and_modify(|v| *v = (*v).min(c))
                .or_insert(c);
            right.entry(r)
                .and_modify(|v| *v = (*v).max(c))
                .or_insert(c);
        }

        // 按 key 排序并转为 Vec
        let sort_by_key = |map: HashMap<u32, u32>| -> Vec<(u32, u32)> {
            let mut v: Vec<_> = map.into_iter().collect();
            v.sort_by_key(|(k, _)| *k);
            v
        };

        vec![
            sort_by_key(top),
            sort_by_key(bottom),
            sort_by_key(left),
            sort_by_key(right),
        ]
    }

    /// 检查是否为矩形（支持圆角矩形）
    ///
    /// 综合判定标准（与原版 Python 一致，额外增加了圆角容错）：
    /// - `max_dent_ratio`: 凹陷像素占比上限
    /// - `min_evenness`: 平坦像素占比下限（`flat / param`）
    /// - `corner_skip_ratio`: 圆角容错，每边两端忽略的比例（默认 0.08）
    ///   圆角弯曲只出现在角部，忽略首尾各 N% 即可排除圆角干扰。
    ///   设为 0.0 则严格检测直角矩形。
    pub fn is_rectangle(&mut self, max_dent_ratio: f64, min_evenness: f64, corner_skip_ratio: f64) -> bool {
        let dent_direction = [1, -1, 1, -1];

        let mut total_flat: usize = 0;
        let mut total_param: usize = 0;

        for (n, border) in self.boundary.iter().enumerate() {
            if border.len() < 4 {
                self.rect = Some(false);
                return false;
            }

            let adj_side = if n <= 1 {
                self.boundary[2].len().max(self.boundary[3].len())
            } else {
                self.boundary[0].len().max(self.boundary[1].len())
            };

            let mut pit = 0;
            let mut depth: i64 = 0;
            let mut abnm = 0;
            let mut flat = 0usize;

            // 跳过首尾的圆角弯曲区域，只检查中间的平直段
            // 圆角弯曲只出现在角部，忽略首尾各 corner_skip_ratio 即可
            let skip = (border.len() as f64 * corner_skip_ratio) as usize;
            let start_idx = skip.min(border.len() / 4);   // 最多跳过 25%
            let end_idx = (border.len() - 1).saturating_sub(skip.min(border.len() / 4));

            let inspected = if end_idx > start_idx { end_idx - start_idx } else { 0 };

            for i in start_idx..end_idx {
                let diff = border[i + 1].1 as i64 - border[i].1 as i64;
                depth += diff;

                // 忽略起始段的噪声
                let ratio_i = (i as f64) / (border.len() as f64);
                let dent_val = ((dent_direction[n] as i64 * diff).unsigned_abs() as f64) / (adj_side as f64);
                if ratio_i < 0.08 && dent_val > 0.5 {
                    depth = 0;
                }

                // 如果表面变化过大，计入异常
                if (depth as f64).abs() / adj_side as f64 > 0.3 {
                    abnm += 1;
                    if abnm as f64 / border.len() as f64 > 0.1 {
                        self.rect = Some(false);
                        return false;
                    }
                    continue;
                } else {
                    abnm = 0;
                }

                // 凹陷检测
                if dent_direction[n] as i64 * diff < 0 && (depth as f64).abs() / adj_side as f64 > 0.15 {
                    pit += 1;
                    continue;
                }

                // 表面变化小 → 计为平整
                if (depth as f64).abs() < 1.0 + adj_side as f64 * 0.015 {
                    flat += 1;
                }
            }

            total_flat += flat;
            total_param += inspected;

            // 凹陷比例检查（用全长做分母更宽松，圆角被跳过不计入后更容易通过）
            if pit as f64 / border.len() as f64 > max_dent_ratio {
                self.rect = Some(false);
                return false;
            }
        }

        // 平整度检查
        let evenness = if total_param > 0 {
            total_flat as f64 / total_param as f64
        } else {
            0.0
        };
        if evenness < min_evenness {
            self.rect = Some(false);
            return false;
        }

        self.rect = Some(true);
        true
    }

    /// 检测是否为线条
    pub fn is_line(&mut self, min_line_thickness: u32) -> bool {
        // 水平线：上下边界差值很小
        if !self.boundary[0].is_empty() && !self.boundary[1].is_empty() {
            let mut slim = 0;
            let n = self.boundary[0].len().min(self.boundary[1].len());
            for i in 0..n {
                let diff = (self.boundary[1][i].1 as i32 - self.boundary[0][i].1 as i32).unsigned_abs();
                if diff <= min_line_thickness {
                    slim += 1;
                }
            }
            if n > 0 && slim as f64 / n as f64 > 0.93 {
                self.line = Some(true);
                return true;
            }
        }

        // 垂直线：左右边界差值很小
        if !self.boundary[2].is_empty() && !self.boundary[3].is_empty() {
            let mut slim = 0;
            let n = self.boundary[2].len().min(self.boundary[3].len());
            for i in 0..n {
                let diff = (self.boundary[3][i].1 as i32 - self.boundary[2][i].1 as i32).unsigned_abs();
                if diff <= min_line_thickness {
                    slim += 1;
                }
            }
            if n > 0 && slim as f64 / n as f64 > 0.93 {
                self.line = Some(true);
                return true;
            }
        }

        self.line = Some(false);
        false
    }

    /// 从原图裁剪该组件区域
    pub fn clipping(&self, img: &image::GrayImage, pad: i32) -> image::GrayImage {
        let (h, w) = (img.height() as i32, img.width() as i32);
        let padded = self.bbox.padding(h, w, pad);
        let sub = img.view(
            padded.col_min as u32,
            padded.row_min as u32,
            (padded.width()) as u32,
            (padded.height()) as u32,
        );
        sub.to_image()
    }

    /// 转换为相对位置（在父组件中的偏移）
    pub fn to_relative_position(&mut self, col_min_base: i32, row_min_base: i32) {
        self.bbox.to_relative(col_min_base, row_min_base);
    }

    /// 合并另一个组件
    pub fn merge(&mut self, other: &Component) {
        self.bbox = self.bbox.merge(&other.bbox);
        // 重新合并区域
        let mut all_region = self.region.clone();
        all_region.extend_from_slice(&other.region);
        self.region = all_region;
        self.boundary = Self::compute_boundary(&self.region);
    }

    /// 判断与另一个组件的关系
    pub fn relation(&self, other: &Component, bias: (i32, i32)) -> i32 {
        self.bbox.relation_with_bias(&other.bbox, bias)
    }
}
