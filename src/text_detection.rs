// ── 基于 oar-ocr（ONNX Runtime + PaddleOCR）的文本检测 ──
// 参考: https://github.com/GreatV/oar-ocr
// 跨平台 GPU 加速：Windows → DirectML, macOS → CoreML, Linux → CPU

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use image::DynamicImage;
use oar_ocr::core::config::{OrtExecutionProvider, OrtGraphOptimizationLevel, OrtSessionConfig};
use oar_ocr::oarocr::{OAROCRBuilder, OAROCR};

use crate::element::Element;

// ── 返回结果 ──

/// 文本检测结果
#[derive(Debug, Clone)]
pub struct TextResult {
    pub texts: Vec<Element>,
}

// ── 全局 OCR 实例（懒初始化） ──

static OCR_INSTANCE: LazyLock<Mutex<Option<OAROCR>>> = LazyLock::new(|| Mutex::new(None));

/// 模型文件列表
const MODEL_FILES: &[&str] = &[
    "ppocrv5_mobile_det.onnx",
    "ppocrv5_mobile_rec.onnx",
    "ppocrv5_dict.txt",
];

// ── 公开 API ──

/// 对图片执行 OCR 文本检测
///
/// OCR 推理时的最大图片边长（超过则等比缩放）
/// 降采样可大幅减少检测阶段计算量（面积减为 1/4 时推理约快 3-4 倍）
const OCR_MAX_DIM: u32 = 1200;

/// 使用 oar-ocr（PaddleOCR v5）自动检测文本区域并识别内容。
/// - 首次调用时会自动初始化 OCR 引擎（查找模型文件）
/// - 模型文件需放在程序目录下的 `ocr-models/` 文件夹
/// - Windows 自动启用 DirectML GPU 加速
/// - macOS 自动启用 CoreML GPU 加速
/// - 图片超过 `OCR_MAX_DIM` 时自动降采样以加速推理，坐标自动还原
pub fn detect_text(img: &DynamicImage) -> TextResult {
    let guard = match get_or_init_ocr() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("[OCR] {}", e);
            return TextResult { texts: Vec::new() };
        }
    };

    let ocr = match guard.as_ref() {
        Some(o) => o,
        None => {
            eprintln!("[OCR] Engine not initialized");
            return TextResult { texts: Vec::new() };
        }
    };

    // ── 降采样：如果图片边长超过 OCR_MAX_DIM，等比缩放以加速推理 ──
    let (orig_w, orig_h) = (img.width(), img.height());
    let (ocr_img, scale_x, scale_y) = if orig_w.max(orig_h) > OCR_MAX_DIM {
        let ratio = OCR_MAX_DIM as f64 / orig_w.max(orig_h) as f64;
        let new_w = (orig_w as f64 * ratio).round() as u32;
        let new_h = (orig_h as f64 * ratio).round() as u32;
        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        println!(
            "  → OCR downsampled: {}x{} → {}x{} (scale={:.3})",
            orig_w, orig_h, new_w, new_h, ratio
        );
        (
            resized,
            orig_w as f64 / new_w as f64,
            orig_h as f64 / new_h as f64,
        )
    } else {
        (img.clone(), 1.0, 1.0)
    };

    let results = match ocr.predict(vec![ocr_img.to_rgb8()]) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[OCR] Prediction failed: {}", e);
            return TextResult { texts: Vec::new() };
        }
    };

    if results.is_empty() {
        return TextResult { texts: Vec::new() };
    }

    let result = &results[0];
    let mut elements = Vec::with_capacity(result.text_regions.len());

    for region in &result.text_regions {
        let text = region.text.as_deref().unwrap_or("");
        let confidence = region.confidence.unwrap_or(0.0);

        // ── 第一步：过滤空白/过短的文本 ──
        if text.len() < 1 {
            continue;
        }
        if text
            .chars()
            .all(|c| c.is_ascii_punctuation() || c.is_whitespace())
        {
            continue;
        }

        // ── 第二步：可信度筛选（参考前端规则） ──
        //   文本越长，阈值越低（长文本更可靠）
        //   单字符文本容易误识别，要求更高置信度
        // println!("[OCR] Text: '{}' (confidence={:.3})", text, confidence);

        let pass = if text.len() > 5 {
            confidence >= 0.85
        } else if text.len() == 1 {
            confidence >= 0.97
        } else {
            confidence >= 0.9
        };
        if !pass {
            continue;
        }

        let bbox = &region.bounding_box;
        // 坐标从降采样空间还原到原始空间
        let x_min = (bbox.x_min() as f64 * scale_x) as i32;
        let y_min = (bbox.y_min() as f64 * scale_y) as i32;
        let x_max = (bbox.x_max() as f64 * scale_x) as i32;
        let y_max = (bbox.y_max() as f64 * scale_y) as i32;

        // ── 第三步：过滤太小的区域（噪声） ──
        if (x_max - x_min) < 5 || (y_max - y_min) < 3 {
            continue;
        }

        let mut element = Element::from_parts(0, x_min, y_min, x_max, y_max, "Text");
        element.text_content = region.text.as_ref().map(|s| s.to_string());
        elements.push(element);
    }

    println!("[OCR] Detected {} text elements", elements.len());
    TextResult { texts: elements }
}

// ── 全局生命周期管理（第三方库调用时批处理复用） ──

/// 主动初始化 OCR 引擎，预加载模型。
/// 批处理时只需调用一次，之后每张图片直接调用 `detect_text()`。
pub fn init_ocr() -> Result<(), String> {
    let _guard = get_or_init_ocr()?;
    println!("[OCR] Engine pre-initialized");
    Ok(())
}

/// 清理 OCR 引擎，释放 GPU/CPU 内存。
/// 处理完所有图片后调用，防止全局单例长期占用内存。
pub fn clean_ocr() {
    if let Ok(mut guard) = OCR_INSTANCE.lock() {
        *guard = None;
        println!("[OCR] Engine cleaned up");
    }
}

/// 获取或初始化 OCR 实例
fn get_or_init_ocr() -> Result<std::sync::MutexGuard<'static, Option<OAROCR>>, String> {
    let mut guard = OCR_INSTANCE
        .lock()
        .map_err(|e| format!("[OCR] Lock error: {}", e))?;

    if guard.is_none() {
        let models_dir = find_models_dir()?;

        let det_path = models_dir.join("ppocrv5_mobile_det.onnx");
        let rec_path = models_dir.join("ppocrv5_mobile_rec.onnx");
        let dict_path = models_dir.join("ppocrv5_dict.txt");

        // 检查模型文件
        for (name, p) in [
            ("detection model", &det_path),
            ("recognition model", &rec_path),
            ("dictionary", &dict_path),
        ] {
            if !p.exists() {
                return Err(format!(
                    "[OCR] {} not found at: {}\n\
                     Please download OCR models from:\n\
                     https://github.com/GreatV/oar-ocr/releases\n\
                     And place them in: {}",
                    name,
                    p.display(),
                    models_dir.display()
                ));
            }
        }

        // ── ONNX Runtime 配置（性能优化） ──
        let mut ort_config = OrtSessionConfig::new()
            .with_memory_pattern(true)
            .with_intra_threads(4)
            .with_inter_threads(2)
            .with_optimization_level(OrtGraphOptimizationLevel::Level3)
            .add_config_entry("session.intra_op.allow_spinning", "1")
            .add_config_entry("session.inter_op.allow_spinning", "1");

        ort_config = ort_config.with_execution_providers(build_provider_chain());

        // ── 构建 OCR 管线 ──
        let ocr = OAROCRBuilder::new(
            det_path.to_string_lossy().as_ref(),
            rec_path.to_string_lossy().as_ref(),
            dict_path.to_string_lossy().as_ref(),
        )
        .ort_session(ort_config)
        .region_batch_size(16)
        .build()
        .map_err(|e| format!("[OCR] Failed to initialize: {}", e))?;

        println!("[OCR] Engine initialized (oar-ocr + PaddleOCR v5)");
        *guard = Some(ocr);
    }

    Ok(guard)
}

/// 构建平台最优执行提供者链（按顺序尝试，自动回退到 CPU）
fn build_provider_chain() -> Vec<OrtExecutionProvider> {
    let mut providers = Vec::new();

    // Windows：DirectML（AMD/Intel/NVIDIA 任何 GPU 均可加速）
    #[cfg(target_os = "windows")]
    providers.push(OrtExecutionProvider::DirectML { device_id: Some(0) });

    // macOS：CoreML（系统自带 Neural Engine/GPU 加速）
    #[cfg(target_os = "macos")]
    providers.push(OrtExecutionProvider::CoreML {
        ane_only: Some(false),
        subgraphs: Some(true),
    });

    // 全平台兜底：CPU
    providers.push(OrtExecutionProvider::CPU);

    providers
}

/// 查找模型文件目录（搜索多个位置）
fn find_models_dir() -> Result<PathBuf, String> {
    // 1. 环境变量指定
    if let Ok(dir) = std::env::var("QUASIVISION_MODELS_DIR") {
        let p = PathBuf::from(&dir);
        if p.join("ppocrv5_mobile_det.onnx").exists() {
            return Ok(p);
        }
    }

    // 2. 程序所在目录下的 ocr-models/
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let p = exe_dir.join("ocr-models");
            if p.join("ppocrv5_mobile_det.onnx").exists() {
                return Ok(p);
            }
        }
    }

    // 3. 当前工作目录下的 ocr-models/
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("ocr-models");
        if p.join("ppocrv5_mobile_det.onnx").exists() {
            return Ok(p);
        }
    }

    // 4. 项目开发模式下的 resources/ocr-models/（编译时嵌入路径，发布后仍可用）
    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        let dev_path = PathBuf::from(manifest_dir)
            .join("resources")
            .join("ocr-models");
        if dev_path.join("ppocrv5_mobile_det.onnx").exists() {
            return Ok(dev_path);
        }
    }

    Err(format!(
        "OCR models directory not found.\n\
         Searched locations:\n\
         - $QUASIVISION_MODELS_DIR\n\
         - (exe dir)/ocr-models/\n\
         - ./ocr-models/\n\
         - <project>/resources/ocr-models/\n\n\
         Setup:\n\
         1. Download models from: https://github.com/GreatV/oar-ocr/releases\n\
         2. Required files:\n\
            {}\n\
         3. Place them in: ./ocr-models/\n\
         Or set env: QUASIVISION_MODELS_DIR=/path/to/models",
        MODEL_FILES.join("\n            "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_text_empty() {
        let img = DynamicImage::new_rgb8(100, 100);
        let result = detect_text(&img);
        assert!(result.texts.is_empty());
    }
}
