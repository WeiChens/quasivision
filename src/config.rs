/// 配置参数，对应原版 CONFIG_UIED.py
#[derive(Debug, Clone)]
pub struct Config {
    /// 梯度二值化阈值 (dribbble:4, rico:4, web:1)
    pub gradient_threshold: u8,

    /// 最小连通区域面积
    pub obj_min_area: u32,

    // ── 矩形检测参数 ──
    /// 矩形最小平整度：边界平坦像素占比 (原版: 0.7)
    pub rec_min_evenness: f64,
    /// 矩形最大凹陷比：凹陷像素占比上限 (原版: 0.25)
    pub rec_max_dent_ratio: f64,
    /// 圆角容错：跳过每边两端边界点的比例 (默认: 0.08)
    /// 圆角弯曲只出现在角部，忽略首尾各 N% 即可排除圆角干扰。
    /// 设为 0 则严格检测直角矩形。值越大，对圆角的容忍度越高。
    pub rec_corner_skip_ratio: f64,

    // ── 线条检测/移除参数 ──
    /// 线条最大粗细（像素）(原版: 8)
    pub line_thickness: u32,
    /// 线条最小长度比例：连续白像素占比 >= 此值则视为行 (原版: 0.95)
    pub line_min_length_ratio: f64,

    // ── 文本检测参数 ──
    /// 文本单词最大间距（像素）(原版: 10)
    pub text_max_word_gap: u32,
    /// 文本最大高度比（相对于图片高度）(原版: 0.04)
    pub text_max_height: f64,

    // ── 顶栏/底栏参数 ──
    /// 顶部/底部栏高度比 (top, bottom) (原版: 0.045, 0.94)
    pub top_bottom_bar: (f64, f64),

    // ── Block 容器检测参数 ──
    /// Block 最小高度比 (原版: 0.03)
    #[allow(dead_code)]
    pub block_min_height: f64,
    /// Block 边长占比阈值：宽或高超过此比例则检查是否为容器 (原版: 0.15)
    pub block_side_length: f64,
    /// Block 嵌套检测的梯度阈值 (原版: 5)
    pub block_gradient_threshold: u8,

    // ── 组件最大尺寸 ──
    /// 原子组件最大尺寸比例 (height_ratio, width_ratio) (原版: 0.25, 0.98)
    #[allow(dead_code)]
    pub compo_max_scale: (f64, f64),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gradient_threshold: 4,
            obj_min_area: 55,
            rec_min_evenness: 0.7,
            rec_max_dent_ratio: 0.25,
            rec_corner_skip_ratio: 0.08,
            line_thickness: 8,
            line_min_length_ratio: 0.95,
            text_max_word_gap: 10,
            text_max_height: 0.08,
            top_bottom_bar: (0.045, 0.94),
            block_min_height: 0.03,
            block_side_length: 0.15,
            block_gradient_threshold: 5,
            compo_max_scale: (0.25, 0.98),
        }
    }
}
