// ── 线条移除 ──
//
// 从二值图中移除水平/垂直线条，避免被误检为 Button / Divider 等组件。
// 对应原版 UIED `ip_detection.py:rm_line()`。

use image::GrayImage;

/// 从二值图中移除水平/垂直线条
///
/// 原理：
/// 1. 逐行扫描，检测连续白色像素占比超过 `min_length_ratio` 的行
/// 2. 如果连续多行（厚度 < `max_thickness`）都满足条件，判定为线条
/// 3. 将线条区域从二值图中清除
pub fn remove_lines(binary: &mut GrayImage, max_thickness: u32, min_length_ratio: f64) {
    let (height, width) = (binary.height() as usize, binary.width() as usize);

    let mut start_row: i32 = -1;
    let mut end_row: i32 = -1;
    let mut check_line = false;
    let mut check_gap = false;

    for i in 0..height {
        let row_slice = &binary.as_raw()[i * width..(i + 1) * width];
        let valid = is_valid_line_row(row_slice, min_length_ratio);

        if valid {
            if !check_line {
                start_row = i as i32;
                check_line = true;
            }
        } else if check_line {
            if (i as i32) - start_row < max_thickness as i32 {
                end_row = i as i32;
                check_gap = true;
            } else {
                start_row = -1;
                end_row = -1;
            }
            check_line = false;
        }

        if check_gap && (i as i32) - end_row > max_thickness as i32 {
            clear_rows(binary, start_row, end_row, height);
            start_row = -1;
            end_row = -1;
            check_line = false;
            check_gap = false;
        }
    }

    // 处理到底部的线条
    if (check_line && height as i32 - start_row < max_thickness as i32) || check_gap {
        clear_rows(binary, start_row, end_row, height);
    }
}

/// 将指定行范围 [sr, er) 的所有像素置 0
fn clear_rows(binary: &mut GrayImage, start_row: i32, end_row: i32, height: usize) {
    let width = binary.width() as usize;
    let sr = start_row.max(0) as usize;
    let er = end_row.max(0) as usize;
    for row_idx in sr..er.min(height) {
        let row_start = row_idx * width;
        let row_end = (row_idx + 1) * width;
        for pixel in binary.as_mut()[row_start..row_end].iter_mut() {
            *pixel = 0;
        }
    }
}

/// 检查一行是否为"有效线条行"：连续白色像素占比超过阈值
#[inline]
fn is_valid_line_row(row: &[u8], min_length_ratio: f64) -> bool {
    let width = row.len();
    let mut line_length = 0usize;
    let mut line_gap = 0usize;

    for &p in row {
        if p > 0 {
            if line_gap > 5 {
                return false;
            }
            line_length += 1;
            line_gap = 0;
        } else if line_length > 0 {
            line_gap += 1;
        }
    }

    line_length as f64 / width as f64 >= min_length_ratio
}
