use image::{DynamicImage, GrayImage};
use imageproc::morphology;
use imageproc::contrast;
use imageproc::contrast::ThresholdType;
use imageproc::distance_transform::Norm;
use rayon::prelude::*;

/// 读取图像，可选 resize 和模糊
pub fn read_image(path: &str, resize_height: Option<u32>, kernel_size: Option<u32>) -> anyhow::Result<(DynamicImage, GrayImage)> {
    let img = image::open(path)
        .map_err(|e| anyhow::anyhow!("Failed to open image '{}': {}", path, e))?;

    let img = if let Some(h) = resize_height {
        let w = (h as f64 * img.width() as f64 / img.height() as f64) as u32;
        img.resize_exact(w, h, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let img = if let Some(ks) = kernel_size {
        if ks > 0 {
            let blurred = image::imageops::blur(&img, ks as f32);
            DynamicImage::ImageRgba8(blurred)
        } else {
            img
        }
    } else {
        img
    };

    let gray = img.to_luma8();
    Ok((img, gray))
}

/// 从内存字节读取图片
pub fn read_image_from_bytes(data: &[u8]) -> anyhow::Result<(DynamicImage, GrayImage)> {
    let img = image::load_from_memory(data)
        .map_err(|e| anyhow::anyhow!("Failed to decode image from bytes: {e}"))?;
    let gray = img.to_luma8();
    Ok((img, gray))
}

/// 从灰度图计算梯度图（标准 Sobel 梯度幅值，并行处理行）
pub fn gray_to_gradient(gray: &GrayImage) -> GrayImage {
    let (w, h) = (gray.width() as i32, gray.height() as i32);
    let mut grad = GrayImage::new(w as u32, h as u32);
    let w_usize = w as usize;

    // 标准 Sobel 水平梯度核 (检测垂直边缘)
    let kernel_h: [[f32; 3]; 3] = [[-1.0, 0.0, 1.0], [-2.0, 0.0, 2.0], [-1.0, 0.0, 1.0]];
    // 标准 Sobel 垂直梯度核 (检测水平边缘)
    let kernel_v: [[f32; 3]; 3] = [[-1.0, -2.0, -1.0], [0.0, 0.0, 0.0], [1.0, 2.0, 1.0]];

    // 并行处理每行（每行像素间独立）
    let buffer = grad.as_mut();
    let h_i32 = h;
    buffer.par_chunks_mut(w_usize).enumerate().for_each(|(y_usize, row)| {
        let y = y_usize as i32;
        if y <= 0 || y >= h_i32 - 1 {
            return; // 边缘行保持 0
        }
        for x in 1..w - 1 {
            let mut gx = 0.0f32;
            let mut gy = 0.0f32;
            for ky in 0..3 {
                for kx in 0..3 {
                    let px = gray.get_pixel((x + kx - 1) as u32, (y + ky - 1) as u32)[0] as f32;
                    gx += px * kernel_h[ky as usize][kx as usize];
                    gy += px * kernel_v[ky as usize][kx as usize];
                }
            }
            row[x as usize] = (gx.abs() + gy.abs()) as u8;
        }
    });

    grad
}

/// 从 RGB 图计算颜色感知梯度（结合亮度 Sobel + 颜色差异，并行处理行）
///
/// 标准 Sobel 只检测亮度变化，会漏掉同亮度但不同颜色的边缘（如彩色 icon 在白色背景上）。
/// 此函数在 Sobel 基础上，叠加 RGB 通道的最大颜色差异，捕捉颜色边缘。
pub fn color_aware_gradient(img: &DynamicImage) -> GrayImage {
    let gray = img.to_luma8();
    let rgb = img.to_rgb8();
    let (w, h) = (gray.width() as i32, gray.height() as i32);
    let w_usize = w as usize;

    // 先算灰度 Sobel
    let sobel = gray_to_gradient(&gray);

    // 并行处理每行
    let mut grad = GrayImage::new(w as u32, h as u32);
    let buffer = grad.as_mut();
    buffer.par_chunks_mut(w_usize).enumerate().for_each(|(y, row)| {
        if y == 0 || y >= h as usize - 1 {
            return;
        }
        for x in 1..w - 1 {
            let center = rgb.get_pixel(x as u32, y as u32);
            // 计算与4邻域的最大颜色差异
            let mut max_color_diff = 0u8;
            let dr0 = (center[0] as i16 - rgb.get_pixel(x as u32, (y - 1) as u32)[0] as i16).unsigned_abs() as u8;
            let dg0 = (center[1] as i16 - rgb.get_pixel(x as u32, (y - 1) as u32)[1] as i16).unsigned_abs() as u8;
            let db0 = (center[2] as i16 - rgb.get_pixel(x as u32, (y - 1) as u32)[2] as i16).unsigned_abs() as u8;
            max_color_diff = max_color_diff.max(dr0.max(dg0).max(db0));

            let dr1 = (center[0] as i16 - rgb.get_pixel(x as u32, (y + 1) as u32)[0] as i16).unsigned_abs() as u8;
            let dg1 = (center[1] as i16 - rgb.get_pixel(x as u32, (y + 1) as u32)[1] as i16).unsigned_abs() as u8;
            let db1 = (center[2] as i16 - rgb.get_pixel(x as u32, (y + 1) as u32)[2] as i16).unsigned_abs() as u8;
            max_color_diff = max_color_diff.max(dr1.max(dg1).max(db1));

            let dr2 = (center[0] as i16 - rgb.get_pixel((x - 1) as u32, y as u32)[0] as i16).unsigned_abs() as u8;
            let dg2 = (center[1] as i16 - rgb.get_pixel((x - 1) as u32, y as u32)[1] as i16).unsigned_abs() as u8;
            let db2 = (center[2] as i16 - rgb.get_pixel((x - 1) as u32, y as u32)[2] as i16).unsigned_abs() as u8;
            max_color_diff = max_color_diff.max(dr2.max(dg2).max(db2));

            let dr3 = (center[0] as i16 - rgb.get_pixel((x + 1) as u32, y as u32)[0] as i16).unsigned_abs() as u8;
            let dg3 = (center[1] as i16 - rgb.get_pixel((x + 1) as u32, y as u32)[1] as i16).unsigned_abs() as u8;
            let db3 = (center[2] as i16 - rgb.get_pixel((x + 1) as u32, y as u32)[2] as i16).unsigned_abs() as u8;
            max_color_diff = max_color_diff.max(dr3.max(dg3).max(db3));

            // 取 Sobel 亮度梯度和颜色差异梯度的最大值
            let sobel_val = sobel.get_pixel(x as u32, y as u32)[0];
            row[x as usize] = sobel_val.max(max_color_diff);
        }
    });

    grad
}

/// 对梯度图进行二值化 + 形态学闭运算
///
/// 使用 color_aware_gradient 替代纯 Sobel，可检测颜色边缘（同亮度不同色）。
pub fn binarization_color(img: &DynamicImage, grad_min: u8) -> GrayImage {
    let grad = color_aware_gradient(img);
    let binary = contrast::threshold(&grad, grad_min, ThresholdType::Binary);
    morphology::close(&binary, Norm::L1, 1)
}

/// 反转二值图
pub fn reverse_binary(bin: &GrayImage) -> GrayImage {
    let mut out = bin.clone();
    for pixel in out.pixels_mut() {
        pixel[0] = 255 - pixel[0];
    }
    out
}
