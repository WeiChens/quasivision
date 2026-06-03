// ── 物体检测 —— YOLOE-26n（基于 ort/ONNX Runtime） ──
//
// 使用 yoloe-26n-seg 模型，支持动态输入尺寸。
// 输出格式：(1, 300, 38)
//   [0:4]  bbox (x1, y1, x2, y2) — letterbox 坐标
//   [4]    max confidence（已 sigmoid）
//   [5]    class label index（由 TopK 选出）
//   [6:38] mask coefficients（分割用，检测忽略）

use std::path::Path;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use ort::session::Session;
use ort::value::TensorRef;
use serde::Serialize;

/// YOLOE 默认推理尺寸（长边）
const DEFAULT_IMGSZ: u32 = 640;

/// NMS IoU 阈值（模型已内置 NMS，此为二次过滤）（模型已内置 NMS，此为二次过滤）
const IOU_THRESH: f32 = 0.70;

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub class_name: String,
    pub confidence: f32,
    pub bbox: DetectionBbox,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DetectionBbox {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

impl DetectionBbox {
    pub fn area(&self) -> f64 {
        (self.x_max - self.x_min) as f64 * (self.y_max - self.y_min) as f64
    }

    pub fn intersection_area(&self, other: &DetectionBbox) -> f64 {
        let x_overlap = (self.x_max.min(other.x_max) - self.x_min.max(other.x_min)).max(0.0) as f64;
        let y_overlap = (self.y_max.min(other.y_max) - self.y_min.max(other.y_min)).max(0.0) as f64;
        x_overlap * y_overlap
    }

    pub fn contained_in(&self, other: &DetectionBbox, threshold: f64) -> bool {
        let inter = self.intersection_area(other);
        if inter <= 0.0 {
            return false;
        }
        let self_area = self.area();
        self_area > 0.0 && inter / self_area > threshold
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectionNode {
    pub class_name: String,
    pub confidence: f32,
    pub bbox: DetectionBbox,
    pub children: Vec<DetectionNode>,
}

pub fn build_detection_tree(detections: &[Detection]) -> Vec<DetectionNode> {
    if detections.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<(usize, &Detection)> = detections.iter().enumerate().collect();
    sorted.sort_by(|(_, a), (_, b)| a.bbox.area().partial_cmp(&b.bbox.area()).unwrap());

    let n = detections.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];

    for i in 0..n {
        let (idx_i, det_i) = sorted[i];
        for j in (i + 1)..n {
            let (idx_j, det_j) = sorted[j];
            if det_i.bbox.contained_in(&det_j.bbox, 0.5) {
                match parent[idx_i] {
                    None => parent[idx_i] = Some(idx_j),
                    Some(current_p) => {
                        if det_j.bbox.area() < detections[current_p].bbox.area() {
                            parent[idx_i] = Some(idx_j);
                        }
                    }
                }
            }
        }
    }

    fn build_node(idx: usize, detections: &[Detection], parent: &[Option<usize>]) -> DetectionNode {
        let det = &detections[idx];
        let children: Vec<DetectionNode> = parent
            .iter()
            .enumerate()
            .filter(|(_, &p)| p == Some(idx))
            .map(|(child_idx, _)| build_node(child_idx, detections, parent))
            .collect();
        DetectionNode {
            class_name: det.class_name.clone(),
            confidence: det.confidence,
            bbox: det.bbox,
            children,
        }
    }

    parent
        .iter()
        .enumerate()
        .filter(|(_, &p)| p.is_none())
        .map(|(idx, _)| build_node(idx, detections, &parent))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// 全局单例
// ═══════════════════════════════════════════════════════════════════════════

struct GlobalDetectorData {
    session: Session,
    class_names: Vec<String>,
}

static GLOBAL_DETECTOR: LazyLock<Mutex<Option<GlobalDetectorData>>> =
    LazyLock::new(|| Mutex::new(None));

/// 模型目录结构指引（用于错误提示）
const MODEL_DIR_HELP: &str = "\n\
Expected directory structure:\n\
  {models_dir}/object-detection/\n\
    yoloe-26n-seg.onnx\n\
    yoloe-26n_classes.txt\n\
Download the YOLOE model files and place them in the correct directory.";

/// 初始化全局物体检测器（预加载 YOLOE 模型 + 标签文件）。
/// 批处理时只需调用一次，之后每张图共享同一模型 Session。
pub fn init_global(models_dir: &str) -> Result<()> {
    let dir = models_dir.trim_end_matches('/');
    let model_str = format!("{}/object-detection/yoloe-26n-seg.onnx", dir);
    let labels_str = format!("{}/object-detection/yoloe-26n_classes.txt", dir);
    let model_path = Path::new(&model_str);
    let labels_path = Path::new(&labels_str);

    if !model_path.exists() {
        anyhow::bail!(
            "[ObjectDetector] Model not found at: {}\n{}",
            model_path.display(),
            MODEL_DIR_HELP.replace("{models_dir}", dir)
        );
    }
    if !labels_path.exists() {
        anyhow::bail!(
            "[ObjectDetector] Labels not found at: {}\n{}",
            labels_path.display(),
            MODEL_DIR_HELP.replace("{models_dir}", dir)
        );
    }

    let class_names = load_labels_file(labels_path)?;
    let num_classes = class_names.len();
    let session = load_model(model_path)?;

    let mut guard = GLOBAL_DETECTOR
        .lock()
        .map_err(|e| anyhow::anyhow!("[ObjectDetector] Lock error: {e}"))?;
    *guard = Some(GlobalDetectorData {
        session,
        class_names,
    });
    println!(
        "  [ObjectDetector] YOLOE-26n global instance initialized ({} classes)",
        num_classes
    );
    Ok(())
}

pub fn clean_global() {
    if let Ok(mut guard) = GLOBAL_DETECTOR.lock() {
        *guard = None;
        println!("  [ObjectDetector] Global instance cleaned up");
    }
}

fn try_global_inference(img: &DynamicImage, conf_threshold: f32) -> Option<Vec<Detection>> {
    let mut guard = GLOBAL_DETECTOR.lock().ok()?;
    let cached = guard.as_mut()?;
    Some(infer_with_session(
        img,
        &mut cached.session,
        &cached.class_names,
        conf_threshold,
        DEFAULT_IMGSZ,
    ))
}

// ── 主函数 ──

/// 对图片执行物体检测
///
/// 优先使用全局缓存的 Session（批处理优化），未初始化时自动从文件加载（兼容单次调用）。
pub fn run_object_detection(
    img: &DynamicImage,
    model_path: &str,
    labels_path: &str,
    conf_threshold: f32,
) -> Vec<Detection> {
    if let Some(dets) = try_global_inference(img, conf_threshold) {
        return dets;
    }

    let model_path = Path::new(model_path);
    let labels_path = Path::new(labels_path);

    if !model_path.exists() {
        eprintln!(
            "  [ObjectDetector] Model not found at: {}.\n\
                   Run `quasivision::init_models(\"resources\")` first or ensure the model file exists.",
            model_path.display()
        );
        return Vec::new();
    }
    if !labels_path.exists() {
        eprintln!(
            "  [ObjectDetector] Labels not found at: {}.",
            labels_path.display()
        );
        return Vec::new();
    }

    let class_names = match load_labels_file(labels_path) {
        Ok(names) => {
            println!(
                "  [ObjectDetector] Loaded {} classes from {}",
                names.len(),
                labels_path.display()
            );
            names
        }
        Err(e) => {
            eprintln!("  [ObjectDetector] Failed to load labels: {}", e);
            return Vec::new();
        }
    };

    let mut session = match load_model(model_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  [ObjectDetector] Load model failed: {}", e);
            return Vec::new();
        }
    };

    infer_with_session(img, &mut session, &class_names, conf_threshold, DEFAULT_IMGSZ)
}

// ── YOLOE-26n 推理核心 ──

fn infer_with_session(
    img: &DynamicImage,
    session: &mut Session,
    class_names: &[String],
    conf_threshold: f32,
    imgsz: u32,
) -> Vec<Detection> {
    let (orig_w, orig_h) = img.dimensions();

    // 1. 预处理：letterbox + normalize
    let (data, scale, pad_left, pad_top, _nw, _nh) = match letterbox_preprocess(img, imgsz) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  [ObjectDetector] Preprocess failed: {}", e);
            return Vec::new();
        }
    };
    let ts = imgsz as usize;

    // 2. 构建输入 tensor
    let input_tensor = match TensorRef::from_array_view(
        ([1usize, 3, ts, ts], data.as_slice()),
    ) {
        Ok(t) => t.into_dyn(),
        Err(e) => {
            eprintln!("  [ObjectDetector] Create tensor failed: {}", e);
            return Vec::new();
        }
    };

    // 3. 推理
    let outputs = match session.run(ort::inputs![input_tensor]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  [ObjectDetector] Inference failed: {}", e);
            return Vec::new();
        }
    };

    // 4. 解析 output0: (1, 300, 38)
    let o0 = match outputs[0].try_extract_array::<f32>() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("  [ObjectDetector] Failed to extract output0: {}", e);
            return Vec::new();
        }
    };

    let num_dets = o0.shape().get(1).copied().unwrap_or(0);

    let mut raw_detections: Vec<Detection> = Vec::new();

    for d in 0..num_dets {
        // 格式: [bbox(4), max_score(1), class_label(1), mask_coeffs(32)]
        let x1 = o0[[0, d, 0]];
        let y1 = o0[[0, d, 1]];
        let x2 = o0[[0, d, 2]];
        let y2 = o0[[0, d, 3]];
        let conf = o0[[0, d, 4]];
        let class_id = o0[[0, d, 5]] as usize;

        if conf < conf_threshold {
            continue;
        }
        if x2 <= x1 || y2 <= y1 {
            continue;
        }

        let class_name = class_names
            .get(class_id)
            .cloned()
            .unwrap_or_else(|| format!("class_{}", class_id));

        raw_detections.push(Detection {
            class_name,
            confidence: conf,
            bbox: DetectionBbox {
                x_min: x1,
                y_min: y1,
                x_max: x2,
                y_max: y2,
            },
        });
    }

    if raw_detections.is_empty() {
        return Vec::new();
    }

    // 5. NMS（模型已内置 NMS，二次过滤确保干净）
    let kept = non_max_suppression(&raw_detections, IOU_THRESH);

    // 6. 将 letterbox 坐标映射回原图坐标
    kept.into_iter()
        .map(|det| Detection {
            class_name: det.class_name,
            confidence: det.confidence,
            bbox: DetectionBbox {
                x_min: ((det.bbox.x_min - pad_left as f32) / scale)
                    .clamp(0.0, orig_w as f32),
                y_min: ((det.bbox.y_min - pad_top as f32) / scale)
                    .clamp(0.0, orig_h as f32),
                x_max: ((det.bbox.x_max - pad_left as f32) / scale)
                    .clamp(0.0, orig_w as f32),
                y_max: ((det.bbox.y_max - pad_top as f32) / scale)
                    .clamp(0.0, orig_h as f32),
            },
        })
        .collect()
}

// ── 辅助函数 ──

fn load_model(model_path: &Path) -> Result<Session> {
    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(4);

    Session::builder()
        .map_err(|e| anyhow::anyhow!("Failed to create ORT session builder: {}", e))?
        .with_intra_threads(num_threads)
        .map_err(|e| anyhow::anyhow!("Failed to set intra threads: {}", e))?
        .with_inter_threads(num_threads)
        .map_err(|e| anyhow::anyhow!("Failed to set inter threads: {}", e))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow::anyhow!("Failed to load model {}: {}", model_path.display(), e))
}

/// Letterbox 预处理：等比例缩放 + 边缘填充
///
/// 返回 (CHW RGB float32 数据, 缩放比例, 左填充, 上填充, 缩放后宽, 缩放后高)
fn letterbox_preprocess(
    img: &DynamicImage,
    imgsz: u32,
) -> Result<(Vec<f32>, f32, u32, u32, u32, u32)> {
    let (w, h) = img.dimensions();
    let scale = imgsz as f32 / w.max(h) as f32;
    let nw = ((w as f32 * scale) as u32).max(1);
    let nh = ((h as f32 * scale) as u32).max(1);

    let resized = img.resize_exact(nw, nh, image::imageops::FilterType::CatmullRom);

    let pad_left = (imgsz - nw) / 2;
    let pad_top = (imgsz - nh) / 2;
    let ts = imgsz as usize;

    // 创建填充画布（RGB，填充值 114）
    let mut canvas = vec![114u8; ts * ts * 3];
    let rgb = resized.to_rgb8();

    for y in 0..nh {
        for x in 0..nw {
            let p = rgb.get_pixel(x, y);
            let idx = ((pad_top + y) as usize * ts + (pad_left + x) as usize) * 3;
            canvas[idx] = p[0]; // R
            canvas[idx + 1] = p[1]; // G
            canvas[idx + 2] = p[2]; // B
        }
    }

    // 转换为 CHW float32，归一化到 [0, 1]
    let n = ts * ts;
    let mut data = vec![0.0f32; 3 * n];
    for i in 0..n {
        data[i] = canvas[i * 3] as f32 / 255.0; // R channel
        data[n + i] = canvas[i * 3 + 1] as f32 / 255.0; // G channel
        data[2 * n + i] = canvas[i * 3 + 2] as f32 / 255.0; // B channel
    }

    Ok((data, scale, pad_left, pad_top, nw, nh))
}

fn iou(a: &DetectionBbox, b: &DetectionBbox) -> f32 {
    let x1 = a.x_min.max(b.x_min);
    let y1 = a.y_min.max(b.y_min);
    let x2 = a.x_max.min(b.x_max);
    let y2 = a.y_max.min(b.y_max);
    if x2 <= x1 || y2 <= y1 {
        return 0.0;
    }
    let inter = (x2 - x1) * (y2 - y1);
    let union = (a.x_max - a.x_min) * (a.y_max - a.y_min)
        + (b.x_max - b.x_min) * (b.y_max - b.y_min)
        - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn non_max_suppression(detections: &[Detection], iou_threshold: f32) -> Vec<Detection> {
    let mut class_indices: std::collections::HashMap<&str, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, det) in detections.iter().enumerate() {
        class_indices.entry(det.class_name.as_str()).or_default().push(i);
    }

    let mut keep_mask = vec![false; detections.len()];
    for (_class, indices) in &class_indices {
        let mut sorted = indices.clone();
        sorted.sort_by(|&a, &b| {
            detections[b]
                .confidence
                .partial_cmp(&detections[a].confidence)
                .unwrap()
        });

        let mut suppressed = vec![false; sorted.len()];
        for i in 0..sorted.len() {
            if suppressed[i] {
                continue;
            }
            keep_mask[sorted[i]] = true;
            for j in (i + 1)..sorted.len() {
                if !suppressed[j]
                    && iou(&detections[sorted[i]].bbox, &detections[sorted[j]].bbox)
                        > iou_threshold
                {
                    suppressed[j] = true;
                }
            }
        }
    }

    detections
        .iter()
        .enumerate()
        .filter(|&(i, _)| keep_mask[i])
        .map(|(_, d)| d.clone())
        .collect()
}

fn load_labels_file(path: &Path) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read labels: {}", path.display()))?;
    let labels: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if labels.is_empty() {
        anyhow::bail!("Labels file is empty: {}", path.display());
    }
    Ok(labels)
}
