use serde::{Deserialize, Serialize};
use crate::bbox::Bbox;

/// 最终检测到的 UI 元素
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    pub id: usize,
    pub class: String,
    pub height: i32,
    pub width: i32,
    pub position: ElementPosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    /// 颜色值（十六进制 "#RRGGBB"）
    /// - Text/Icon：主体（前景）颜色
    /// - 其他：背景颜色
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// 视觉重要性分数 0.0~1.0
    /// 由 compute_prominence() 计算，基于面积、类别、颜色对比度
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prominence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ElementPosition {
    pub column_min: i32,
    pub row_min: i32,
    pub column_max: i32,
    pub row_max: i32,
}

impl Element {
    pub fn new(id: usize, bbox: &Bbox, class: &str, text_content: Option<String>) -> Self {
        Self {
            id,
            class: class.to_string(),
            height: bbox.height(),
            width: bbox.width(),
            position: ElementPosition {
                column_min: bbox.col_min,
                row_min: bbox.row_min,
                column_max: bbox.col_max,
                row_max: bbox.row_max,
            },
            text_content,
            color: None,
            prominence: None,
            children: None,
            parent: None,
        }
    }

    /// 从 bbox 和类别快速创建
    pub fn from_parts(id: usize, col_min: i32, row_min: i32, col_max: i32, row_max: i32, class: &str) -> Self {
        let bbox = Bbox::new(col_min, row_min, col_max, row_max);
        Self::new(id, &bbox, class, None)
    }

    pub fn put_bbox(&self) -> (i32, i32, i32, i32) {
        (self.position.column_min, self.position.row_min, self.position.column_max, self.position.row_max)
    }

    pub fn area(&self) -> i64 {
        self.width as i64 * self.height as i64
    }

    /// 计算与另一个元素的交集面积和 IoU
    pub fn calc_intersection(&self, other: &Element, bias: (i32, i32)) -> (i64, f64, f64, f64) {
        let (c1, r1, c2, r2) = self.put_bbox();
        let (c3, r3, c4, r4) = other.put_bbox();

        let inter_col_min = c1.max(c3) - bias.0;
        let inter_row_min = r1.max(r3) - bias.1;
        let inter_col_max = c2.min(c4);
        let inter_row_max = r2.min(r4);

        let w = (inter_col_max - inter_col_min).max(0);
        let h = (inter_row_max - inter_row_min).max(0);
        let inter = w as i64 * h as i64;

        if inter == 0 {
            return (0, 0.0, 0.0, 0.0);
        }

        let union = self.area() + other.area() - inter;
        let iou = if union > 0 { inter as f64 / union as f64 } else { 0.0 };
        let ioa = inter as f64 / self.area() as f64;
        let iob = inter as f64 / other.area() as f64;

        (inter, iou, ioa, iob)
    }

    /// 判断两个元素的关系
    pub fn element_relation(&self, other: &Element, bias: (i32, i32)) -> i32 {
        let (_, _, ioa, iob) = self.calc_intersection(other, bias);

        if ioa == 0.0 && iob == 0.0 {
            return 0;
        }
        // self in other
        if ioa >= 1.0 {
            return -1;
        }
        // other in self
        if iob >= 1.0 {
            return 1;
        }
        2
    }

    /// 合并两个元素
    pub fn element_merge(&mut self, other: &Element) {
        let (c1, r1, c2, r2) = self.put_bbox();
        let (c3, r3, c4, r4) = other.put_bbox();

        self.position.column_min = c1.min(c3);
        self.position.row_min = r1.min(r3);
        self.position.column_max = c2.max(c4);
        self.position.row_max = r2.max(r4);

        self.width = self.position.column_max - self.position.column_min;
        self.height = self.position.row_max - self.position.row_min;

        // 合并文本内容
        if other.text_content.is_some() {
            let other_text = other.text_content.as_deref().unwrap_or("");
            match &self.text_content {
                Some(t) if !t.is_empty() => {
                    self.text_content = Some(format!("{}\n{}", t, other_text));
                }
                _ => {
                    self.text_content = other.text_content.clone();
                }
            }
        }
    }
}

// ── 视觉重要性分数计算 ──

/// 解析颜色字符串 "bg(#RRGGBB)" 或 "fg(#RRGGBB)"，返回 (r, g, b) 和是否为前景色
fn parse_color_str(s: &str) -> Option<(u8, u8, u8, bool)> {
    // 格式: "bg(#RRGGBB)" 或 "fg(#RRGGBB)"
    if s.len() < 11 {
        return None;
    }
    let is_fg = s.starts_with("fg(");
    let is_bg = s.starts_with("bg(");
    if !is_fg && !is_bg {
        return None;
    }
    // 提取 RRGGBB
    let hex_start = if is_fg { 4 } else { 4 };
    let hex = &s[hex_start..hex_start + 6];
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b, is_fg))
}

/// 计算颜色的相对亮度（WCAG 标准）
fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let srgb = |c: u8| -> f64 {
        let c = c as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * srgb(r) + 0.7152 * srgb(g) + 0.0722 * srgb(b)
}

/// 为所有元素计算视觉重要性分数 (0.0 ~ 1.0)
///
/// 基于三个确定性指标加权：
/// - **面积**: 相对于最大元素的面积比（越大越重要）
/// - **类别**: 不同 UI 类别的 base 重要性
/// - **颜色对比度**: 颜色与白色背景的差异度（越显眼越重要）
///
/// 输出标签阈值：≥0.50 → `[Primary]`, ≥0.40 → `[Secondary]`, <0.40 → 省略
/// 这是一个纯规则的连续分数，LLM 可以自行决定阈值。
pub fn compute_prominence(elements: &mut [Element]) {
    if elements.is_empty() {
        return;
    }

    // 1. 最大面积 & 最大高度（归一化基准）
    let max_area = elements
        .iter()
        .map(|e| e.area())
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let max_height = elements
        .iter()
        .map(|e| e.height as f64)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(1.0)
        .max(1.0);

    // 2. 类别基础分
    let class_base = |class: &str| -> f64 {
        match class {
            "Image" => 0.30,  // 图片通常是主要内容
            "Button" => 0.25, // 按钮是交互核心
            "Block" => 0.20,  // 容器承载内容
            "Compo" => 0.20,  // 复合组件
            "Text" => 0.15,   // 文字
            "Icon" => 0.10,   // 图标通常较小
            _ => 0.15,
        }
    };

    for element in elements.iter_mut() {
        // 面积分 (0 ~ 0.5)
        // Text：用 height（≈字号），避免长文本因字数多而误判为更显眼
        // 非 Text：用 width×height（真实面积）
        let area_ratio = if element.class == "Text" {
            (element.height as f64 / max_height).min(1.0)
        } else {
            (element.area() as f64 / max_area).min(1.0)
        };
        let area_score = area_ratio * 0.5;

        // 类别分 (0 ~ 0.3)
        let type_score = class_base(&element.class) * 0.3 / 0.30; // normalize to 0~0.3 range
        // Image=0.30 → 0.30, Button=0.25 → 0.25, ..., Icon=0.10 → 0.10

        // 颜色对比度分 (0 ~ 0.2)
        let contrast_score = if let Some(ref color_str) = element.color {
            if let Some((r, g, b, is_fg)) = parse_color_str(color_str) {
                if is_fg {
                    // 前景色（Text/Icon）：对比度取决于父容器背景，这里用默认值
                    // 真实显眼度由父容器的 bg 色体现
                    0.10
                } else {
                    // 背景色（Block/Button/Compo）：与白色对比
                    let lum = relative_luminance(r, g, b);
                    let contrast = 1.0 - lum; // 越深 → 越显眼
                    contrast * 0.2
                }
            } else {
                0.10 // 默认中等对比度
            }
        } else {
            0.10 // 无颜色信息，默认中等
        };

        let raw = area_score + type_score + contrast_score;
        // 保留两位小数（JSON 输出用）
        element.prominence = Some((raw * 100.0).round() / 100.0);
    }
}

/// 将 prominence 分数转为文本标签（tree text 输出用）
///
/// - ≥ 0.50 → `[Primary]`
/// - ≥ 0.40 → `[Secondary]`
/// - < 0.40 → `""`（省略，不污染输出）
pub fn prominence_label(p: f64) -> &'static str {
    if p >= 0.50 {
        " [Primary]"
    } else if p >= 0.40 {
        " [Secondary]"
    } else {
        ""
    }
}

/// 输出结果结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputResult {
    pub comps: Vec<Element>,
    pub img_shape: (u32, u32),
}

// ── 面向 AI 模型的输出格式 ──

/// 类别缩写映射（进一步压缩 token）
fn class_abbr(class: &str) -> String {
    match class {
        "Button" => "Btn".to_string(),
        "Text" => "Txt".to_string(),
        "Image" => "Img".to_string(),
        "Compo" => "Cmp".to_string(),
        "Block" => "Blk".to_string(),
        "Icon" => "Icn".to_string(),
        _ => class.chars().take(3).collect(),
    }
}

/// ---------- 格式 A：扁平压缩 ----------

/// 压缩后的扁平元素（去掉 id/children/parent，拍平 position，短键名）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactElement {
    pub c: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
    /// 颜色（十六进制 "#RRGGBB"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cl: Option<String>,
    /// 视觉重要性分数 0.0~1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<f64>,
}

impl From<&Element> for CompactElement {
    fn from(e: &Element) -> Self {
        Self {
            c: class_abbr(&e.class),
            x: e.position.column_min,
            y: e.position.row_min,
            w: e.width,
            h: e.height,
            t: e.text_content.clone(),
            cl: e.color.clone(),
            p: e.prominence,
        }
    }
}

/// ---------- 格式 B：AI 归一化（0-1000） ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiOutput {
    pub shape: (u32, u32),
    pub elements: Vec<CompactElement>,
}

impl AiOutput {
    pub fn from_elements(elements: &[Element], img_shape: (u32, u32)) -> Self {
        let (img_w, img_h) = (img_shape.1 as f64, img_shape.0 as f64);
        let norm = |val: i32, max: f64| -> i32 {
            if max <= 0.0 { return 0; }
            ((val as f64 / max) * 1000.0).round() as i32
        };
        Self {
            shape: (1000, 1000),
            elements: elements.iter().map(|e| CompactElement {
                c: class_abbr(&e.class),
                x: norm(e.position.column_min, img_w),
                y: norm(e.position.row_min, img_h),
                w: norm(e.width, img_w),
                h: norm(e.height, img_h),
                t: e.text_content.clone(),
                cl: e.color.clone(),
                p: e.prominence,
            }).collect(),
        }
    }
}

/// ---------- 格式 C：树形结构（推荐给 AI） ----------

/// 树节点：递归嵌套，AI 一眼看懂 DOM 层级
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub c: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
    /// 颜色（十六进制 "#RRGGBB"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cl: Option<String>,
    /// 视觉重要性分数 0.0~1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

/// 树形输出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeOutput {
    /// 原始图片尺寸
    pub img_shape: (u32, u32),
    /// 树根节点（虚拟根，包含所有顶层元素作为子节点）
    pub root: TreeNode,
}

impl TreeOutput {
    /// 从 Elements + img_shape 构建树形结构
    /// 利用 Element 已有的 parent/children 关系（由 check_containment 建立）
    pub fn from_elements(elements: &[Element], img_shape: (u32, u32)) -> Self {
        // 建立 id → Element 的索引
        let map: std::collections::HashMap<usize, &Element> =
            elements.iter().map(|e| (e.id, e)).collect();

        // 递归构建子树
        fn build_node(id: usize, map: &std::collections::HashMap<usize, &Element>) -> TreeNode {
            let e = map.get(&id).expect("Element id should exist");
            let mut node = TreeNode {
                c: class_abbr(&e.class),
                x: e.position.column_min,
                y: e.position.row_min,
                w: e.width,
                h: e.height,
                t: e.text_content.clone(),
                cl: e.color.clone(),
                p: e.prominence,
                children: Vec::new(),
            };
            // 递归添加子节点
            if let Some(child_ids) = &e.children {
                for &child_id in child_ids {
                    node.children.push(build_node(child_id, map));
                }
            }
            node
        }

        // 收集根节点（没有 parent 的顶层元素）
        let root_ids: Vec<usize> = elements.iter()
            .filter(|e| e.parent.is_none())
            .map(|e| e.id)
            .collect();

        let mut root = TreeNode {
            c: "Root".to_string(),
            x: 0,
            y: 0,
            w: img_shape.1 as i32,
            h: img_shape.0 as i32,
            t: None,
            cl: None,
            p: None,
            children: Vec::new(),
        };

        for &id in &root_ids {
            root.children.push(build_node(id, &map));
        }

        Self { img_shape, root }
    }

    /// 输出为带缩进的文本树（最省 token，AI 最爱）
    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("UI Layout ({}×{})", self.img_shape.1, self.img_shape.0));

        fn render_node(node: &TreeNode, prefix: &str, is_last: bool, is_root: bool, lines: &mut Vec<String>) {
            let connector = if is_root { "" } else if is_last { "└─ " } else { "├─ " };
            // Icn 类型不加引号（possibility 内容本身已语义清晰），其他类型加引号
            let text = match &node.t {
                Some(t) if node.c == "Icn" => format!(" {}", t),
                Some(t) => format!(" \"{}\"", t),
                None => String::new(),
            };
            let color = node.cl.as_ref().map(|c| format!(" {}", c)).unwrap_or_default();
            let prominence = node.p.and_then(|p| {
                let label = prominence_label(p);
                if label.is_empty() { None } else { Some(label.to_string()) }
            }).unwrap_or_default();
            let line = format!("{}{}[{:>3},{:>3} {:>3}×{:>3}] {}{}{}{}",
                prefix, connector, node.x, node.y, node.w, node.h, node.c, text, color, prominence);
            lines.push(line);

            let child_prefix = if is_root { "" } else if is_last { "   " } else { "│  " };
            let new_prefix = format!("{}{}", prefix, child_prefix);
            let count = node.children.len();
            for (i, child) in node.children.iter().enumerate() {
                render_node(child, &new_prefix, i == count - 1, false, lines);
            }
        }

        render_node(&self.root, "", true, true, &mut lines);
        lines.join("\n")
    }
}

/// ---------- 辅助：扁平文本（兼容旧版） ----------

/// 扁平列表的文本表示（适合简单场景）
pub fn elements_to_text(elements: &[Element], img_shape: (u32, u32)) -> String {
    let ai = AiOutput::from_elements(elements, img_shape);
    let mut lines = Vec::new();
    lines.push(format!("UI layout ({}x{})", img_shape.1, img_shape.0));
    for e in &ai.elements {
        // Icn 不加引号，其他类型加引号
        let text = match &e.t {
            Some(t) if e.c == "Icn" => format!(" {}", t),
            Some(t) => format!(" \"{}\"", t),
            None => String::new(),
        };
        let color = e.cl.as_ref().map(|c| format!(" {}", c)).unwrap_or_default();
        let prominence = e.p.and_then(|p| {
            let label = prominence_label(p);
            if label.is_empty() { None } else { Some(label.to_string()) }
        }).unwrap_or_default();
        lines.push(format!("  [{:>3},{:>3} {:>3}x{:>3}] {}{}{}{}",
            e.x, e.y, e.w, e.h, e.c, text, color, prominence));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bbox::Bbox;

    #[test]
    fn test_compact_conversion() {
        let bbox = Bbox::new(10, 20, 100, 200);
        let el = Element::new(0, &bbox, "Button", Some("OK".to_string()));
        let ce = CompactElement::from(&el);
        assert_eq!(ce.c, "Btn");
        assert_eq!(ce.x, 10);
        assert_eq!(ce.t, Some("OK".to_string()));
    }

    #[test]
    fn test_ai_output_normalized() {
        let bbox = Bbox::new(0, 0, 500, 500);
        let el = Element::new(0, &bbox, "Block", None);
        let ai = AiOutput::from_elements(&[el], (1000, 1000));
        assert_eq!(ai.elements[0].x, 0);
        assert_eq!(ai.elements[0].w, 500);
    }

    #[test]
    fn test_class_abbr_icon() {
        assert_eq!(class_abbr("Icon"), "Icn");
    }

    #[test]
    fn test_elements_to_text() {
        let bbox = Bbox::new(10, 20, 100, 60);
        let el = Element::new(0, &bbox, "Button", Some("登录".to_string()));
        let text = elements_to_text(&[el], (200, 400));
        assert!(text.contains("登录"));
        assert!(text.contains("Btn"));
    }

    #[test]
    fn test_tree_output_basic() {
        // 创建两个元素：父元素包含子元素
        let parent_bbox = Bbox::new(0, 0, 200, 200);
        let child_bbox = Bbox::new(10, 10, 50, 50);

        let mut parent = Element::new(0, &parent_bbox, "Block", None);
        let mut child = Element::new(1, &child_bbox, "Button", Some("Click".to_string()));

        parent.children = Some(vec![1]);
        child.parent = Some(0);

        let tree = TreeOutput::from_elements(&[parent, child], (200, 200));
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(tree.root.children[0].c, "Blk");
        assert_eq!(tree.root.children[0].children.len(), 1);
        assert_eq!(tree.root.children[0].children[0].c, "Btn");
        assert_eq!(tree.root.children[0].children[0].t.as_deref(), Some("Click"));
    }

    #[test]
    fn test_tree_output_flat_text() {
        let bbox = Bbox::new(0, 0, 100, 100);
        let el = Element::new(0, &bbox, "Block", None);
        let tree = TreeOutput::from_elements(&[el], (100, 100));
        let text = tree.to_text();
        assert!(text.contains("Blk"));
        assert!(text.contains("100×100"));
    }
}
