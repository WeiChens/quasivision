use image::DynamicImage;

use crate::component::Component;
use crate::element::Element;
use crate::config::Config;

/// 合并组件和文本检测结果，生成最终元素列表
pub fn merge(
    img: &DynamicImage,
    comps: &[Component],
    texts: &[Element],
    config: &Config,
    is_paragraph: bool,
    is_remove_bar: bool,
) -> Vec<Element> {
    let img_shape = (img.height(), img.width());
    let img_height = img_shape.0 as f64;

    // 1. 转换 Component → Element
    let comp_eles: Vec<Element> = comps.iter()
        .map(|c| {
            Element::new(0, &c.bbox, &c.category, None)
        })
        .collect();

    // 2. 转换文本 Element
    let mut text_eles = texts.to_vec();

    // 3. 精炼文本：去除噪声（使用 config.text_max_height 而非硬编码）
    text_eles = refine_texts(&text_eles, img_shape, config.text_max_height);

    // 4. 精炼元素：处理组件与文本的包含关系
    let mut elements = refine_elements(&comp_eles, &text_eles, (2, 2), 0.8);

    // 5. 可选：移除顶部/底部栏（使用 config.top_bottom_bar 而非硬编码）
    if is_remove_bar {
        elements = remove_top_bar(&elements, img_height, config.top_bottom_bar);
        elements = remove_bottom_bar(&elements, img_height, config.top_bottom_bar);
    }

    // 6. 可选：文本行合并为段落
    if is_paragraph {
        elements = merge_text_lines(&elements, config.text_max_word_gap as i32);
    }

    // 7. 分配 ID
    reassign_ids(&mut elements);

    // 8. 检查包含关系，建立 parent-child
    check_containment(&mut elements);

    elements
}

/// 精炼文本：移除太高的文本（噪声）
///
/// 规则：
/// - 空内容（长度 0）→ 总是移除
/// - 单字符：
///   - 数字 (0-9)、字母、中文 → 保留（如 "0"、"目"、"品" 是有效数据）
///   - 纯标点符号 → 过滤（如 "…"、"£"）
/// - 有意义长文本（>5 字符 + 宽度 > 2倍高度，如标题/标语）→ 总是保留
/// - 短文本（如按钮上的文字）→ 应用高度比过滤
pub fn refine_texts(texts: &[Element], img_shape: (u32, u32), text_max_height_ratio: f64) -> Vec<Element> {
    texts.iter()
        .filter(|t| {
            let content = t.text_content.as_deref().unwrap_or("");
            let content_len = content.len();
            if content_len == 0 {
                return false; // 空内容 → 过滤
            }

            // 单字符：数字、字母、中文保留，纯标点符号过滤
            if content_len == 1 {
                let c = content.chars().next().unwrap();
                // 数字 0-9 → 保留
                if c.is_ascii_digit() {
                    return true;
                }
                // 英文字母 → 保留
                if c.is_ascii_alphabetic() {
                    return true;
                }
                // 中文字符（CJK Unified Ideographs 0x4E00-0x9FFF）→ 保留
                if ('\u{4E00}'..='\u{9FFF}').contains(&c) {
                    return true;
                }
                // 其他单字符（标点、符号等）→ 过滤
                return false;
            }

            // 有意义的长文本（标题、标语等）→ 保留，不受高度限制
            if content_len > 5 && t.width > t.height * 2 {
                return true;
            }

            // 短文本 → 应用高度比过滤
            let height_ratio = (t.height as f64) / (img_shape.0 as f64);
            height_ratio < text_max_height_ratio
        })
        .cloned()
        .collect()
}

/// 精炼元素：
/// - 过滤被文本完全覆盖的非 Block/Image 组件（用文本取代）
/// - 所有文本都保留，后续由 check_containment 建立 parent-child 层级
pub fn refine_elements(
    comps: &[Element],
    texts: &[Element],
    bias: (i32, i32),
    containment_ratio: f64,
) -> Vec<Element> {
    let mut elements: Vec<Element> = Vec::new();

    for comp in comps {
        let mut is_covered_by_text = false;

        for text in texts.iter() {
            let (inter, _, ioa, _) = comp.calc_intersection(text, bias);

            if inter > 0 {
                // 非 Block/Image 组件被文本完全覆盖 → 用文本取代组件
                if ioa >= containment_ratio
                    && comp.class != "Block"
                    && comp.class != "Image"
                {
                    is_covered_by_text = true;
                    break;
                }
            }
        }

        if !is_covered_by_text && comp.area() > 0 {
            elements.push(comp.clone());
        }
    }

    // 所有文本都加入（后续 check_containment 会处理包含关系）
    for text in texts.iter() {
        elements.push(text.clone());
    }

    elements
}

/// 移除顶部栏（使用 config.top_bottom_bar 控制高度比例阈值）
pub fn remove_top_bar(elements: &[Element], img_height: f64, top_bottom_bar: (f64, f64)) -> Vec<Element> {
    let max_height = img_height * 0.04;
    let top_threshold = (img_height * top_bottom_bar.0) as i32;
    elements.iter()
        .filter(|e| !(e.position.row_min < top_threshold && (e.height as f64) < max_height))
        .cloned()
        .collect()
}

/// 移除底部栏（使用 config.top_bottom_bar 控制高度比例阈值）
pub fn remove_bottom_bar(elements: &[Element], img_height: f64, top_bottom_bar: (f64, f64)) -> Vec<Element> {
    let bottom_start = (img_height * top_bottom_bar.1) as i32;
    elements.iter()
        .filter(|e| {
            !(e.position.row_min > bottom_start
                && (e.height as f64) < img_height * 0.03
                && (e.width as f64) < img_height * 0.03)
        })
        .cloned()
        .collect()
}

/// 合并文本行到段落
pub fn merge_text_lines(elements: &[Element], max_line_gap: i32) -> Vec<Element> {
    let mut texts: Vec<Element> = Vec::new();
    let mut non_texts: Vec<Element> = Vec::new();

    for ele in elements {
        if ele.class == "Text" {
            texts.push(ele.clone());
        } else {
            non_texts.push(ele.clone());
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        let mut temp: Vec<Element> = Vec::new();

        'outer: for ta in &texts {
            for tb in &mut temp {
                let (inter, _, _, _) = ta.calc_intersection(tb, (0, max_line_gap));
                if inter > 0 {
                    tb.element_merge(ta);
                    changed = true;
                    continue 'outer;
                }
            }
            temp.push(ta.clone());
        }

        texts = temp;
    }

    // 合并回非文本列表
    non_texts.extend(texts);
    non_texts
}

/// 重新分配 ID
pub fn reassign_ids(elements: &mut [Element]) {
    for (i, ele) in elements.iter_mut().enumerate() {
        ele.id = i;
    }
}

/// 检查包含关系，建立 parent-child 层级
///
/// 规则：优先选择**面积最小**（最内层）的容器作为 parent。
/// 如果元素已被一个容器包含，新容器只有**面积更小**（更精确）时才替换。
pub fn check_containment(elements: &mut [Element]) {
    let n = elements.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let rel = elements[i].element_relation(&elements[j], (2, 2));
            if rel == -1 {
                // i in j → j 是容器
                let i_id = elements[i].id;
                let current_parent = elements[i].parent;
                let should_assign = match current_parent {
                    None => true,
                    Some(pid) => {
                        elements[j].area() < elements[pid].area()
                    }
                };
                if should_assign {
                    // 从旧父节点移除
                    if let Some(pid) = current_parent {
                        if let Some(children) = elements[pid].children.as_mut() {
                            children.retain(|&c| c != i_id);
                        }
                    }
                    elements[j].children.get_or_insert(vec![]).push(elements[i].id);
                    elements[i].parent = Some(elements[j].id);
                }
            } else if rel == 1 {
                // j in i → i 是容器
                let j_id = elements[j].id;
                let current_parent = elements[j].parent;
                let should_assign = match current_parent {
                    None => true,
                    Some(pid) => {
                        elements[i].area() < elements[pid].area()
                    }
                };
                if should_assign {
                    if let Some(pid) = current_parent {
                        if let Some(children) = elements[pid].children.as_mut() {
                            children.retain(|&c| c != j_id);
                        }
                    }
                    elements[i].children.get_or_insert(vec![]).push(elements[j].id);
                    elements[j].parent = Some(elements[i].id);
                }
            }
        }
    }
}

/// 为未被任何非 Text 组件覆盖的「孤儿文本」创建容器 Block
///
/// ## 场景
/// 手写文章、扫描文档等场景下，CV 连通区域检测不到文字框，
/// 但 OCR 能识别出文本。这些 Text 元素没有父容器，结构上"飘着"。
///
/// 此函数直接使用原始 OCR 文本（不过滤高度），检查它们是否被已有组件覆盖，
/// 如果未被覆盖，则将附近（同行/同段落）的文本聚合成一个 Block 容器，
/// 并建立 parent-child 关系。
///
/// ## 参数
/// - `elements`: 合并后的元素列表（会被修改——添加新 Block 和孤儿 Text）
/// - `raw_texts`: 原始 OCR 文本（未经过 refine_texts 过滤）
/// - `coverage_threshold`: Text 被覆盖的比例阈值，默认 0.3
/// - `line_gap`: 同一段落内两行之间的最大垂直间距（像素）
pub fn synthesize_orphan_text_regions(
    elements: &mut Vec<Element>,
    raw_texts: &[Element],
    coverage_threshold: f64,
    line_gap: i32,
) {
    if raw_texts.is_empty() {
        return;
    }

    // 1. 收集当前所有非 Text 元素（用于覆盖检测）
    let non_texts: Vec<&Element> = elements.iter()
        .filter(|e| e.class != "Text")
        .collect();

    // 2. 找出孤儿文本：与任何非 Text 组件的 IoU < coverage_threshold
    //    直接使用原始 OCR 文本（不经过高度过滤）
    let orphan_indices: Vec<usize> = raw_texts.iter().enumerate()
        .filter(|(_, text)| {
            let max_iou = non_texts.iter()
                .map(|nt| {
                    let (_, iou, _, _) = text.calc_intersection(nt, (2, 2));
                    iou
                })
                .fold(0.0f64, f64::max);
            max_iou < coverage_threshold
        })
        .map(|(idx, _)| idx)
        .collect();

    if orphan_indices.is_empty() {
        return;
    }

    // 3. 按垂直位置排序
    let mut sorted_orphan: Vec<(usize, i32)> = orphan_indices.iter()
        .map(|&idx| (idx, raw_texts[idx].position.row_min))
        .collect();
    sorted_orphan.sort_by_key(|&(_, row)| row);

    // 4. 分组——将同一段落中的文本行聚在一起
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut used = vec![false; sorted_orphan.len()];

    for i in 0..sorted_orphan.len() {
        if used[i] { continue; }
        let mut group = vec![sorted_orphan[i].0];
        used[i] = true;

        for j in (i + 1)..sorted_orphan.len() {
            if used[j] { continue; }

            let last_idx = *group.last().unwrap();
            let last = &raw_texts[last_idx];
            let candidate = &raw_texts[sorted_orphan[j].0];

            let vert_gap = candidate.position.row_min - last.position.row_max;

            // 计算水平间距（正值 = 有间距，≤0 = 有重叠）
            let horiz_gap = if candidate.position.column_min > last.position.column_max {
                candidate.position.column_min - last.position.column_max
            } else if last.position.column_min > candidate.position.column_max {
                last.position.column_min - candidate.position.column_max
            } else {
                0 // 水平有重叠
            };

            // 条件：垂直间距 ≤ line_gap，且水平间距 ≤ 60px（防止跨栏合并）
            if vert_gap <= line_gap && horiz_gap <= 60 {
                group.push(sorted_orphan[j].0);
                used[j] = true;
            }
        }
        groups.push(group);
    }

    // 5. 为每个组创建一个 Block 容器（原始文本已在 elements 中，无需重复添加）
    let mut new_blocks: Vec<Element> = Vec::new();

    for (_g_idx, group) in groups.iter().enumerate() {
        if group.is_empty() { continue; }

        let col_min = group.iter().map(|&idx| raw_texts[idx].position.column_min).min().unwrap_or(0);
        let row_min = group.iter().map(|&idx| raw_texts[idx].position.row_min).min().unwrap_or(0);
        let col_max = group.iter().map(|&idx| raw_texts[idx].position.column_max).max().unwrap_or(0);
        let row_max = group.iter().map(|&idx| raw_texts[idx].position.row_max).max().unwrap_or(0);

        let pad = 6i32;
        let block = Element::from_parts(
            0,
            (col_min - pad).max(0),
            (row_min - pad).max(0),
            col_max + pad,
            row_max + pad,
            "Block",
        );
        new_blocks.push(block);
    }

    // 6. 只加入新 Block，重建 parent-child（先清空旧关系避免残留）
    if !new_blocks.is_empty() {
        // 清空所有旧 parent-child 关系，避免第一次 containment 的残留导致重复
        for e in elements.iter_mut() {
            e.parent = None;
            e.children = None;
        }
        elements.extend(new_blocks);
        reassign_ids(elements);
        check_containment(elements);
    }
}
