// ── Icon 含义识别 ──

use std::path::Path;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use ort::value::Value;

use crate::element::Element;

const IMG_SIZE: u32 = 96;
const MODEL_SUBDIR: &str = "icon-classifier";
const MODEL_FILENAME: &str = "icon_classifier.onnx";
const LABELS_FILENAME: &str = "labels.json";

/// 模型目录结构指引（用于错误提示）
const MODEL_DIR_HELP: &str = "\n\
Expected directory structure:\n\
  {models_dir}/icon-classifier/\n\
    icon_classifier.onnx\n\
    labels.json\n\
Download the model files and place them in the correct directory.";

/// Icon 分类器
pub struct IconClassifier {
    session: Session,
    labels: Vec<String>,
}

impl IconClassifier {
    /// 创建并初始化 Icon 分类器
    ///
    /// `models_root`：模型根目录（通常是 `resources/`），
    /// 方法会自动在其下查找 `icon-classifier/` 子目录。
    pub fn new(models_root: &Path) -> Result<Self> {
        let model_dir = models_root.join(MODEL_SUBDIR);
        let model_path = model_dir.join(MODEL_FILENAME);
        let labels_path = model_dir.join(LABELS_FILENAME);

        // 提前检查文件存在性（比 ONNX Runtime 的 C++ 错误信息更友好）
        if !model_path.exists() {
            anyhow::bail!(
                "[IconClassifier] Model not found at: {}\n{}",
                model_path.display(),
                MODEL_DIR_HELP.replace("{models_dir}", &models_root.display().to_string())
            );
        }
        if !labels_path.exists() {
            anyhow::bail!(
                "[IconClassifier] Labels not found at: {}\n{}",
                labels_path.display(),
                MODEL_DIR_HELP.replace("{models_dir}", &models_root.display().to_string())
            );
        }

        println!("  [IconClassifier] Loading model: {}", model_path.display());
        let session = SessionBuilder::new()
            .context("Failed to create ONNX Runtime session builder")?
            .commit_from_file(&model_path)
            .with_context(|| {
                format!(
                    "Failed to load icon classifier model: {}\n\
                     Possible cause: corrupted or incompatible ONNX model.",
                    model_path.display()
                )
            })?;

        let labels = load_labels(&labels_path)?;
        println!("  [IconClassifier] Model loaded: {} labels", labels.len());

        Ok(Self { session, labels })
    }

    pub fn classify(&mut self, icon_img: &DynamicImage) -> Result<Vec<(String, f32)>> {
        let pixels = preprocess_icon(icon_img)?;
        let logits = self.infer(pixels)?;

        let probs = softmax(&logits);
        let mut ranked: Vec<(usize, f32)> = probs.iter().enumerate().map(|(i, &p)| (i, p)).collect();
        ranked.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap());

        let candidates: Vec<(String, f32)> = ranked
            .iter()
            .take(5)
            .filter(|(_, conf)| *conf >= 0.40)
            .map(|(idx, conf)| {
                let name = self.labels.get(*idx).cloned().unwrap_or_else(|| "unknown".to_string());
                (name, *conf)
            })
            .collect();

        if candidates.is_empty() {
            Ok(vec![("unknown".to_string(), ranked[0].1)])
        } else {
            Ok(candidates)
        }
    }

    fn infer(&mut self, pixels: Vec<f32>) -> Result<Vec<f32>> {
        let tensor = Value::from_array((vec![1, 1, IMG_SIZE as usize, IMG_SIZE as usize], pixels))
            .context("Failed to create input tensor")?;
        let outputs = self.session.run(ort::inputs! { "input" => tensor })
            .context("ONNX inference failed")?;
        let (_shape, data) = outputs["output"]
            .try_extract_tensor::<f32>()
            .context("Failed to extract output tensor")?;
        Ok(data.to_vec())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 全局单例
// ═══════════════════════════════════════════════════════════════════════════

static GLOBAL_CLASSIFIER: LazyLock<Mutex<Option<IconClassifier>>> = LazyLock::new(|| Mutex::new(None));

/// 初始化全局 Icon 分类器。
/// 批处理时只需调用一次，之后所有图片共享同一模型实例。
///
/// 也可通过 `quasivision::init_models("resources")` 统一初始化所有模型。
pub fn init_global(models_root: &Path) -> Result<()> {
    let classifier = IconClassifier::new(models_root)?;
    let mut guard = GLOBAL_CLASSIFIER
        .lock()
        .map_err(|e| anyhow::anyhow!("[IconClassifier] Lock error: {e}"))?;
    *guard = Some(classifier);
    println!("  [IconClassifier] Global instance initialized");
    Ok(())
}

/// 清理全局 Icon 分类器，释放 ONNX 模型占用的内存。
pub fn clean_global() {
    if let Ok(mut guard) = GLOBAL_CLASSIFIER.lock() {
        *guard = None;
        println!("  [IconClassifier] Global instance cleaned up");
    }
}

/// 使用全局 Icon 分类器对所有 Icon 元素进行含义识别。
///
/// 必须先调用 `init_global()`，否则返回错误。
/// 便捷方式：调用 `quasivision::init_models("resources")` 一次性初始化所有模型。
pub fn classify_all_icons_global(img: &DynamicImage, elements: &mut [Element]) -> Result<()> {
    let mut guard = GLOBAL_CLASSIFIER
        .lock()
        .map_err(|e| anyhow::anyhow!("[IconClassifier] Lock error: {e}"))?;
    match guard.as_mut() {
        Some(classifier) => {
            classify_all_icons(classifier, img, elements);
            Ok(())
        }
        None => Err(anyhow::anyhow!(
            "[IconClassifier] Global instance not initialized.\n\
             Call `quasivision::init_models(\"resources\")` to initialize all models at once,\n\
             or call `icon_classifier::init_global(Path::new(\"resources\"))` directly.\n\
             Replace \"resources\" with the correct path to your model files."
        )),
    }
}

// ── 预处理 ──

fn preprocess_icon(icon_img: &DynamicImage) -> Result<Vec<f32>> {
    let (w, h) = icon_img.dimensions();
    let max_dim = w.max(h) as f32;
    let scale = IMG_SIZE as f32 / max_dim;
    let new_w = (w as f32 * scale).round() as u32;
    let new_h = (h as f32 * scale).round() as u32;
    let pad_x = (IMG_SIZE - new_w) / 2;
    let pad_y = (IMG_SIZE - new_h) / 2;

    let resized = icon_img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
    let mut canvas = image::RgbImage::from_pixel(IMG_SIZE, IMG_SIZE, image::Rgb([255, 255, 255]));
    composite_to_canvas(&resized, &mut canvas, pad_x, pad_y);

    let bg = estimate_bg_color(&canvas);
    let max_dist = 255.0 * 3.0;
    let mut output = Vec::with_capacity((IMG_SIZE * IMG_SIZE) as usize);

    for y in 0..IMG_SIZE {
        for x in 0..IMG_SIZE {
            let px = canvas.get_pixel(x, y);
            let dist = (px[0] as f32 - bg[0] as f32).abs()
                + (px[1] as f32 - bg[1] as f32).abs()
                + (px[2] as f32 - bg[2] as f32).abs();
            output.push(1.0 - (dist / max_dist).min(1.0));
        }
    }
    Ok(output)
}

fn composite_to_canvas(src: &DynamicImage, canvas: &mut image::RgbImage, pad_x: u32, pad_y: u32) {
    let (sw, sh) = (src.width(), src.height());
    match src.as_rgba8() {
        Some(rgba) => {
            for y in 0..sh {
                for x in 0..sw {
                    let px = rgba.get_pixel(x, y);
                    let a = px[3] as f32 / 255.0;
                    let inv = 1.0 - a;
                    canvas.put_pixel(x + pad_x, y + pad_y, image::Rgb([
                        (px[0] as f32 * a + 255.0 * inv).round() as u8,
                        (px[1] as f32 * a + 255.0 * inv).round() as u8,
                        (px[2] as f32 * a + 255.0 * inv).round() as u8,
                    ]));
                }
            }
        }
        None => {
            let rgb = src.to_rgb8();
            for y in 0..sh {
                for x in 0..sw {
                    canvas.put_pixel(x + pad_x, y + pad_y, *rgb.get_pixel(x, y));
                }
            }
        }
    }
}

fn estimate_bg_color(img: &image::RgbImage) -> [u8; 3] {
    let (w, h) = (img.width(), img.height());
    let sample = 8u32;
    let (mut r, mut g, mut b, mut count) = (0u64, 0u64, 0u64, 0u64);

    macro_rules! sample_rect {
        ($x1:expr, $y1:expr, $x2:expr, $y2:expr) => {
            for y in $y1..$y2.min(h) {
                for x in $x1..$x2.min(w) {
                    let px = img.get_pixel(x, y);
                    r += px[0] as u64; g += px[1] as u64; b += px[2] as u64; count += 1;
                }
            }
        };
    }
    sample_rect!(0, 0, sample, sample);
    sample_rect!(w.saturating_sub(sample), 0, w, sample);
    sample_rect!(0, h.saturating_sub(sample), sample, h);
    sample_rect!(w.saturating_sub(sample), h.saturating_sub(sample), w, h);

    if count == 0 { return [255, 255, 255]; }
    [(r / count) as u8, (g / count) as u8, (b / count) as u8]
}

// ── 工具函数 ──

fn load_labels(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read labels file: {}", path.display()))?;
    let map: serde_json::Value = serde_json::from_str(&content)
        .context("Failed to parse labels JSON")?;
    let obj = map.as_object().context("Labels JSON is not an object")?;

    let mut labels: Vec<(usize, String)> = obj.iter()
        .filter_map(|(k, v)| {
            let id = k.parse().ok()?;
            let name = v.as_str()?.to_string();
            Some((id, name))
        }).collect();
    labels.sort_by_key(|(id, _)| *id);
    Ok(labels.into_iter().map(|(_, name)| name).collect())
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max_val = logits.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let exps: Vec<f32> = logits.iter().map(|&x| (x - max_val).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|&x| x / sum).collect()
}

// ── 批量处理接口 ──

/// 对所有 `class == "Icon"` 的元素执行含义识别
pub fn classify_all_icons(
    classifier: &mut IconClassifier,
    img: &DynamicImage,
    elements: &mut [Element],
) {
    let icon_ids: Vec<usize> = elements.iter().enumerate()
        .filter(|(_, e)| e.class == "Icon")
        .map(|(idx, _)| idx)
        .collect();

    if icon_ids.is_empty() { return; }

    let mut classified = 0u32;
    let mut unknown_count = 0u32;
    let mut total_confidence = 0.0f32;
    let rgba_img = img.to_rgba8();

    for &idx in &icon_ids {
        let element = &elements[idx];
        let (x, y, x2, y2) = element.put_bbox();
        let (w, h) = ((x2 - x).max(1) as u32, (y2 - y).max(1) as u32);

        let icon_crop = rgba_img.view(x as u32, y as u32, w, h).to_image();
        let icon_dyn = DynamicImage::ImageRgba8(icon_crop);

        match classifier.classify(&icon_dyn) {
            Ok(candidates) => {
                let element = &mut elements[idx];
                let is_unknown = candidates.len() == 1 && candidates[0].0 == "unknown";

                if is_unknown { unknown_count += 1; continue; }

                let parts: Vec<String> = candidates.iter()
                    .map(|(name, conf)| format!("{} {:.0}%", name, conf * 100.0))
                    .collect();
                let meaning = format!("possibility({})", parts.join(", "));
                let top1_conf = candidates[0].1;

                match &element.text_content {
                    Some(existing) if !existing.is_empty() => {
                        element.text_content = Some(format!("{} | {}", existing, meaning));
                    }
                    _ => { element.text_content = Some(meaning); }
                }
                classified += 1;
                total_confidence += top1_conf;
            }
            Err(e) => {
                eprintln!("  [IconClassifier] Failed to classify icon #{}: {}", element.id, e);
            }
        }
    }

    let avg_conf = if classified > 0 { total_confidence / classified as f32 } else { 0.0 };
    println!("  [IconClassifier] Classified {}/{} icons ({} known, {} unknown, avg conf: {:.1}%)",
        classified, icon_ids.len(), classified, unknown_count, avg_conf * 100.0);
}
