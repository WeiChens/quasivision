// ── quasivision 库入口 ──
//
// 分步式 API：调用方可自由组合各检测步骤。
//
// 快速上手：
// ```ignore
// use quasivision::pipeline::PipelineConfig;
//
// let cfg = PipelineConfig::new("resources");
// let img = cfg.read_image("screenshot.png")?;
// let elements = cfg.detect_components(&img)?;
// let texts = cfg.run_ocr(&img)?;
// let elements = cfg.merge(&img, &elements, &texts)?;
// cfg.classify_icons(&img, &mut elements)?;
//
// let json = quasivision::to_json_string(&elements, img_shape);
// println!("{}", json);
// ```

pub mod bbox;
pub mod color;
pub mod component;
pub mod config;
pub mod download;
pub mod detection;
pub mod element;
pub mod export;
pub mod icon_classifier;
pub mod merger;
pub mod object_detector;
pub mod preprocess;
pub mod text_detection;

// ═══════════════════════════════════════════════════════════════════════════
// 重新导出核心类型（方便调用方直接 use quasivision::Element 而非层层深入）
// ═══════════════════════════════════════════════════════════════════════════

pub use bbox::Bbox;
pub use color::{
    detect_background_color, detect_colors, detect_dominant_color, detect_element_color,
};
pub use component::Component;
pub use config::Config;
pub use element::{
    compute_prominence, prominence_label, AiOutput, CompactElement, Element, ElementPosition,
    OutputResult, TreeNode, TreeOutput,
};
pub use export::{
    draw_elements, draw_object_detections, save_ai_json, save_compact_json,
    save_detection_tree_json, save_detection_tree_text, save_json,
    save_object_detection_visualization, save_text_summary, save_tree_json, save_tree_text,
    save_visualization,
};
pub use icon_classifier::IconClassifier;
pub use merger::{
    check_containment, merge, merge_text_lines, reassign_ids, refine_elements, refine_texts,
    remove_bottom_bar, remove_top_bar, synthesize_orphan_text_regions,
};
pub use object_detector::{build_detection_tree, Detection, DetectionBbox, DetectionNode};
pub use text_detection::{detect_text, TextResult};

use std::sync::atomic::{AtomicBool, Ordering};

/// 标记是否已调用过 `init_models()`，实现幂等性
static MODELS_INITIALIZED: AtomicBool = AtomicBool::new(false);

// ═══════════════════════════════════════════════════════════════════════════
// 全局模型生命周期管理（第三方库调用时批处理复用）
// ═══════════════════════════════════════════════════════════════════════════

/// 模型资源目录的期望文件结构（用于错误提示）
const MODEL_DIR_HELP: &str = "\n\
Expected directory structure:\n\
  {models_dir}/\n\
    object-detection/\n\
      yoloe-26n-seg.onnx\n\
      yoloe-26n_classes.txt\n\
    icon-classifier/\n\
      icon_classifier.onnx\n\
      labels.json\n\
    ocr-models/\n\
      ppocrv5_mobile_det.onnx\n\
      ppocrv5_mobile_rec.onnx\n\
      ppocrv5_dict.txt";

/// 预加载所有模型（OCR + Icon 分类器 + YOLO 物体检测）。
///
/// - 幂等：多次调用只会加载一次
/// - 批处理多张图片时只需调用**一次**，所有模型共享内存中的 Session
/// - `models_dir`：模型资源根目录
///
/// # 示例
/// ```ignore
/// quasivision::init_models("resources")?;
/// for img_path in image_list {
///     let result = pipeline.run_full(img_path)?;
/// }
/// quasivision::clean_models();
/// ```
pub fn init_models(models_dir: &str) -> anyhow::Result<()> {
    // 幂等保护：已初始化则跳过
    if MODELS_INITIALIZED.load(Ordering::Relaxed) {
        println!("[init_models] Already initialized, skipping");
        return Ok(());
    }

    let models_path = std::path::Path::new(models_dir);
    println!("[init_models] Loading models from: {}", models_dir);

    // 检查模型是否齐全，缺失时自动从 Hugging Face 下载
    if !download::all_models_exist(models_path) {
        println!("[init_models] Some model files are missing, downloading from Hugging Face...");
        println!("  Repo: https://huggingface.co/chenjian-wei/quasivision-models");
        download::download_missing(models_path)
            .map_err(|e| anyhow::anyhow!("[init_models] Download failed: {e}"))?;
    } else {
        println!("  [download] All model files already exist");
    }

    // 每个步骤加上明确的 context，失败时用户立即知道哪一步出问题
    text_detection::init_ocr()
        .map_err(|e| anyhow::anyhow!("[init_models] OCR init failed: {}\n{}", e, MODEL_DIR_HELP.replace("{models_dir}", models_dir)))?;

    icon_classifier::init_global(std::path::Path::new(models_dir))
        .map_err(|e| anyhow::anyhow!("[init_models] Icon classifier init failed: {}\n{}", e, MODEL_DIR_HELP.replace("{models_dir}", models_dir)))?;

    object_detector::init_global(models_dir)
        .map_err(|e| anyhow::anyhow!("[init_models] Object detector init failed: {}\n{}", e, MODEL_DIR_HELP.replace("{models_dir}", models_dir)))?;

    MODELS_INITIALIZED.store(true, Ordering::Relaxed);
    println!("[init_models] All models loaded successfully");
    Ok(())
}

/// 清理所有已加载的模型，释放 GPU/CPU 内存。
///
/// 在所有图片处理完毕后调用，避免全局单例长期占用内存。
pub fn clean_models() {
    MODELS_INITIALIZED.store(false, Ordering::Relaxed);
    text_detection::clean_ocr();
    icon_classifier::clean_global();
    object_detector::clean_global();
    println!("[clean_models] All models cleaned up");
}

// ═══════════════════════════════════════════════════════════════════════════
// 序列化辅助函数（返回字符串，不写文件）
// ═══════════════════════════════════════════════════════════════════════════

/// 标准 JSON 格式输出
pub fn to_json_string(elements: &[Element], img_shape: (u32, u32)) -> String {
    let out = OutputResult {
        comps: elements.to_vec(),
        img_shape,
    };
    serde_json::to_string_pretty(&out).unwrap_or_default()
}

/// 压缩 JSON 格式输出（短键名，-50% token）
pub fn to_compact_string(elements: &[Element], _img_shape: (u32, u32)) -> String {
    let compact: Vec<CompactElement> = elements.iter().map(CompactElement::from).collect();
    serde_json::to_string_pretty(&compact).unwrap_or_default()
}

/// AI 归一化输出（坐标 0-1000）
pub fn to_ai_json_string(elements: &[Element], img_shape: (u32, u32)) -> String {
    let ai = AiOutput::from_elements(elements, img_shape);
    serde_json::to_string_pretty(&ai).unwrap_or_default()
}

/// 树形 JSON 输出（嵌套 children，推荐 AI 使用）
pub fn to_tree_json_string(elements: &[Element], img_shape: (u32, u32)) -> String {
    let tree = TreeOutput::from_elements(elements, img_shape);
    serde_json::to_string_pretty(&tree).unwrap_or_default()
}

/// 树形文本输出（适合人眼阅读）
pub fn to_tree_text_string(elements: &[Element], img_shape: (u32, u32)) -> String {
    TreeOutput::from_elements(elements, img_shape).to_text()
}

/// 纯文本摘要输出（适合直接粘贴到 prompt）
pub fn to_text_summary(elements: &[Element], img_shape: (u32, u32)) -> String {
    element::elements_to_text(elements, img_shape)
}

/// 物体检测树 JSON 输出
pub fn object_detection_to_json_string(roots: &[DetectionNode], img_shape: (u32, u32)) -> String {
    let count: usize = roots.iter().map(|n| 1 + count_all(&n.children)).sum();
    let value = serde_json::json!({
        "img_shape": [img_shape.1, img_shape.0],
        "count": count,
        "objects": roots,
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn count_all(nodes: &[DetectionNode]) -> usize {
    nodes.iter().map(|n| 1 + count_all(&n.children)).sum()
}

/// 物体检测树文本输出
pub fn object_detection_to_tree_text(roots: &[DetectionNode], img_shape: (u32, u32)) -> String {
    if roots.is_empty() {
        return format!(
            "Objects ({}×{}):\n  (none detected)",
            img_shape.1, img_shape.0
        );
    }
    let total: usize = roots.iter().map(|n| 1 + count_all(&n.children)).sum();
    let mut lines = Vec::new();
    lines.push(format!(
        "Objects ({}×{}) — {} found:",
        img_shape.1, img_shape.0, total
    ));

    fn render_node(node: &DetectionNode, prefix: &str, is_last: bool, lines: &mut Vec<String>) {
        let connector = if is_last { "└─ " } else { "├─ " };
        let x = node.bbox.x_min.round() as i32;
        let y = node.bbox.y_min.round() as i32;
        let w = (node.bbox.x_max - node.bbox.x_min).round() as i32;
        let h = (node.bbox.y_max - node.bbox.y_min).round() as i32;
        let pct = (node.confidence * 100.0).round() as u32;
        lines.push(format!(
            "{}{}[{:>3},{:>3} {:>3}×{:>3}] {} ({}%)",
            prefix, connector, x, y, w, h, node.class_name, pct
        ));
        let child_prefix = if is_last { "   " } else { "│  " };
        let new_prefix = format!("{}{}", prefix, child_prefix);
        let count = node.children.len();
        for (i, child) in node.children.iter().enumerate() {
            render_node(child, &new_prefix, i == count - 1, lines);
        }
    }

    let root_count = roots.len();
    for (i, root) in roots.iter().enumerate() {
        render_node(root, "", i == root_count - 1, &mut lines);
    }

    lines.join("\n")
}

// ═══════════════════════════════════════════════════════════════════════════
// 高级流水线 API
// ═══════════════════════════════════════════════════════════════════════════

pub mod pipeline {
    //! 分步式流水线 API
    //!
    //! # 示例
    //!
    //! ```ignore
    //! use quasivision::pipeline::PipelineConfig;
    //!
    //! let cfg = PipelineConfig::new("resources");
    //!
    //! // 分步调用（灵活组合）
    //! let img = cfg.read_image("screenshot.png")?;
    //! let elements = cfg.detect_components(&img)?;
    //! let texts = cfg.run_ocr(&img)?;
    //! let elements = cfg.merge(&img, &elements, &texts)?;
    //! cfg.classify_icons(&img, &mut elements)?;
    //!
    //! // 或一键运行全部
    //! let result = cfg.run_full("screenshot.png")?;
    //! ```

    use std::path::Path;
    use std::thread;

    use anyhow::{Context, Result};
    use image::DynamicImage;

    use crate::component::Component;
    use crate::config::Config;
    use crate::detection::*;
    use crate::element::Element;
    use crate::icon_classifier::IconClassifier;
    use crate::merger::*;
    use crate::object_detector::{self, Detection};
    use crate::preprocess;
    use crate::text_detection::{self, TextResult};

    /// 流水线配置（分步式 API 的入口）
    #[derive(Debug, Clone)]
    pub struct PipelineConfig {
        /// UI 检测底层参数
        pub ui_config: Config,
        /// 模型资源根目录（存放 ocr-models / icon-classifier / object-detection 等）
        pub models_dir: String,
        /// 是否启用段落合并
        pub paragraph: bool,
        /// 是否移除顶栏/底栏
        pub remove_bar: bool,
        /// 是否启用子组件检测
        pub sub_component: bool,
        /// 是否为孤儿文本自动合成容器 Block
        pub synthesize_text: bool,
        // ── 物体检测参数 ──
        /// YOLO 模型路径（绝对或相对 models_dir）
        pub detect_model_path: String,
        /// YOLO 标签文件路径
        pub detect_labels_path: String,
        /// 物体检测置信度阈值
        pub detect_conf: f32,
    }

    impl PipelineConfig {
        /// 创建默认配置
        ///
        /// `models_dir`：模型资源根目录，如 `"resources"`
        pub fn new(models_dir: &str) -> Self {
            Self {
                ui_config: Config::default(),
                models_dir: models_dir.to_string(),
                paragraph: false,
                remove_bar: true,
                sub_component: true,
                synthesize_text: true,
                detect_model_path: format!(
                    "{}/object-detection/yoloe-26n-seg.onnx",
                    models_dir.trim_end_matches('/')
                ),
                detect_labels_path: format!(
                    "{}/object-detection/yoloe-26n_classes.txt",
                    models_dir.trim_end_matches('/')
                ),
                detect_conf: 0.01,
            }
        }

        // ── 便捷 setter ──

        pub fn with_ui_config(mut self, config: Config) -> Self {
            self.ui_config = config;
            self
        }

        pub fn with_paragraph(mut self, enabled: bool) -> Self {
            self.paragraph = enabled;
            self
        }

        pub fn with_remove_bar(mut self, enabled: bool) -> Self {
            self.remove_bar = enabled;
            self
        }

        pub fn with_sub_component(mut self, enabled: bool) -> Self {
            self.sub_component = enabled;
            self
        }

        pub fn with_synthesize_text(mut self, enabled: bool) -> Self {
            self.synthesize_text = enabled;
            self
        }

        pub fn with_detect_model(mut self, model_path: &str, labels_path: &str) -> Self {
            self.detect_model_path = model_path.to_string();
            self.detect_labels_path = labels_path.to_string();
            self
        }

        pub fn with_detect_conf(mut self, conf: f32) -> Self {
            self.detect_conf = conf;
            self
        }

        // ═══════════════════════════════════════════════════════════════
        // 步骤 1: 读取图片
        // ═══════════════════════════════════════════════════════════════

        /// 从文件读取图片，返回 (原图, 灰度图)
        pub fn read_image(&self, path: &str) -> Result<(DynamicImage, image::GrayImage)> {
            preprocess::read_image(path, None, None)
                .with_context(|| format!("Failed to read image: {path}"))
        }

        /// 从内存字节读取图片
        pub fn read_image_from_bytes(
            &self,
            data: &[u8],
        ) -> Result<(DynamicImage, image::GrayImage)> {
            preprocess::read_image_from_bytes(data)
        }

        // ═══════════════════════════════════════════════════════════════
        // 步骤 2: 预处理（梯度 + 二值化 + 去线条）
        // ═══════════════════════════════════════════════════════════════

        /// 对彩色图做梯度二值化
        pub fn binarize(&self, img: &DynamicImage) -> image::GrayImage {
            preprocess::binarization_color(img, self.ui_config.gradient_threshold)
        }

        /// 移除线条（分割线等）
        pub fn remove_lines(&self, binary: &mut image::GrayImage) {
            crate::detection::remove_lines(
                binary,
                self.ui_config.line_thickness,
                self.ui_config.line_min_length_ratio,
            );
        }

        // ═══════════════════════════════════════════════════════════════
        // 步骤 3: 组件检测
        // ═══════════════════════════════════════════════════════════════

        /// 执行完整的 UI 组件检测（步骤 2 + 3 + 4 的合并快捷方式）
        pub fn detect_components(&self, img: &DynamicImage) -> Result<Vec<Component>> {
            let gray = img.to_luma8();

            let mut binary = preprocess::binarization_color(img, self.ui_config.gradient_threshold);
            remove_lines(
                &mut binary,
                self.ui_config.line_thickness,
                self.ui_config.line_min_length_ratio,
            );

            let (mut comps, _) = component_detection(&binary, &self.ui_config, true);
            comps = merge_intersected(&comps, false, (0, 0));
            comps = component_filter(
                &comps,
                self.ui_config.obj_min_area as i64,
                (img.height(), img.width()),
            );

            block_recognition(&binary, &mut comps, self.ui_config.block_side_length);

            let nested = nested_components_detection(
                &gray,
                &self.ui_config,
                self.ui_config.block_gradient_threshold,
            );
            for mut nc in nested {
                let mut is_new = true;
                for c in &comps {
                    let rel = nc.bbox.relation(&c.bbox);
                    if rel == -1 || rel == 2 {
                        is_new = false;
                        break;
                    }
                }
                if is_new {
                    nc.category = "Block".to_string();
                    comps.push(nc);
                }
            }

            classify_by_geometry(&mut comps, (img.height(), img.width()));

            let rgb = img.to_rgb8();
            let extra_icons = icon_color_detection(&rgb, &comps, &self.ui_config);
            for mut ic in extra_icons {
                ic.category = "Icon".to_string();
                comps.push(ic);
            }

            if self.sub_component {
                let sub_comps = detect_sub_components(&comps, &binary, &self.ui_config);
                for mut sub in sub_comps {
                    let mut overlap = false;
                    for c in &comps {
                        let rel = sub.bbox.relation_with_bias(&c.bbox, (2, 2));
                        if rel != 0 {
                            overlap = true;
                            break;
                        }
                    }
                    if !overlap {
                        if sub.category == "Compo" {
                            sub.category = "Button".to_string();
                        }
                        comps.push(sub);
                    }
                }
            }

            Ok(comps)
        }

        // ═══════════════════════════════════════════════════════════════
        // 步骤 5: OCR 文本检测
        // ═══════════════════════════════════════════════════════════════

        /// 对图片执行 OCR 文字识别
        pub fn run_ocr(&self, img: &DynamicImage) -> Result<TextResult> {
            Ok(text_detection::detect_text(img))
        }

        // ═══════════════════════════════════════════════════════════════
        // 步骤 6: 合并
        // ═══════════════════════════════════════════════════════════════

        /// 合并组件和 OCR 文本，生成最终 Element 列表
        pub fn merge(
            &self,
            img: &DynamicImage,
            comps: &[Component],
            text_result: &TextResult,
        ) -> Result<Vec<Element>> {
            let mut elements = merge(
                img,
                comps,
                &text_result.texts,
                &self.ui_config,
                self.paragraph,
                self.remove_bar,
            );

            if self.synthesize_text {
                synthesize_orphan_text_regions(&mut elements, &text_result.texts, 0.3, 12);
            }

            let rgb = img.to_rgb8();
            crate::color::detect_colors(&rgb, &mut elements);

            Ok(elements)
        }

        // ═══════════════════════════════════════════════════════════════
        // 步骤 6c: Icon 含义识别
        // ═══════════════════════════════════════════════════════════════

        /// 对 Icon 元素进行含义识别（修改 elements 的 text_content）
        ///
        /// 优先使用全局缓存的 `IconClassifier`（批处理时只需加载一次模型），
        /// 未初始化时自动创建临时实例（兼容单次调用场景）。
        pub fn classify_icons(&self, img: &DynamicImage, elements: &mut [Element]) -> Result<()> {
            let icon_count = elements.iter().filter(|e| e.class == "Icon").count();
            if icon_count == 0 {
                return Ok(());
            }

            // 先尝试使用全局已加载的实例
            if crate::icon_classifier::classify_all_icons_global(img, elements).is_ok() {
                return Ok(());
            }

            // 没有全局实例 → 创建临时实例（兼容单次调用）
            let models_root = Path::new(&self.models_dir);
            let mut classifier =
                IconClassifier::new(models_root).context("Failed to initialize IconClassifier")?;
            crate::icon_classifier::classify_all_icons(&mut classifier, img, elements);
            Ok(())
        }

        // ═══════════════════════════════════════════════════════════════
        // 步骤 6d: 物体检测
        // ═══════════════════════════════════════════════════════════════

        /// 对图片执行物体检测（YOLOE-26n）
        ///
        /// 优先使用全局缓存的 Session（批处理时只需加载一次模型），
        /// 未初始化时自动从文件加载（兼容单次调用）。
        pub fn detect_objects(&self, img: &DynamicImage) -> Vec<Detection> {
            object_detector::run_object_detection(
                img,
                &self.detect_model_path,
                &self.detect_labels_path,
                self.detect_conf,
            )
        }

        // ═══════════════════════════════════════════════════════════════
        // 一图通吃：上面各步骤组合，带后台线程并行
        // ═══════════════════════════════════════════════════════════════

        /// 一键运行完整流水线（含后台并行 OCR + 物体检测）
        ///
        /// 返回 `PipelineResult`，包含所有检测结果和序列化字符串。
        pub fn run_full(&self, img_path: &str) -> Result<PipelineResult> {
            let (_img, _gray) = self.read_image(img_path)?;

            let ocr_handle = thread::spawn({
                let img = _img.clone();
                move || text_detection::detect_text(&img)
            });
            let od_handle = thread::spawn({
                let img = _img.clone();
                let model = self.detect_model_path.clone();
                let labels = self.detect_labels_path.clone();
                let conf = self.detect_conf;
                move || object_detector::run_object_detection(&img, &model, &labels, conf)
            });

            let comps = self.detect_components(&_img)?;
            let text_result = ocr_handle.join().expect("OCR thread panicked");
            let mut elements = self.merge(&_img, &comps, &text_result)?;
            self.classify_icons(&_img, &mut elements)?;
            let object_detections = od_handle.join().expect("Object detection thread panicked");

            let img_shape = (_img.height(), _img.width());

            Ok(PipelineResult {
                elements,
                object_detections,
                img_shape,
                _img,
            })
        }
    }

    /// 完整流水线结果
    pub struct PipelineResult {
        /// 检测到的 UI 元素列表
        pub elements: Vec<Element>,
        /// 物体检测结果
        pub object_detections: Vec<Detection>,
        /// 图片尺寸 (height, width)
        pub img_shape: (u32, u32),
        /// 原始图片（可用于自定义可视化）
        pub _img: DynamicImage,
    }
}
