# quasivision 使用帮助

> **quasivision** — 基于 Rust 的伪视觉理解工具。  
> 从截图/UI 设计稿中自动检测 UI 组件（按钮、文本框、图标、图片等），执行 OCR 文字识别、物体检测、Icon 含义识别，并输出结构化描述。

---

## 📋 目录

1. [快速开始](#快速开始)
2. [输出内容说明](#输出内容说明)
3. [命令行参数详解](#命令行参数详解)
4. [输出文件一览](#输出文件一览)
5. [检测流程](#检测流程)
6. [核心功能](#核心功能)
7. [常见问题](#常见问题)

---

## 🚀 快速开始

### 基本用法

```bash
# 单图检测
cargo run -- --input 图片.png

# 指定输出目录
cargo run -- --input 图片.png --output ./result

# 批量处理目录中所有图片
cargo run -- --input ./screenshots/

# 递归处理子目录
cargo run -- --input ./screenshots/ --recursive
```

### 最简示例

```bash
cargo run -- --input 9.png
```

输出到 `./output/9/` 目录下，包含检测结果文件。

---

## 📤 输出内容说明

### 5 种输出格式（`--format` 参数）

```
--format standard    完整 JSON（含 id/parent/children 层级关系）
         compact     压缩 JSON（短键名，体积小 ~50%）
         ai          归一化坐标（0-1000），适合 LLM 输入
         text        纯文本摘要，适合直接粘贴到 prompt
         tree        [推荐] 树形嵌套结构，AI 一眼看懂 DOM 层级
```

**推荐使用 `tree` 格式**，同时输出 `elements.tree.json` 和 `elements.tree.txt`。

### 输出文件列表

| 文件                                   | 来源        | 说明                                               |
| -------------------------------------- | ----------- | -------------------------------------------------- |
| `elements.tree.json` / `elements.json` | UI 元素检测 | 检测到的所有 UI 组件（按钮/文本/图标/Block 等）    |
| `elements.tree.txt` / `elements.txt`   | UI 元素检测 | 纯文本格式摘要                                     |
| `visualization.jpg`                    | UI 元素检测 | 可视化标注图（各组件用不同颜色边框标记）           |
| `objects.tree.json` / `objects.json`   | 物体检测    | YOLO 检测的物体（人/车/手机等 254 类），含父子层级 |
| `objects.tree.txt`                     | 物体检测    | 物体检测纯文本格式                                 |
| `objects.jpg`                          | 物体检测    | 物体检测可视化标注图（带标签）                     |

---

## ⚙️ 命令行参数详解

### 基础参数

| 参数           | 类型   | 默认值              | 说明                                                      |
| -------------- | ------ | ------------------- | --------------------------------------------------------- |
| `-i, --input`  | String | **必填**            | 输入图片路径或目录                                        |
| `-o, --output` | String | `output`            | 输出根目录                                                |
| `--format`     | String | `tree`              | 输出格式：`standard` / `compact` / `ai` / `text` / `tree` |
| `--recursive`  | bool   | `false`             | 递归处理子目录中的图片                                    |
| `--extensions` | String | `png,jpg,jpeg,jfif` | 图片扩展名过滤（逗号分隔）                                |

### UI 检测参数

| 参数                | 类型 | 默认值  | 说明                                   |
| ------------------- | ---- | ------- | -------------------------------------- |
| `--gradient`        | u8   | `4`     | 梯度阈值（dribbble/rico: 4, web: 1）   |
| `--min-area`        | u32  | `55`    | 最小连通区域面积                       |
| `--paragraph`       | bool | `false` | 是否启用段落合并                       |
| `--remove-bar`      | bool | `true`  | 是否移除顶栏/底栏                      |
| `--sub-component`   | bool | `true`  | 是否启用子组件检测（图片内部按钮检测） |
| `--synthesize-text` | bool | `true`  | 是否为孤儿文本自动合成容器 Block       |

### 线条 / 矩形参数

| 参数                | 类型 | 默认值 | 说明                                         |
| ------------------- | ---- | ------ | -------------------------------------------- |
| `--line-thickness`  | u32  | `8`    | 线条最大粗细（像素）                         |
| `--line-min-length` | f64  | `0.95` | 线条最小长度比例                             |
| `--rec-evenness`    | f64  | `0.7`  | 矩形最小平整度                               |
| `--rec-dent`        | f64  | `0.25` | 矩形最大凹陷比                               |
| `--rec-corner-skip` | f64  | `0.08` | 圆角容错（0=严格直角，0.08~0.12=识别大圆角） |

### Block 检测参数

| 参数           | 类型 | 默认值 | 说明                   |
| -------------- | ---- | ------ | ---------------------- |
| `--block-side` | f64  | `0.15` | Block 边长占比阈值     |
| `--block-grad` | u8   | `5`    | Block 嵌套检测梯度阈值 |

### 文本参数

| 参数           | 类型 | 默认值 | 说明                           |
| -------------- | ---- | ------ | ------------------------------ |
| `--text-max-h` | f64  | `0.08` | 文本最大高度比（相对图片高度） |
| `--text-gap`   | u32  | `10`   | 文本单词最大间距（像素）       |
| `--ocr`        | bool | `true` | 是否启用 OCR 文字识别          |

### Icon / 物体检测参数

| 参数              | 类型   | 默认值                                                  | 说明                      |
| ----------------- | ------ | ------------------------------------------------------- | ------------------------- |
| `--icon-classify` | bool   | `true`                                                  | 是否启用 Icon 含义识别    |
| `--object-detect` | bool   | `true`                                                  | 是否启用物体检测          |
| `--detect-model`  | String | `resources/object-detection/yolov8s-worldv2.onnx`       | YOLO 模型路径             |
| `--detect-labels` | String | `resources/object-detection/yolov8s-worldv2_labels.txt` | YOLO 标签文件路径         |
| `--detect-conf`   | f32    | `0.2`                                                   | 物体检测置信度阈值（0~1） |
| `--models-dir`    | String | `resources`                                             | 模型资源根目录            |

### 关闭特定功能

```bash
# 关闭 OCR（仅做 UI 结构检测，不识别文字）
cargo run -- --input 图片.png --ocr false

# 关闭物体检测
cargo run -- --input 图片.png --object-detect false

# 关闭 Icon 含义识别
cargo run -- --input 图片.png --icon-classify false

# 仅做 UI 检测（全部可选功能关闭）
cargo run -- --input 图片.png --ocr false --object-detect false --icon-classify false
```

---

## 📁 输出文件一览

### 单张图片的输出目录结构

```
output/
└── 图片名/                  # 以图片文件名（不含扩展名）命名
    ├── elements.tree.json   # UI 元素树（JSON 格式）
    ├── elements.tree.txt    # UI 元素树（文本格式）
    ├── visualization.jpg    # UI 检测可视化图
    ├── objects.tree.json    # 物体检测树（JSON 格式）
    ├── objects.tree.txt     # 物体检测树（文本格式）
    └── objects.jpg          # 物体检测可视化图
```

> 注意：`objects.*` 文件仅在 `--object-detect true` 且检测到物体时生成。

---

## 🔄 检测流程

```
输入图片
  │
  ├─ 1. 预处理 ─────────── 灰度化、去线条、去背景
  │
  ├─ 2. 连通区域检测 ───── 梯度计算 → CCL 连通域
  │
  ├─ 3. 矩形/线条检测 ──── 识别按钮、输入框等规则形状
  │
  ├─ 4. 合并过滤 ───────── 合并重叠区域、过滤噪声
  │
  ├─ 5. 规则分类 ───────── Block / Button / Text / Icon / Image
  │      │
  │      ├─ Icon 分类器 ── 81 类常见 Icon 含义（ONNX Runtime）
  │      │
  │      └─ OCR（后台） ── 文本识别（PaddleOCR 模型）
  │
  ├─ 6. 合并 ───────────── OCR 文本合并到 UI 元素
  │
  ├─ 7. 颜色检测 ───────── 提取各元素的背景/前景色
  │
  └─ 8. 输出 ───────────── 5 种格式 + 可视化标注
```

### 并行执行

物体检测（YOLO-World）和 OCR 在**后台线程**中与主流程并行执行，不增加额外等待时间。

---

## 🧩 核心功能

### 1. UI 元素检测（主体功能）

检测 7 类 UI 元素：

| 类别          | 说明                               |
| ------------- | ---------------------------------- |
| **Block**     | 容器区块（卡片、列表项、导航栏等） |
| **Button**    | 可点击按钮                         |
| **Text**      | 文字标签                           |
| **Icon**      | 图标（小尺寸方形元素）             |
| **Image**     | 图片                               |
| **Input**     | 输入框                             |
| **List Item** | 列表项（带勾选标记）               |

### 2. OCR 文字识别

- 基于 PaddleOCR 模型（PP-OCRv5）
- Windows 平台支持 DirectML GPU 加速
- 自动检测图片中的文字内容
- 大文本保护：有意义的较长文字（>5 字符）不受高度限制过滤

### 3. 物体检测（YOLO-World）

- 基于 ONNX Runtime 的 YOLO-World 模型
- 254 类常见物体识别（人、车、手机、食物、动物等）
- 自动构建父子包含关系树
- 输出可视化标注图 `objects.jpg`

### 4. Icon 含义识别

- 基于 ONNX 模型的 81 类 Icon 分类
- 常见 UI Icon 含义识别（设置、搜索、分享、返回等）
- 置信度 > 40% 显示候选含义

### 5. 颜色检测

- 自动提取各元素的背景/前景色
- 输出十六进制颜色值

---

## ❓ 常见问题

### Q: 模型文件从哪里获取？

模型文件位于 `resources/` 目录下：

```
resources/
├── ocr-models/
│   ├── ppocrv5_mobile_det.onnx   # OCR 检测模型
│   ├── ppocrv5_mobile_rec.onnx   # OCR 识别模型
│   └── ppocrv5_dict.txt          # 中文字典
├── icon-classifier/
│   ├── icon_classifier.onnx      # Icon 分类模型
│   └── labels.json               # 81 类标签
└── object-detection/
    ├── yolov8s-worldv2.onnx      # YOLO 物体检测模型
    └── yolov8s-worldv2_labels.txt # 254 类标签
```

### Q: 输出结果坐标是多少？

默认输出原始图片像素坐标，格式为：

```json
{
  "column_min": 100, // 左上角 x
  "row_min": 200, // 左上角 y
  "column_max": 300, // 右下角 x
  "row_max": 400 // 右下角 y
}
```

使用 `--format ai` 输出归一化坐标（0~1000）。

### Q: 如何只检测物体（不做 UI 检测）？

当前设计为全流水线运行，无法单独运行物体检测。可以通过设置 `--ocr false --icon-classify false` 关闭附属功能。

### Q: 如何提高检测质量？

- **梯度阈值**：网页截图用 `--gradient 1`，App 截图用 `--gradient 4`
- **圆角识别**：大圆角元素用 `--rec-corner-skip 0.12`
- **文本识别**：小字体用 `--text-max-h 0.12` 提高文本高度上限

### Q: 置信度阈值调多少合适？

| 场景                         | `--detect-conf` 建议值 |
| ---------------------------- | :--------------------: |
| 只想看到高置信度物体         |          0.5           |
| 平衡查准率和查全率           |      0.2（默认）       |
| 尽量多地检测物体（容忍误检） |          0.1           |

### Q: 支持的图片格式？

默认支持 `png`、`jpg`、`jpeg`、`jfif`。可通过 `--extensions` 自定义。

---

## 💡 实用示例

```bash
# App 截图检测（推荐参数）
cargo run -- -i app.png --gradient 4 --format tree

# Web 页面检测
cargo run -- -i webpage.png --gradient 1 --rec-corner-skip 0.1

# 批量处理 + 递归子目录
cargo run -- -i ./screenshots/ --recursive --format tree

# AI 友好输出 + 关闭不必要的功能
cargo run -- -i ui.png --format ai --icon-classify false

# 高精度检测（调低阈值，更多物体）
cargo run -- -i photo.jpg --detect-conf 0.1 --format tree

# 带段落合并的文本检测
cargo run -- -i document.png --paragraph true --text-max-h 0.15
```

---

> **项目地址**：`E:/code/quasivision`  
> **Cargo 运行**：确保在项目根目录下执行 `cargo run -- ...`

## License

- **源代码**: MIT © quasivision
- **PP-OCRv5**: Apache 2.0 © PaddlePaddle
- **YOLOv8s-worldv2**: AGPL-3.0 © Ultralytics
- **Icon Classifier**: MIT
