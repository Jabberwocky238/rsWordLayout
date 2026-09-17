//! 绘制契约的性质测试。
//!
//! 两条核心断言：
//!   1. **core 的产物与分辨率无关**——坐标一律 twips，没有像素；
//!   2. **所有绘制归约到路径**——矩形只是路径的特例，环绕区能表达任意多边形。

use rsword_layout_core::{
    Color, DrawCmd, FillRule, PaintOp, Path, PathSeg, Transform, VectorCanvas,
};
use rsword_layout_core::{Engine, PageSetup, Para, Run};
use rsword_layout_core::Rect;
use rsword_layout_core::FontSpec;
use rsword_layout_core::{WrapRegion, paint_document};
use rsword_layout_core::SimpleMetrics;

fn doc() -> Vec<rsword_layout_core::Page> {
    let metrics = SimpleMetrics;
    let engine = Engine::new(&metrics, PageSetup::a4());
    let para = Para {
        runs: vec![Run {
            text: "Hello 世界".to_string(),
            font: FontSpec::new("Test", 24),
            color: Color::BLACK,
        }],
        ..Para::default()
    };
    engine.layout(&[para])
}

#[test]
fn paint_output_carries_no_pixels() {
    // 页面尺寸应当是 A4 的 twips 值，而不是任何像素数。
    let list = paint_document(&doc(), None, &[]);
    let page = &list.pages[0];
    assert_eq!(page.width, 11906, "A4 宽应为 11906 twips");
    assert_eq!(page.height, 16838, "A4 高应为 16838 twips");
}

#[test]
fn text_survives_without_shaper() {
    // 没有 shaper 时仍要保留原文与起点：SVG / PDF 后端直接排文字，不需要字形序列。
    let list = paint_document(&doc(), None, &[]);
    let found = list.pages[0].cmds.iter().any(|c| match c {
        DrawCmd::DrawGlyphs { text, origin_x, origin_y, glyphs, .. } => {
            glyphs.is_empty() && text.contains("Hello") && *origin_x > 0 && *origin_y > 0
        }
        _ => false,
    });
    assert!(found, "应当保留原文与起点，且起点不能是 0");
}

#[test]
fn geometry_is_stable_across_invocations() {
    // 同一份布局重复转换必须给出同一结果——后端据此做快照回归。
    let d = doc();
    assert_eq!(
        paint_document(&d, None, &[]),
        paint_document(&d, None, &[]),
        "绘制指令必须是确定性的"
    );
}

#[test]
fn content_area_excludes_margins() {
    let list = paint_document(&doc(), None, &[]);
    let area = list.pages[0].content_area;
    // 1 英寸页边距 = 1440 twips。
    assert_eq!(area.x, 1440);
    assert_eq!(area.y, 1440);
    assert_eq!(area.width, 11906 - 2880);
}

#[test]
fn rect_is_a_path_special_case() {
    // 矩形不是独立的绘制原语，而是四点闭合路径。
    let p = Path::rect(Rect::new(100, 200, 300, 400));
    assert_eq!(p.segs.len(), 5, "MoveTo + 三条 LineTo + Close");
    assert!(matches!(p.segs[0], PathSeg::MoveTo { x: 100, y: 200 }));
    assert!(matches!(p.segs[4], PathSeg::Close));
}

#[test]
fn wrap_region_supports_arbitrary_polygon() {
    // Word 的 w:wrapPolygon 是任意多边形，矩形表达不了——这正是原语必须是路径的理由。
    let tri = WrapRegion::polygon(&[(0, 0), (1000, 0), (500, 800)], 120);
    assert_eq!(tri.path.segs.len(), 4, "三点 + Close");
    assert_eq!(tri.distance, 120);

    let r = WrapRegion::rect(Rect::new(0, 0, 100, 100), 0);
    assert_eq!(r.path.segs.len(), 5);
}

#[test]
fn transform_composition_matches_pdf_convention() {
    // `a.then(&b)` 的语义是「b 在 a 的坐标系里生效」，与 PDF 的 concatmatrix 一致：
    // 已有的平移量不被后来的缩放放大。
    let t = Transform::translate(10.0, 20.0).then(&Transform::scale(2.0, 3.0));
    assert_eq!(t.apply(1.0, 1.0), (12.0, 23.0), "点先缩放，再加上原平移量");

    // 反序则平移量会被缩放吃掉，两者不可交换——这正是要钉住顺序的原因。
    let u = Transform::scale(2.0, 3.0).then(&Transform::translate(10.0, 20.0));
    assert_eq!(u.apply(1.0, 1.0), (22.0, 63.0), "平移量被缩放放大");
    assert_ne!(t.apply(1.0, 1.0), u.apply(1.0, 1.0));
}

#[test]
fn canvas_replays_commands_in_order() {
    // 画布是有状态的：指令顺序必须被保留，否则 Save/Clip 的语义不成立。
    #[derive(Default)]
    struct Recorder {
        log: Vec<String>,
    }
    impl VectorCanvas for Recorder {
        type Error = ();
        fn begin_page(&mut self, _w: i32, _h: i32) -> Result<(), ()> {
            self.log.push("begin".into());
            Ok(())
        }
        fn end_page(&mut self) -> Result<(), ()> {
            self.log.push("end".into());
            Ok(())
        }
        fn draw(&mut self, cmd: &DrawCmd) -> Result<(), ()> {
            self.log.push(
                match cmd {
                    DrawCmd::Save => "save",
                    DrawCmd::Restore => "restore",
                    DrawCmd::Transform(_) => "xform",
                    DrawCmd::Clip { .. } => "clip",
                    DrawCmd::DrawPath { .. } => "path",
                    DrawCmd::DrawGlyphs { .. } => "glyphs",
                    DrawCmd::DrawImage { .. } => "image",
                }
                .into(),
            );
            Ok(())
        }
    }

    let mut page = rsword_layout_core::PaintPage::new(100, 100, Rect::new(0, 0, 100, 100));
    page.cmds.push(DrawCmd::Save);
    page.cmds.push(DrawCmd::Clip { path: Path::rect(Rect::new(0, 0, 50, 50)), rule: FillRule::NonZero });
    page.cmds.push(DrawCmd::DrawPath {
        path: Path::rect(Rect::new(0, 0, 10, 10)),
        op: PaintOp::Fill { paint: Default::default(), rule: FillRule::NonZero },
    });
    page.cmds.push(DrawCmd::Restore);

    let mut rec = Recorder::default();
    page.replay(&mut rec).unwrap();
    assert_eq!(rec.log, ["begin", "save", "clip", "path", "restore", "end"]);
}
