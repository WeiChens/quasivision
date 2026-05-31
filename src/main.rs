use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Instant;

use anyhow::Context;
use clap::Parser;

use quasivision::config::Config;
use quasivision::element;
use quasivision::export;
use quasivision::object_detector;
use quasivision::pipeline;
use quasivision::text_detection;

/// quasivision: Pseudo visual understanding for screenshots
#[derive(Parser, Debug)]
#[command(name = "quasivision", version, about)]
struct Args {
    /// 输入路径：图片文件 或 包含图片的目录
    #[arg(short, long)]
    input: String,

    /// 输出根目录
    #[arg(short, long, default_value = "output")]
    output: String,

    /// 梯度阈值 (dribbble:4, rico:4, web:1)
    #[arg(long, default_value_t = 4)]
    gradient: u8,

    /// 最小连通区域面积
    #[arg(long, default_value_t = 55)]
    min_area: u32,

    /// 是否启用段落合并 (默认: false)
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    paragraph: bool,

    /// 是否移除顶栏/底栏 (默认: true)
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    remove_bar: bool,

    /// 是否启用子组件检测（图片内部按钮检测，默认: true）
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    sub_component: bool,

    /// 是否启用 OCR 文本检测 (默认: true)
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    ocr: bool,

    /// 是否为孤儿文本自动合成容器 Block（手写/文档场景，默认: false）
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    synthesize_text: bool,

    /// 递归处理子目录（输入为目录时）
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false)]
    recursive: bool,

    /// 图片文件扩展名过滤（逗号分隔，如 "png,jpg,jpeg"）
    #[arg(long, default_value = "png,jpg,jpeg,jfif")]
    extensions: String,

    /// 输出格式: standard | compact | ai | text | tree
    #[arg(long, default_value = "tree")]
    format: String,

    // ── 线条移除参数 ──
    /// 线条最大粗细（像素），默认 8
    #[arg(long, default_value_t = 8)]
    line_thickness: u32,

    /// 线条最小长度比例，默认 0.95
    #[arg(long, default_value_t = 0.95)]
    line_min_length: f64,

    // ── 矩形检测参数 ──
    /// 矩形最小平整度，默认 0.7
    #[arg(long, default_value_t = 0.7)]
    rec_evenness: f64,

    /// 矩形最大凹陷比，默认 0.25
    #[arg(long, default_value_t = 0.25)]
    rec_dent: f64,

    /// 圆角容错：跳过每边两端边界点的比例 (0.0=严格直角, 0.08=推荐)
    #[arg(long, default_value_t = 0.08)]
    rec_corner_skip: f64,

    // ── Block 检测参数 ──
    /// Block 边长占比阈值，默认 0.15
    #[arg(long, default_value_t = 0.15)]
    block_side: f64,

    /// Block 嵌套检测梯度阈值，默认 5
    #[arg(long, default_value_t = 5)]
    block_grad: u8,

    // ── 文本参数 ──
    /// 文本最大高度比，默认 0.08
    #[arg(long, default_value_t = 0.08)]
    text_max_h: f64,

    /// 文本单词最大间距（像素），默认 10
    #[arg(long, default_value_t = 10)]
    text_gap: u32,

    // ── Icon 含义识别参数 ──
    /// 是否启用 Icon 含义识别 (默认: true)
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    icon_classify: bool,

    // ── 物体检测参数 ──
    /// 是否启用物体检测 (默认: true)
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    object_detect: bool,

    /// 物体检测置信度阈值
    #[arg(long, default_value_t = 0.2)]
    detect_conf: f32,

    // ── 公共模型目录 ──
    /// 模型资源根目录（存放 ocr-models / icon-classifier / object-detection 等子目录）
    /// 物体检测模型和标签文件按约定放在 {models_dir}/object-detection/ 下
    #[arg(long, default_value = "resources")]
    models_dir: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// RunOptions：将 run_pipeline 的 15 个散落参数封装为结构体
// ═══════════════════════════════════════════════════════════════════════════

/// 流水线运行参数（替代 `run_pipeline` 的散落参数）
///
/// 物体检测模型路径由 `models_dir` 按约定自动推导：
///   - 模型: `{models_dir}/object-detection/yolov8s-worldv2.onnx`
///   - 标签: `{models_dir}/object-detection/yolov8s-worldv2_labels.txt`
struct RunOptions {
    /// UI 检测底层参数
    cfg: Config,
    /// 是否启用段落合并
    is_paragraph: bool,
    /// 是否移除顶栏/底栏
    is_remove_bar: bool,
    /// 是否启用子组件检测
    enable_sub_component: bool,
    /// 是否启用 OCR
    enable_ocr: bool,
    /// 是否为孤儿文本自动合成容器 Block
    enable_synthesize: bool,
    /// 是否启用 Icon 含义识别
    enable_icon_classify: bool,
    /// 是否启用物体检测
    enable_object_detect: bool,
    /// 输出格式
    output_format: String,
    /// 物体检测置信度阈值
    detect_conf: f32,
    /// 模型资源根目录（ocr-models / icon-classifier / object-detection 等子目录的父目录）
    models_dir: String,
}

impl From<&Args> for RunOptions {
    fn from(args: &Args) -> Self {
        let cfg = Config {
            gradient_threshold: args.gradient,
            obj_min_area: args.min_area,
            rec_min_evenness: args.rec_evenness,
            rec_max_dent_ratio: args.rec_dent,
            rec_corner_skip_ratio: args.rec_corner_skip,
            line_thickness: args.line_thickness,
            line_min_length_ratio: args.line_min_length,
            text_max_word_gap: args.text_gap,
            text_max_height: args.text_max_h,
            block_side_length: args.block_side,
            block_gradient_threshold: args.block_grad,
            ..Config::default()
        };

        Self {
            cfg,
            is_paragraph: args.paragraph,
            is_remove_bar: args.remove_bar,
            enable_sub_component: args.sub_component,
            enable_ocr: args.ocr,
            enable_synthesize: args.synthesize_text,
            enable_icon_classify: args.icon_classify,
            enable_object_detect: args.object_detect,
            output_format: args.format.clone(),
            detect_conf: args.detect_conf,
            models_dir: args.models_dir.clone(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// main
// ═══════════════════════════════════════════════════════════════════════════

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let opts = RunOptions::from(&args);

    let input_path = Path::new(&args.input);

    if input_path.is_dir() {
        // 批量处理模式
        println!("[Batch Mode] Processing directory: {}", args.input);
        let extensions: Vec<&str> = args.extensions.split(',').map(|s| s.trim()).collect();

        let entries = collect_images(input_path, &extensions, args.recursive)?;

        if entries.is_empty() {
            println!("[Batch] No images found in: {}", args.input);
            return Ok(());
        }

        println!("[Batch] Found {} images to process", entries.len());

        let mut success_count = 0;
        let mut fail_count = 0;

        for entry in &entries {
            let entry_str = entry.as_str();
            println!("\n{} Processing: {}", "=".repeat(60), entry_str);
            match run_pipeline(entry_str, &args.output, &opts) {
                Ok(_) => success_count += 1,
                Err(e) => {
                    eprintln!("[ERROR] Failed to process {}: {}", entry_str, e);
                    fail_count += 1;
                }
            }
        }

        println!("\n{} Batch Complete {}", "=".repeat(25), "=".repeat(25));
        println!(
            "  Total: {}, Success: {}, Failed: {}",
            entries.len(),
            success_count,
            fail_count
        );
    } else if input_path.is_file() {
        // 单文件处理模式
        run_pipeline(&args.input, &args.output, &opts)?;
    } else {
        eprintln!("[ERROR] Input path does not exist: {}", args.input);
    }

    Ok(())
}

/// 收集目录下的图片文件（可选递归扫描子目录）
fn collect_images(dir: &Path, extensions: &[&str], recursive: bool) -> anyhow::Result<Vec<String>> {
    let mut images = Vec::new();

    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_str().unwrap_or("").to_lowercase();
                if extensions.contains(&ext_lower.as_str()) {
                    images.push(path.to_string_lossy().to_string());
                }
            }
        } else if recursive && path.is_dir() {
            let sub_images = collect_images(&path, extensions, true)?;
            images.extend(sub_images);
        }
    }

    images.sort();
    Ok(images)
}

// ═══════════════════════════════════════════════════════════════════════════
// 核心流水线（委托给 PipelineConfig，消除与 lib.rs 的重复实现）
// ═══════════════════════════════════════════════════════════════════════════

fn run_pipeline(img_path: &str, output_root: &str, opts: &RunOptions) -> anyhow::Result<()> {
    let t0 = Instant::now();

    // ── 统一流水线入口：用 RunOptions 构建 PipelineConfig ──
    // 后续所有核心逻辑均委托给 PipelineConfig，避免 main.rs 与 lib.rs 两套实现
    // 物体检测模型路径由 models_dir 按约定自动推导，消除冗余
    let models_dir = opts.models_dir.trim_end_matches('/');
    let pipeline_cfg = pipeline::PipelineConfig {
        ui_config: opts.cfg.clone(),
        models_dir: opts.models_dir.clone(),
        paragraph: opts.is_paragraph,
        remove_bar: opts.is_remove_bar,
        sub_component: opts.enable_sub_component,
        synthesize_text: opts.enable_synthesize,
        detect_model_path: format!("{}/object-detection/yolov8s-worldv2.onnx", models_dir),
        detect_labels_path: format!("{}/object-detection/yolov8s-worldv2_labels.txt", models_dir),
        detect_conf: opts.detect_conf,
    };

    // ── 初始化所有模型（缺失时自动从 Hugging Face 下载） ──
    if let Err(e) = quasivision::init_models(models_dir) {
        eprintln!("  [Warning] Model init failed (partial functionality):\n    {e}");
    }

    // ── 步骤 1: 读取图片 ──
    let t_step = Instant::now();
    println!("[Step 1/8] Reading image: {}", img_path);
    let (img, _gray) = pipeline_cfg
        .read_image(img_path)
        .with_context(|| format!("Failed to read image: {}", img_path))?;
    let img_shape = (img.height(), img.width());
    println!(
        "  → Image size: {} x {}  ({:.1}ms)",
        img_shape.1,
        img_shape.0,
        t_step.elapsed().as_secs_f64() * 1000.0
    );

    // ── 后台并行：OCR + 物体检测（与步骤 2-4 同时执行） ──
    let ocr_handle = if opts.enable_ocr {
        let img_for_ocr = img.clone();
        println!("  → OCR thread spawned (running in background)");
        Some(thread::spawn(move || {
            text_detection::detect_text(&img_for_ocr)
        }))
    } else {
        None
    };

    let object_detect_handle = if opts.enable_object_detect {
        let img_for_detect = img.clone();
        let model = pipeline_cfg.detect_model_path.clone();
        let labels = pipeline_cfg.detect_labels_path.clone();
        let conf = opts.detect_conf;
        println!("  → Object detection thread spawned (running in background)");
        Some(thread::spawn(move || {
            object_detector::run_object_detection(&img_for_detect, &model, &labels, conf)
        }))
    } else {
        None
    };

    // ── 步骤 2-4: 组件检测 + 规则分类 ──
    // 委托给 PipelineConfig::detect_components()
    // 内部依次执行：二值化 → 去线条 → CCL → 合并 → 过滤 → Block识别 → 嵌套组件 → 几何分类 → 颜色图标 → 子组件
    let t_step = Instant::now();
    println!("[Step 2-4/8] Component detection & classification...");

    let comps = pipeline_cfg.detect_components(&img)?;

    // 打印各类别统计
    let mut class_counts: HashMap<String, u32> = HashMap::new();
    for c in &comps {
        *class_counts.entry(c.category.clone()).or_insert(0) += 1;
    }
    for (k, v) in &class_counts {
        println!("  → {}: {}", k, v);
    }
    println!(
        "  → Total: {} components  ({:.1}ms)",
        comps.len(),
        t_step.elapsed().as_secs_f64() * 1000.0
    );

    // ── 步骤 5: OCR — 等待后台线程 ──
    let t_step = Instant::now();
    println!("[Step 5/8] Text detection...");
    let text_result = if let Some(handle) = ocr_handle {
        let result = handle.join().expect("OCR thread panicked");
        println!(
            "  → OCR result ready (waited {:.1}ms)",
            t_step.elapsed().as_secs_f64() * 1000.0
        );
        result
    } else {
        println!("  → OCR disabled by user");
        text_detection::TextResult { texts: Vec::new() }
    };
    println!("  → Found {} text elements", text_result.texts.len());

    // ── 步骤 6: 合并组件 + 文本 ──
    // 委托给 PipelineConfig::merge()
    // 内部已包含：合并 → 孤儿文本合成(option) → 颜色检测
    let t_step = Instant::now();
    println!("[Step 6/8] Merging components and texts...");
    let mut elements = pipeline_cfg.merge(&img, &comps, &text_result)?;
    println!(
        "  → Final: {} elements  ({:.1}ms)",
        elements.len(),
        t_step.elapsed().as_secs_f64() * 1000.0
    );

    // ── 步骤 6b2: 视觉重要性计算（仅 main.rs 特有） ──
    let t_sub = Instant::now();
    element::compute_prominence(&mut elements);
    let prominent_count = elements
        .iter()
        .filter(|e| e.prominence.map_or(false, |p| p >= 0.5))
        .count();
    println!(
        "  → 6b2/8. Prominence computed: {} prominent (≥0.5), {} total  ({:.1}ms)",
        prominent_count,
        elements.len(),
        t_sub.elapsed().as_secs_f64() * 1000.0
    );

    // ── 步骤 6c: Icon 含义识别（委托给 PipelineConfig） ──
    if opts.enable_icon_classify {
        let t_sub = Instant::now();
        let icon_count = elements.iter().filter(|e| e.class == "Icon").count();
        if icon_count > 0 {
            match pipeline_cfg.classify_icons(&img, &mut elements) {
                Ok(_) => {
                    println!(
                        "  → 6c/8. Icon classification done  ({:.1}ms)",
                        t_sub.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Err(e) => {
                    eprintln!(
                        "  → 6c/8. Failed to initialize IconClassifier: {} (skipping)",
                        e
                    );
                }
            }
        } else {
            println!(
                "  → 6c/8. No icons to classify  ({:.1}ms)",
                t_sub.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    // ── 步骤 6d: 物体检测 — 等待后台线程 ──
    let object_detections = if let Some(handle) = object_detect_handle {
        let t_sub = Instant::now();
        println!("  → 6d/8. Waiting for object detection...");
        let detections = handle.join().expect("Object detection thread panicked");
        println!(
            "  → 6d/8. Object detection: {} objects found  ({:.1}ms)",
            detections.len(),
            t_sub.elapsed().as_secs_f64() * 1000.0
        );
        detections
    } else {
        Vec::new()
    };

    // ── 步骤 7: 输出 ──
    let t_step = Instant::now();
    println!("[Step 8/8] Exporting results...");
    let img_name = Path::new(img_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let output_dir = Path::new(output_root).join(img_name);
    fs::create_dir_all(&output_dir)?;

    match opts.output_format.as_str() {
        "compact" => {
            let json_path = output_dir.join("elements.compact.json");
            export::save_compact_json(&elements, img_shape, json_path.to_str().unwrap())?;
        }
        "ai" => {
            let json_path = output_dir.join("elements.ai.json");
            export::save_ai_json(&elements, img_shape, json_path.to_str().unwrap())?;
        }
        "text" => {
            let txt_path = output_dir.join("elements.txt");
            export::save_text_summary(&elements, img_shape, txt_path.to_str().unwrap())?;
        }
        "tree" => {
            let json_path = output_dir.join("elements.tree.json");
            export::save_tree_json(&elements, img_shape, json_path.to_str().unwrap())?;
            let txt_path = output_dir.join("elements.tree.txt");
            export::save_tree_text(&elements, img_shape, txt_path.to_str().unwrap())?;
        }
        _ => {
            let json_path = output_dir.join("elements.json");
            export::save_json(&elements, img_shape, json_path.to_str().unwrap())?;
        }
    }

    // 物体检测结果输出
    let detection_roots = object_detector::build_detection_tree(&object_detections);
    if !detection_roots.is_empty() {
        match opts.output_format.as_str() {
            "tree" => {
                let det_tree_path = output_dir.join("objects.tree.json");
                export::save_detection_tree_json(
                    &detection_roots,
                    img_shape,
                    det_tree_path.to_str().unwrap(),
                )?;
                let det_txt_path = output_dir.join("objects.tree.txt");
                export::save_detection_tree_text(
                    &detection_roots,
                    img_shape,
                    det_txt_path.to_str().unwrap(),
                )?;
            }
            _ => {
                let det_path = output_dir.join("objects.json");
                export::save_detection_tree_json(
                    &detection_roots,
                    img_shape,
                    det_path.to_str().unwrap(),
                )?;
            }
        }
    } else if matches!(opts.output_format.as_str(), "tree") {
        let det_txt_path = output_dir.join("objects.tree.txt");
        export::save_detection_tree_text(
            &detection_roots,
            img_shape,
            det_txt_path.to_str().unwrap(),
        )?;
    }

    let vis_path = output_dir.join("visualization.jpg");
    export::save_visualization(&img, &elements, vis_path.to_str().unwrap())?;

    if !detection_roots.is_empty() {
        let det_vis_path = output_dir.join("objects.jpg");
        export::save_object_detection_visualization(
            &img,
            &detection_roots,
            det_vis_path.to_str().unwrap(),
        )?;
    }

    println!(
        "  → Export done  ({:.1}ms)",
        t_step.elapsed().as_secs_f64() * 1000.0
    );

    // ── 统计汇总 ──
    let mut compo_count = 0;
    let mut text_count = 0;
    let mut block_count = 0;
    let mut btn_count = 0;
    let mut img_count = 0;
    let mut icon_count = 0;
    for e in &elements {
        match e.class.as_str() {
            "Text" => text_count += 1,
            "Block" => block_count += 1,
            "Button" => btn_count += 1,
            "Image" => img_count += 1,
            "Icon" => icon_count += 1,
            _ => compo_count += 1,
        }
    }
    println!();
    println!("=== Result Summary ===");
    println!("  Blocks: {}", block_count);
    println!("  Buttons: {}", btn_count);
    println!("  Icons: {}", icon_count);
    println!("  Images: {}", img_count);
    println!("  Texts: {}", text_count);
    println!("  Components: {}", compo_count);
    println!("  Total: {}", elements.len());
    if !object_detections.is_empty() {
        println!("  Objects detected: {}", object_detections.len());
    }
    println!("  Output: {}", output_dir.display());
    println!("  ⏱ Total time: {:.1}s", t0.elapsed().as_secs_f64());

    Ok(())
}
