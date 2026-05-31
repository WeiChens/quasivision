// ── 物体检测 —— YOLO-World（基于 ort/ONNX Runtime） ──

use std::path::Path;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Value;
use serde::Serialize;

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
        if inter <= 0.0 { return false; }
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
    if detections.is_empty() { return Vec::new(); }

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
        let children: Vec<DetectionNode> = parent.iter().enumerate()
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

    parent.iter().enumerate()
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

static GLOBAL_DETECTOR: LazyLock<Mutex<Option<GlobalDetectorData>>> = LazyLock::new(|| Mutex::new(None));

/// 模型目录结构指引（用于错误提示）
const MODEL_DIR_HELP: &str = "\n\
Expected directory structure:\n\
  {models_dir}/object-detection/\n\
    yolov8s-worldv2.onnx\n\
    yolov8s-worldv2_labels.txt\n\
Download the YOLO model files and place them in the correct directory.";

/// 初始化全局物体检测器（预加载 YOLO 模型 + 标签文件）。
/// 批处理时只需调用一次，之后每张图共享同一模型 Session。
pub fn init_global(models_dir: &str) -> Result<()> {
    let dir = models_dir.trim_end_matches('/');
    let model_str = format!("{}/object-detection/yolov8s-worldv2.onnx", dir);
    let labels_str = format!("{}/object-detection/yolov8s-worldv2_labels.txt", dir);
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
    let session = load_model(model_path)?;

    let mut guard = GLOBAL_DETECTOR
        .lock()
        .map_err(|e| anyhow::anyhow!("[ObjectDetector] Lock error: {e}"))?;
    *guard = Some(GlobalDetectorData { session, class_names });
    println!("  [ObjectDetector] Global instance initialized");
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
    Some(infer_with_session(img, &mut cached.session, &cached.class_names, conf_threshold))
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
        eprintln!("  [ObjectDetector] Model not found at: {}.\n\
                   Run `quasivision::init_models(\"resources\")` first or ensure the model file exists.",
            model_path.display());
        return Vec::new();
    }
    if !labels_path.exists() {
        eprintln!("  [ObjectDetector] Labels not found at: {}.", labels_path.display());
        return Vec::new();
    }

    let class_names = match load_labels_file(labels_path) {
        Ok(names) => {
            println!("  [ObjectDetector] Loaded {} classes from {}", names.len(), labels_path.display());
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

    infer_with_session(img, &mut session, &class_names, conf_threshold)
}

fn infer_with_session(
    img: &DynamicImage,
    session: &mut Session,
    class_names: &[String],
    conf_threshold: f32,
) -> Vec<Detection> {
    let num_classes = class_names.len();
    let (orig_w, orig_h) = img.dimensions();
    let target_size = compute_optimal_size(orig_w, orig_h, 1280);

    let (padded_rgb, scale, pad_left, pad_top) = match letterbox_preprocess(img, target_size) {
        Ok(r) => r,
        Err(e) => { eprintln!("  [ObjectDetector] Preprocess failed: {}", e); return Vec::new(); }
    };

    let input_array = create_input_tensor(&padded_rgb, target_size);
    let input_value = match Value::from_array(([1usize, 3, target_size as usize, target_size as usize], input_array.into_raw_vec_and_offset().0)) {
        Ok(v) => v,
        Err(e) => { eprintln!("  [ObjectDetector] Create tensor failed: {}", e); return Vec::new(); }
    };

    let outputs = match session.run(ort::inputs!["images" => input_value]) {
        Ok(v) => v,
        Err(e) => { eprintln!("  [ObjectDetector] Inference failed: {}", e); return Vec::new(); }
    };

    let output_value = match outputs.get("output0") {
        Some(v) => v,
        None => { eprintln!("  [ObjectDetector] Output 'output0' not found"); return Vec::new(); }
    };

    let (_shape, output_slice) = match output_value.try_extract_tensor::<f32>() {
        Ok(t) => t,
        Err(e) => { eprintln!("  [ObjectDetector] Failed to extract output: {}", e); return Vec::new(); }
    };

    let channels = 4 + num_classes;
    let num_anchors = output_slice.len() / channels;
    if output_slice.len() % channels != 0 {
        eprintln!("  [ObjectDetector] Unexpected output size: {}", output_slice.len());
        return Vec::new();
    }

    let mut raw_detections: Vec<Detection> = Vec::new();
    for i in 0..num_anchors {
        let cx = output_slice[i];
        let cy = output_slice[num_anchors + i];
        let w = output_slice[2 * num_anchors + i];
        let h = output_slice[3 * num_anchors + i];

        let mut best_class = 0usize;
        let mut best_score = f32::NEG_INFINITY;
        for c in 0..num_classes {
            let score = output_slice[(4 + c) * num_anchors + i];
            if score > best_score { best_score = score; best_class = c; }
        }

        if best_score < conf_threshold { continue; }
        let (x1, y1, x2, y2) = (cx - w * 0.5, cy - h * 0.5, cx + w * 0.5, cy + h * 0.5);
        if (x2 - x1) <= 0.0 || (y2 - y1) <= 0.0 { continue; }

        raw_detections.push(Detection {
            class_name: class_names[best_class].clone(),
            confidence: best_score,
            bbox: DetectionBbox { x_min: x1, y_min: y1, x_max: x2, y_max: y2 },
        });
    }

    if raw_detections.is_empty() { return Vec::new(); }

    let kept = non_max_suppression(&raw_detections, 0.45);
    kept.into_iter().map(|det| Detection {
        class_name: det.class_name,
        confidence: det.confidence,
        bbox: DetectionBbox {
            x_min: ((det.bbox.x_min - pad_left) / scale).clamp(0.0, orig_w as f32),
            y_min: ((det.bbox.y_min - pad_top) / scale).clamp(0.0, orig_h as f32),
            x_max: ((det.bbox.x_max - pad_left) / scale).clamp(0.0, orig_w as f32),
            y_max: ((det.bbox.y_max - pad_top) / scale).clamp(0.0, orig_h as f32),
        },
    }).collect()
}

// ── 辅助函数 ──

fn load_model(model_path: &Path) -> Result<Session> {
    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get().min(4)).unwrap_or(4);

    Session::builder()
        .map_err(|e| anyhow::anyhow!("Failed to create ORT session builder: {}", e))?
        .with_intra_threads(num_threads)
        .map_err(|e| anyhow::anyhow!("Failed to set intra threads: {}", e))?
        .with_inter_threads(num_threads)
        .map_err(|e| anyhow::anyhow!("Failed to set inter threads: {}", e))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow::anyhow!("Failed to load model {}: {}", model_path.display(), e))
}

fn letterbox_preprocess(img: &DynamicImage, target_size: u32) -> Result<(Vec<u8>, f32, f32, f32)> {
    let (w, h) = img.dimensions();
    let scale = target_size as f32 / w.max(h) as f32;
    let (new_w, new_h) = ((w as f32 * scale).round() as u32, (h as f32 * scale).round() as u32);
    let (pad_left, pad_top) = ((target_size - new_w) as f32 / 2.0, (target_size - new_h) as f32 / 2.0);

    let rgb = img.to_rgb8();
    let resized = image::imageops::resize(&rgb, new_w, new_h, image::imageops::FilterType::Triangle);
    let mut canvas = image::ImageBuffer::from_pixel(target_size, target_size, image::Rgb([114, 114, 114]));
    for y in 0..new_h {
        for x in 0..new_w {
            canvas.put_pixel(x + pad_left.round() as u32, y + pad_top.round() as u32, *resized.get_pixel(x, y));
        }
    }
    Ok((canvas.into_raw(), scale, pad_left, pad_top))
}

fn compute_optimal_size(w: u32, h: u32, max_cap: u32) -> u32 {
    ((w.max(h) + 31) / 32 * 32).clamp(64, max_cap)
}

fn create_input_tensor(rgb_data: &[u8], size: u32) -> Array4<f32> {
    let n = size as usize;
    let mut tensor = Array4::<f32>::zeros((1, 3, n, n));
    for y in 0..n {
        for x in 0..n {
            let idx = (y * n + x) * 3;
            tensor[[0, 0, y, x]] = rgb_data[idx] as f32 / 255.0;
            tensor[[0, 1, y, x]] = rgb_data[idx + 1] as f32 / 255.0;
            tensor[[0, 2, y, x]] = rgb_data[idx + 2] as f32 / 255.0;
        }
    }
    tensor
}

fn iou(a: &DetectionBbox, b: &DetectionBbox) -> f32 {
    let inter = ((a.x_max.min(b.x_max) - a.x_min.max(b.x_min)).max(0.0))
              * ((a.y_max.min(b.y_max) - a.y_min.max(b.y_min)).max(0.0));
    let union = (a.x_max - a.x_min) * (a.y_max - a.y_min)
              + (b.x_max - b.x_min) * (b.y_max - b.y_min) - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

fn non_max_suppression(detections: &[Detection], iou_threshold: f32) -> Vec<Detection> {
    let mut class_indices: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    for (i, det) in detections.iter().enumerate() {
        class_indices.entry(det.class_name.as_str()).or_default().push(i);
    }

    let mut keep_mask = vec![false; detections.len()];
    for (_class, indices) in &class_indices {
        let mut sorted = indices.clone();
        sorted.sort_by(|&a, &b| detections[b].confidence.partial_cmp(&detections[a].confidence).unwrap());

        let mut suppressed = vec![false; sorted.len()];
        for i in 0..sorted.len() {
            if suppressed[i] { continue; }
            keep_mask[sorted[i]] = true;
            for j in (i + 1)..sorted.len() {
                if !suppressed[j] && iou(&detections[sorted[i]].bbox, &detections[sorted[j]].bbox) > iou_threshold {
                    suppressed[j] = true;
                }
            }
        }
    }
    detections.iter().enumerate().filter(|&(i, _)| keep_mask[i]).map(|(_, d)| d.clone()).collect()
}

fn load_labels_file(path: &Path) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read labels: {}", path.display()))?;
    let labels: Vec<String> = raw.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
    if labels.is_empty() { anyhow::bail!("Labels file is empty: {}", path.display()); }
    Ok(labels)
}
