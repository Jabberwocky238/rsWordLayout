//! 字形图集的行为测试。
//!
//! 重点是**淘汰**：几千个 CJK 字形装不进一张固定纹理，所以按需填充 + 满了回收
//! 是这个模块存在的理由，也是最容易写错的地方。

use rsword_layout_core::gpu::{GlyphAtlas, RasterGlyph, Rasterizer};
use rsword_layout_core::measure::FontSpec;

/// 桩栅格化器：给每个字形一个固定大小的实心方块，便于断言。
struct Stub {
    size: u32,
    calls: usize,
}

impl Rasterizer for Stub {
    fn rasterize(&mut self, ch: char, _font: &FontSpec) -> Option<RasterGlyph> {
        self.calls += 1;
        if ch == ' ' {
            // 空白字形：0×0，但仍应被缓存。
            return Some(RasterGlyph {
                width: 0,
                height: 0,
                left: 0.0,
                top: 0.0,
                advance: self.size as f32 / 2.0,
                coverage: Vec::new(),
            });
        }
        let n = (self.size * self.size) as usize;
        Some(RasterGlyph {
            width: self.size,
            height: self.size,
            left: 0.0,
            top: self.size as f32,
            advance: self.size as f32,
            coverage: vec![255; n],
        })
    }
}

fn font() -> FontSpec {
    FontSpec::new("Test", 24)
}

#[test]
fn solid_texel_is_opaque_white() {
    // 纯色批次靠左上角这个纹素复用同一套着色器。
    let a = GlyphAtlas::new(64, 64);
    assert_eq!(a.pixels()[0], 255);
}

#[test]
fn caches_repeated_glyph() {
    let mut a = GlyphAtlas::new(256, 256);
    let mut r = Stub { size: 16, calls: 0 };
    let f = font();

    let q1 = a.get('字', &f, &mut r).expect("应放得下");
    let q2 = a.get('字', &f, &mut r).expect("应命中缓存");

    assert_eq!(r.calls, 1, "同一字形不该重复栅格化");
    assert_eq!(q1, q2, "缓存命中应返回同一 UV");
    assert_eq!(a.len(), 1);
}

#[test]
fn different_fonts_are_distinct_entries() {
    let mut a = GlyphAtlas::new(256, 256);
    let mut r = Stub { size: 16, calls: 0 };

    let normal = font();
    let mut bold = font();
    bold.bold = true;

    a.get('A', &normal, &mut r).unwrap();
    a.get('A', &bold, &mut r).unwrap();

    assert_eq!(r.calls, 2, "粗体与常规是不同字形");
    assert_eq!(a.len(), 2);
}

#[test]
fn blank_glyph_cached_without_area() {
    let mut a = GlyphAtlas::new(64, 64);
    let mut r = Stub { size: 16, calls: 0 };
    let f = font();

    let q = a.get(' ', &f, &mut r).unwrap();
    a.get(' ', &f, &mut r).unwrap();

    assert_eq!(q.width, 0.0);
    assert_eq!(q.height, 0.0);
    assert_eq!(r.calls, 1, "空白字形也要缓存，避免反复栅格化");
}

#[test]
fn oversized_glyph_is_rejected() {
    // 比整张图集还大的字形：宁可不画也不要画错。
    let mut a = GlyphAtlas::new(32, 32);
    let mut r = Stub { size: 64, calls: 0 };
    assert!(a.get('大', &font(), &mut r).is_none());
}

#[test]
fn evicts_when_full_and_keeps_serving() {
    // 图集只放得下少量字形，塞远超容量的字符，全程不能失败。
    let mut a = GlyphAtlas::new(64, 64);
    let mut r = Stub { size: 16, calls: 0 };
    let f = font();

    let mut served = 0;
    for ch in "一二三四五六七八九十甲乙丙丁戊己庚辛壬癸".chars() {
        if a.get(ch, &f, &mut r).is_some() {
            served += 1;
        }
    }
    assert_eq!(served, 20, "淘汰之后仍应能继续放入新字形");
    assert!(a.len() <= 20);
    assert!(!a.is_empty(), "不该把图集清空");
}

#[test]
fn eviction_marks_full_reset() {
    let mut a = GlyphAtlas::new(48, 48);
    let mut r = Stub { size: 16, calls: 0 };
    let f = font();

    for ch in "一二三".chars() {
        a.get(ch, &f, &mut r);
    }
    a.clear_dirty();

    // 继续塞直到触发淘汰
    for ch in "四五六七八九十".chars() {
        a.get(ch, &f, &mut r);
    }
    let d = a.dirty().expect("淘汰后必须报脏");
    assert_eq!(d.width, a.width(), "淘汰导致 UV 失效，必须整幅重传");
    assert_eq!(d.height, a.height());
}

#[test]
fn dirty_region_tracks_writes() {
    let mut a = GlyphAtlas::new(256, 256);
    let mut r = Stub { size: 16, calls: 0 };
    a.clear_dirty();
    assert!(a.dirty().is_none(), "清空后不该有脏区域");

    a.get('A', &font(), &mut r).unwrap();
    let d = a.dirty().expect("写入后应报脏");
    assert!(d.width >= 16 && d.height >= 16);
}

#[test]
fn uv_within_unit_range() {
    let mut a = GlyphAtlas::new(128, 128);
    let mut r = Stub { size: 16, calls: 0 };
    let q = a.get('X', &font(), &mut r).unwrap();
    for v in [q.u0, q.v0, q.u1, q.v1] {
        assert!((0.0..=1.0).contains(&v), "UV 必须归一化：{v}");
    }
    assert!(q.u1 > q.u0 && q.v1 > q.v0, "UV 区域不能退化");
}
