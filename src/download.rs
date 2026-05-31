// ── 模型自动下载（从 Hugging Face） ──
//
// 当本地模型文件缺失时，自动从 `chenjian-wei/quasivision-models` 下载。
// 下载进度会打印到控制台。

use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// 默认下载源（Hugging Face）
/// 可通过环境变量 `QUASIVISION_MODELS_URL` 覆盖为国内镜像
const HF_BASE: &str = "https://huggingface.co/chenjian-wei/quasivision-models/resolve/main";

fn get_base_url() -> String {
    match std::env::var("QUASIVISION_MODELS_URL") {
        Ok(url) if !url.is_empty() => {
            println!("  [download] Using custom mirror: {url}");
            url.trim_end_matches('/').to_string()
        }
        _ => HF_BASE.to_string(),
    }
}

/// 需要下载的所有模型文件（相对路径）
const MODEL_FILES: &[&str] = &[
    "ocr-models/ppocrv5_mobile_det.onnx",
    "ocr-models/ppocrv5_mobile_rec.onnx",
    "ocr-models/ppocrv5_dict.txt",
    "icon-classifier/icon_classifier.onnx",
    "icon-classifier/labels.json",
    "object-detection/yolov8s-worldv2.onnx",
    "object-detection/yolov8s-worldv2_labels.txt",
];

static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// 检查模型文件是否齐全
pub fn all_models_exist(models_dir: &Path) -> bool {
    MODEL_FILES.iter().all(|rel| models_dir.join(rel).exists())
}

/// 下载缺失的模型文件
///
/// 返回下载的文件数量（0 = 全部已存在）
pub fn download_missing(models_dir: &Path) -> Result<usize, String> {
    if all_models_exist(models_dir) {
        println!("  [download] All model files already exist, skipping download");
        return Ok(0);
    }

    // 防止并行下载
    if DOWNLOAD_IN_PROGRESS.swap(true, Ordering::Relaxed) {
        return Err("Download already in progress".into());
    }
    defer::guard(|| {
        DOWNLOAD_IN_PROGRESS.store(false, Ordering::Relaxed);
    });

    let mut count = 0;

    for rel_path in MODEL_FILES {
        let local = models_dir.join(rel_path);
        if local.exists() {
            continue;
        }

        // 创建父目录
        if let Some(parent) = local.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
        }

        let base = get_base_url();
        let url = format!("{base}/{rel_path}");
        println!("  [download] ↓ {url}");

        download_file(&url, &local).map_err(|e| {
            fallback_hint();
            format!("Failed to download {url}: {e}")
        })?;

        count += 1;
    }

    if count > 0 {
        println!(
            "  [download] Downloaded {count} file(s) to {}",
            models_dir.display()
        );
    }
    Ok(count)
}

/// 下载单个文件（流式写入，无大小限制）
fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    // 检查 HTTP 状态码
    let status = response.status();
    if status != 200 {
        return Err(format!("HTTP {status} for {url}"));
    }

    let mut body = response.into_body();

    // 流式写入文件，避免 ureq 默认 10MB 的限制
    let mut out =
        fs::File::create(dest).map_err(|e| format!("Failed to create {}: {e}", dest.display()))?;

    io::copy(&mut body.as_reader(), &mut out)
        .map_err(|e| format!("Failed to read/write response: {e}"))?;

    Ok(())
}

/// 下载失败时打印替代方案提示
fn fallback_hint() {
    eprintln!(
        "\n         [download] Hint: Download models manually:\n         \n         1. Make sure \"resources/\" directory exists in the current folder\n         2. Download or copy the model files into it:\n         \n         Option A (recommended for China users):\n         - Use a HF mirror, e.g.:\n           set QUASIVISION_MODELS_URL=https://hf-mirror.com/chenjian-wei/quasivision-models/resolve/main\n         \n         Option B:\n         - Download from Hugging Face manually:\n           https://huggingface.co/chenjian-wei/quasivision-models\n         \n         Option C:\n         - Copy an existing \"resources/\" folder from the project directory\n         "
    );
}

// 简单的 defer 实现（RAII guard）
mod defer {
    pub struct Guard<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Guard<F> {
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }
    pub fn guard<F: FnOnce()>(f: F) -> Guard<F> {
        Guard(Some(f))
    }
}
