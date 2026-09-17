//! 从 rsword 的文档模型提取文字环绕区。
//!
//! 走 rsword 的 **Rust API 而非 JSON 投影**：`document()` 的 JSON 里只有
//! `{"kind":"image","node":N}`，没有 `wrap` / `anchor` / `extent`——锚定几何
//! 在 model 层存在但未导出。所以这里直接用 `rsword::model`，耦合集中在本模块。
//!
//! # 覆盖范围
//!
//! rsword 提供了环绕类型（`Wrap`）、环绕方向（`wrapText`）、四周边距（`Dist`，EMU）、
//! 图形尺寸（`Extent`，EMU）与锚定位置（`Position`）。**但没有 `wrapPolygon` 顶点**
//! （`model/mod.rs` 里 `WrapPolygon` 零命中），所以 `Tight` / `Through` 只能退化为
//! 按 `Extent` 的包围盒近似——包围盒偏大，遮挡略多而非略少，文字不会压到图上。
//!
//! 锚定位置只认最明确的情形：`posOffset` 绝对偏移。`align`（相对对齐）与
//! 各种 `relativeFrom` 基准的完整解析尚未实现，此类图形被跳过并计数，
//! 由调用方报出——不静默出错。

use rsword::model::{AnchorGeom, Block, Document, Wrap};

use crate::layout::{Rect, WrapContext, WrapRegion, WrapSide, emu_to_twips};

/// 提取结果。
#[derive(Debug, Default)]
pub struct AnchorScan {
    pub wrap: WrapContext,
    /// 因锚定方式暂不支持而跳过的图形数。
    pub skipped: usize,
    /// 不参与绕排的图形数（`wrapNone` 或随文）。
    pub not_wrapping: usize,
}

impl AnchorScan {
    /// 扫描整篇文档的锚定图形。
    ///
    /// `content` 是正文可用区（已扣页边距）：锚定偏移与相对对齐都相对它换算。
    pub fn from_document(doc: &Document, dom: &rsword::xml::Dom, content: Rect) -> AnchorScan {
        let mut out = AnchorScan::default();
        for block in &doc.main {
            match block {
                // 段落里锚定的图形：环绕最常见的来源。
                Block::Text(t) => {
                    for d in &t.facts.drawings {
                        if !d.anchored {
                            out.not_wrapping += 1;
                            continue;
                        }
                        let display = rsword::model::drawing::drawing_display(dom, d.node);
                        out.consume(display.anchor.as_ref(), &display.extent, content);
                    }
                }
                // 独立成块的图片。
                Block::Image(b) => {
                    let Some(display) = b.display.as_ref().and_then(|d| d.as_drawing()) else {
                        out.not_wrapping += 1;
                        continue;
                    };
                    out.consume(display.anchor.as_ref(), &display.extent, content);
                }
                _ => {}
            }
        }
        out
    }

    /// 处理一个图形的锚定信息。
    ///
    /// `content` 是正文可用区（已扣页边距）：`align` 相对对齐要靠它算横向位置。
    fn consume(
        &mut self,
        anchor: Option<&AnchorGeom>,
        extent: &Option<rsword::model::Extent>,
        content: Rect,
    ) {
        let Some(a) = anchor else {
            // 随文图形（`wp:inline`）不绕排，它占据行内空间，由断行自然处理。
            self.not_wrapping += 1;
            return;
        };

        let side = match &a.wrap {
            // 不绕排：浮在文字上/下，不产生排除区。
            Wrap::None | Wrap::Unspecified => {
                self.not_wrapping += 1;
                return;
            }
            Wrap::TopAndBottom => WrapSide::TopAndBottom,
            w => match w.text() {
                Some("left") => WrapSide::Left,
                Some("right") => WrapSide::Right,
                Some("largest") => WrapSide::Largest,
                // `bothSides` 与缺省都是两侧。
                _ => WrapSide::BothSides,
            },
        };

        let Some(e) = extent else {
            self.skipped += 1;
            return;
        };
        let (w, h) = (emu_to_twips(e.cx), emu_to_twips(e.cy));

        // 横向：posOffset 优先，其次 align 相对对齐。语料里 align 才是常见情形
        // （`<wp:align>right</wp:align>`），只支持 posOffset 会把大多数图形跳过。
        let Some(x) = Self::resolve_h(&a.h, content, w) else {
            self.skipped += 1;
            return;
        };
        // 纵向：只认 posOffset。align（top/bottom/center）与 relativeFrom 的
        // 各种基准需要知道所在段落的 y，而那要等布局跑起来才知道——本层拿不到，
        // 所以退回 0（即段落起点），由调用方按 content.y 平移。
        let y = a.v.offset_emu.map(emu_to_twips).unwrap_or(0);

        let rect = Rect::new(x, y + content.y, w, h);
        // 四周边距取最大值：WrapRegion 目前是各向同性的单一 distance。
        let dist = [a.dist.top, a.dist.bottom, a.dist.left, a.dist.right]
            .iter()
            .filter_map(|d| *d)
            .map(emu_to_twips)
            .max()
            .unwrap_or(0);

        self.wrap.add(WrapRegion::rect(rect, dist).with_side(side));
    }

    /// 解析横向位置。
    ///
    /// `relativeFrom` 的 column / margin / page 在单栏无分栏的常见情形下都落到
    /// 同一个正文区，故这里统一按 `content` 算；多栏与 page 基准的区分待实现。
    fn resolve_h(p: &rsword::model::Position, content: Rect, w: i32) -> Option<i32> {
        if let Some(off) = p.offset_emu {
            return Some(content.x + emu_to_twips(off));
        }
        match p.align.as_deref() {
            Some("left") | Some("inside") => Some(content.x),
            Some("center") => Some(content.x + (content.width - w) / 2),
            Some("right") | Some("outside") => Some(content.x + content.width - w),
            _ => None,
        }
    }

    /// 是否提取到任何环绕区。
    pub fn is_empty(&self) -> bool {
        self.wrap.is_empty()
    }
}
