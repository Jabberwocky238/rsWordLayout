//! 布局产物。
//!
//! 布局的结果是一棵**已定位**的树：每个元素带页号与页内坐标，不再有任何待决定的东西。
//! 它是布局层与后端之间的唯一契约，也是回归测试的快照对象——
//! 断言「这段文字在第 2 页 x=1440 y=2880」比截图对比稳定得多。

use crate::canvas::Color;
use crate::geom::{Rect, Size, Twips};
use crate::measure::FontSpec;

/// 页内一个已定位的绘制项。
#[derive(Debug, Clone)]
pub enum Fragment {
    Text(TextFragment),
    /// 实心矩形：底纹、边框、下划线、删除线都归一到它。
    Rect { rect: Rect, color: Color },
    Image { id: String, rect: Rect },
}

#[derive(Debug, Clone)]
pub struct TextFragment {
    pub x: Twips,
    /// 基线绝对 y（页内坐标）。
    pub baseline_y: Twips,
    pub text: String,
    pub font: FontSpec,
    pub color: Color,
    /// 来源段落的 `rsword` 节点 id，供调试与反查。
    pub source_node: Option<u32>,
}

/// 一行。保留行信息而不直接摊平成 Fragment，是因为对齐、两端对齐的空白分配、
/// 以及垂直居中都需要「整行」这个单位。
#[derive(Debug, Clone, Default)]
pub struct Line {
    /// 行顶 y（页内）。
    pub top: Twips,
    pub height: Twips,
    /// 基线相对行顶的偏移。
    pub baseline: Twips,
    pub fragments: Vec<Fragment>,
}

#[derive(Debug, Clone)]
pub struct Page {
    /// 页面物理尺寸。
    pub size: Size,
    /// 正文可用区（已扣页边距），供调试与页眉页脚定位参考。
    pub content_area: Rect,
    pub fragments: Vec<Fragment>,
}

impl Page {
    pub fn new(size: Size, content_area: Rect) -> Page {
        Page { size, content_area, fragments: Vec::new() }
    }

    /// 把一行摊平进页面。
    pub fn push_line(&mut self, line: Line) {
        self.fragments.extend(line.fragments);
    }
}

/// 整篇文档的布局结果。
#[derive(Debug, Clone, Default)]
pub struct LaidOutDocument {
    pub pages: Vec<Page>,
}

impl LaidOutDocument {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}
