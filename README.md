<h1 align="center">quasivision</h1>

<p align="center">
  <a href="https://github.com/WeiChens/quasivision/stargazers"><img src="https://img.shields.io/github/stars/WeiChens/quasivision?style=for-the-badge&logo=github" alt="Stars"></a>
  <a href="https://github.com/WeiChens/quasivision/network/members"><img src="https://img.shields.io/github/forks/WeiChens/quasivision?style=for-the-badge&logo=github" alt="Forks"></a>
  <a href="https://github.com/WeiChens/quasivision/issues"><img src="https://img.shields.io/github/issues/WeiChens/quasivision?style=for-the-badge&logo=github" alt="Issues"></a>
  <a href="https://github.com/WeiChens/quasivision/blob/main/LICENSE"><img src="https://img.shields.io/github/license/WeiChens/quasivision?style=for-the-badge" alt="License"></a>
</p>
<p align="center">
  <a href="README.md">🇬🇧 English</a> · <a href="README-zh.md">🇨🇳 中文</a>
</p>

A Rust-based pseudo-visual understanding tool.  
Analyzes screenshots, UI mockups, and real-world photos — detects UI components (buttons, text fields, icons, images, etc.), recognizes text via OCR, identifies 254 classes of everyday objects (people, cars, phones, food, etc.) with YOLO-World, classifies 81 types of icon meanings, and outputs structured descriptions with visual annotations.

---

## 📋 Table of Contents

1. [Quick Start](#quick-start)
2. [Demo Gallery](#demo-gallery)
3. [Output Overview](#output-overview)
4. [CLI Reference](#cli-reference)
5. [Output File Structure](#output-file-structure)
6. [Pipeline](#pipeline)
7. [Core Features](#core-features)
8. [FAQ](#faq)
9. [Practical Examples](#practical-examples)
10. [Proxy Configuration](#proxy-configuration)
11. [License](#license)

---

## 🚀 Quick Start <a id="quick-start"></a>

### Basic Usage

```bash
# Single image
cargo run -- --input image.png

# Try with the built-in demo
cargo run -- --input demo/ui.jpg

# Custom output directory
cargo run -- --input image.png --output ./result

# Batch process all images in a directory
cargo run -- --input ./screenshots/

# Recursive processing (include subdirectories)
cargo run -- --input ./screenshots/ --recursive
```

### Minimal Example

```bash
cargo run -- --input demo/ui.jpg
```

Results are written to `./output/ui/`.

---

## 🖼️ Demo Gallery <a id="demo-gallery"></a>

### 1. UI Detection — Web Search Page

|          Input           |                   Output                    |
| :----------------------: | :-----------------------------------------: |
| ![ui-input](demo/ui.jpg) | ![ui-viz](demo/output/ui/visualization.jpg) |

Detected UI components including text, icons, buttons, and structured blocks from a search result page — with full OCR text extraction.

### 2. Object Detection — Real-World Photo

|               Input                |                     Output                      |
| :--------------------------------: | :---------------------------------------------: |
| ![reality-input](demo/reality.jpg) | ![reality-viz](demo/output/reality/objects.jpg) |

Detected objects with hierarchical relationships (woman → child → hat/glasses/dress/coat), visualized with bounding boxes and labels.

**Detection result:**

```
Objects (474×714) — 7 found:
├─ woman (54%)
   └─ child (21%)
      ├─ hat (50%)
      │  └─ face (39%)
      │     └─ glasses (32%)
      └─ dress (24%)
         └─ coat (41%)
```

### 3. Mixed Scenario — Stock Photo Gallery

|                 Input                 |                       Output (UI)                       |                  Output (Objects)                  |
| :-----------------------------------: | :-----------------------------------------------------: | :------------------------------------------------: |
| ![mixed-input](demo/realityAndUi.jpg) | ![mixed-ui](demo/output/realityAndUi/visualization.jpg) | ![mixed-obj](demo/output/realityAndUi/objects.jpg) |

A stock photo gallery page: UI detection extracts the layout structure (image grid, navigation bar, text labels), while object detection identifies photo subjects (people, faces, etc.).

---

## 📤 Output Overview <a id="output-overview"></a>

### 5 Output Formats (`--format` flag)

```
--format standard    Full JSON with id/parent/children hierarchy
         compact     Minified JSON (short keys, ~50% smaller)
         ai          Normalized coordinates (0-1000), LLM-friendly
         text        Plain text summary, ready to paste into prompts
         tree        [Recommended] Nested tree structure, AI-readable DOM
```

**`tree` format is recommended** — it generates both `elements.tree.json` and `elements.tree.txt`.

### Output Files

| File                                   | Source        | Description                                         |
| -------------------------------------- | ------------- | --------------------------------------------------- |
| `elements.tree.json` / `elements.json` | UI Detection  | All detected UI components (buttons/text/icons/etc) |
| `elements.tree.txt` / `elements.txt`   | UI Detection  | Plain text summary                                  |
| `visualization.jpg`                    | UI Detection  | Annotated image with color-coded component borders  |
| `objects.tree.json` / `objects.json`   | Object Detect | YOLO-detected objects (254 classes) with hierarchy  |
| `objects.tree.txt`                     | Object Detect | Object detection plain text summary                 |
| `objects.jpg`                          | Object Detect | Object detection visualization with labels          |

---

## ⚙️ CLI Reference <a id="cli-reference"></a>

### Basic Options

| Argument       | Type   | Default             | Description                                            |
| -------------- | ------ | ------------------- | ------------------------------------------------------ |
| `-i, --input`  | String | **Required**        | Input image path or directory                          |
| `-o, --output` | String | `output`            | Output root directory                                  |
| `--format`     | String | `tree`              | Output format: `standard`/`compact`/`ai`/`text`/`tree` |
| `--recursive`  | bool   | `false`             | Recursively process subdirectories                     |
| `--extensions` | String | `png,jpg,jpeg,jfif` | Comma-separated image file extensions                  |

### UI Detection Options

| Argument            | Type | Default | Description                                      |
| ------------------- | ---- | ------- | ------------------------------------------------ |
| `--gradient`        | u8   | `4`     | Gradient threshold (dribbble/rico: 4, web: 1)    |
| `--min-area`        | u32  | `55`    | Minimum connected component area                 |
| `--paragraph`       | bool | `false` | Enable paragraph merging                         |
| `--remove-bar`      | bool | `true`  | Remove top/bottom navigation bars                |
| `--sub-component`   | bool | `true`  | Detect sub-components (buttons inside images)    |
| `--synthesize-text` | bool | `true`  | Auto-synthesize container blocks for orphan text |

### Line / Rectangle Options

| Argument            | Type | Default | Description                                                |
| ------------------- | ---- | ------- | ---------------------------------------------------------- |
| `--line-thickness`  | u32  | `8`     | Maximum line thickness (pixels)                            |
| `--line-min-length` | f64  | `0.95`  | Minimum line length ratio                                  |
| `--rec-evenness`    | f64  | `0.7`   | Minimum rectangle evenness                                 |
| `--rec-dent`        | f64  | `0.25`  | Maximum rectangle dent ratio                               |
| `--rec-corner-skip` | f64  | `0.08`  | Corner tolerance (0=strict right angle, 0.08~0.12=rounded) |

### Block Detection Options

| Argument       | Type | Default | Description                                |
| -------------- | ---- | ------- | ------------------------------------------ |
| `--block-side` | f64  | `0.15`  | Block side length ratio threshold          |
| `--block-grad` | u8   | `5`     | Block nesting detection gradient threshold |

### Text Options

| Argument       | Type | Default | Description                                      |
| -------------- | ---- | ------- | ------------------------------------------------ |
| `--text-max-h` | f64  | `0.08`  | Max text height ratio (relative to image height) |
| `--text-gap`   | u32  | `10`    | Max word gap (pixels)                            |
| `--ocr`        | bool | `true`  | Enable OCR text recognition                      |

### Icon / Object Detection Options

| Argument          | Type   | Default                                                 | Description                          |
| ----------------- | ------ | ------------------------------------------------------- | ------------------------------------ |
| `--icon-classify` | bool   | `true`                                                  | Enable icon meaning classification   |
| `--object-detect` | bool   | `true`                                                  | Enable object detection              |
| `--detect-model`  | String | `resources/object-detection/yolov8s-worldv2.onnx`       | YOLO model path                      |
| `--detect-labels` | String | `resources/object-detection/yolov8s-worldv2_labels.txt` | YOLO labels file path                |
| `--detect-conf`   | f32    | `0.2`                                                   | Detection confidence threshold (0~1) |
| `--models-dir`    | String | `resources`                                             | Model resource root directory        |

### Disabling Features

```bash
# Disable OCR (structure-only detection)
cargo run -- --input image.png --ocr false

# Disable object detection
cargo run -- --input image.png --object-detect false

# Disable icon classification
cargo run -- --input image.png --icon-classify false

# UI detection only (all optional features off)
cargo run -- --input image.png --ocr false --object-detect false --icon-classify false
```

---

## 📁 Output File Structure <a id="output-file-structure"></a>

### Single Image Output

```
output/
└── image_name/             # Named after the input file (without extension)
    ├── elements.tree.json  # UI element tree (JSON)
    ├── elements.tree.txt   # UI element tree (text)
    ├── visualization.jpg   # UI detection visualization
    ├── objects.tree.json   # Object detection tree (JSON)
    ├── objects.tree.txt    # Object detection tree (text)
    └── objects.jpg         # Object detection visualization
```

> Note: `objects.*` files are only generated when `--object-detect true` and objects are found.

---

## 🔄 Pipeline <a id="pipeline"></a>

```
Input Image
  │
  ├─ 1. Preprocessing ────── Grayscale, line removal, background removal
  │
  ├─ 2. Connected Component ─ Gradient → CCL (Connected Component Labeling)
  │
  ├─ 3. Rect/Line Detection ─ Buttons, input fields, etc.
  │
  ├─ 4. Merge & Filter ───── Merge overlapping regions, remove noise
  │
  ├─ 5. Classification ───── Block / Button / Text / Icon / Image
  │      │
  │      ├─ Icon Classifier ── 81 common icon categories (ONNX Runtime)
  │      │
  │      └─ OCR (background) ─ Text recognition (PaddleOCR)
  │
  ├─ 6. Merge ────────────── Merge OCR text into UI elements
  │
  ├─ 7. Color Detection ──── Extract background/foreground colors
  │
  └─ 8. Output ───────────── 5 formats + visualization annotation
```

### Parallel Execution

Object detection (YOLO-World) and OCR run on **background threads** in parallel with the main pipeline, adding no extra wait time.

---

## 🧩 Core Features <a id="core-features"></a>

### 1. UI Element Detection (Main Feature)

Detects 7 types of UI elements:

| Category      | Description                                    |
| ------------- | ---------------------------------------------- |
| **Block**     | Container blocks (cards, list items, nav bars) |
| **Button**    | Clickable buttons                              |
| **Text**      | Text labels                                    |
| **Icon**      | Icons (small square elements)                  |
| **Image**     | Images                                         |
| **Input**     | Input fields                                   |
| **List Item** | List items (with checkmark indicators)         |

### 2. OCR Text Recognition

- Based on PaddleOCR (PP-OCRv5) models
- Windows: DirectML GPU acceleration supported
- Auto-detects text in images
- Long text protection: meaningful text (>5 chars) bypasses height filters

### 3. Object Detection (YOLO-World)

- ONNX Runtime-based YOLO-World model
- 254 common object classes (people, cars, phones, food, animals, etc.)
- Auto-builds parent-child containment trees
- Outputs annotated `objects.jpg` visualization

### 4. Icon Meaning Classification

- 81-class icon classification via ONNX model
- Common UI icon meanings (settings, search, share, back, etc.)
- Confidence > 40% displays candidate meanings

### 5. Color Detection

- Auto-extracts background/foreground colors per element
- Outputs hex color values

---

## ❓ FAQ <a id="faq"></a>

### Q: Where do model files come from?

Model files are located in the `resources/` directory:

```
resources/
├── ocr-models/
│   ├── ppocrv5_mobile_det.onnx   # OCR detection model
│   ├── ppocrv5_mobile_rec.onnx   # OCR recognition model
│   └── ppocrv5_dict.txt          # Chinese dictionary
├── icon-classifier/
│   ├── icon_classifier.onnx      # Icon classification model
│   └── labels.json               # 81 class labels
└── object-detection/
    ├── yolov8s-worldv2.onnx      # YOLO object detection model
    └── yolov8s-worldv2_labels.txt # 254 class labels
```

**Auto-download**: Missing model files are automatically downloaded from the Hugging Face repo ([WeiChens/quasivision-models](https://huggingface.co/WeiChens/quasivision-models)) on first run.

**Mirror for China users**:

```bash
set QUASIVISION_MODELS_URL=https://hf-mirror.com/WeiChens/quasivision-models/resolve/main
cargo run -- --input image.png
```

### Q: What coordinate system does the output use?

Default output uses raw pixel coordinates:

```json
{
  "column_min": 100,
  "row_min": 200,
  "column_max": 300,
  "row_max": 400
}
```

Use `--format ai` for normalized coordinates (0–1000).

### Q: How can I run object detection only (without UI detection)?

The current design runs the full pipeline. You can disable ancillary features with `--ocr false --icon-classify false`.

### Q: How to improve detection quality?

- **Gradient threshold**: Web pages: `--gradient 1`, App screenshots: `--gradient 4`
- **Rounded corners**: Use `--rec-corner-skip 0.12` for large rounded elements
- **Small text**: Increase `--text-max-h 0.12` to raise text height limit

### Q: What confidence threshold should I use?

| Scenario                        | `--detect-conf` Recommended |
| ------------------------------- | :-------------------------: |
| Only high-confidence objects    |             0.5             |
| Balanced precision & recall     |        0.2 (default)        |
| Maximum recall (tolerate noise) |             0.1             |

### Q: Supported image formats?

Default: `png`, `jpg`, `jpeg`, `jfif`. Customize with `--extensions`.

---

## 💡 Practical Examples <a id="practical-examples"></a>

```bash
# App screenshot (recommended parameters)
cargo run -- -i app.png --gradient 4 --format tree

# Web page detection
cargo run -- -i webpage.png --gradient 1 --rec-corner-skip 0.1

# Batch processing with recursion
cargo run -- -i ./screenshots/ --recursive --format tree

# AI-friendly output, disable non-essential features
cargo run -- -i ui.png --format ai --icon-classify false

# High-recall detection
cargo run -- -i photo.jpg --detect-conf 0.1 --format tree

# Paragraph-aware text detection
cargo run -- -i document.png --paragraph true --text-max-h 0.15
```

---

## 🌐 Proxy Configuration <a id="proxy-configuration"></a>

On Windows, quasivision automatically detects system proxy settings (compatible with Clash, V2Ray, etc.). If your proxy requires manual configuration:

```bash
# Windows (cmd)
set HTTP_PROXY=http://127.0.0.1:7890
set HTTPS_PROXY=http://127.0.0.1:7890
cargo run -- --input image.png

# macOS / Linux
HTTP_PROXY=http://127.0.0.1:7890 HTTPS_PROXY=http://127.0.0.1:7890 cargo run -- --input image.png
```

---

## 📄 License <a id="license"></a>

- **Source code**: MIT © quasivision
- **PP-OCRv5**: Apache 2.0 © PaddlePaddle
- **YOLOv8s-worldv2**: AGPL-3.0 © Ultralytics
- **Icon Classifier**: MIT

---

## 📖 Also Available In

- [中文文档 (Chinese)](README-zh.md)
