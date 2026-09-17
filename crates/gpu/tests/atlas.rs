//! 字形图集的行为测试。
//!
//! 重点是**淘汰**：几万个字形装不进一张固定纹理，所以按需填充 + 满了回收
//! 是这个模块存在的理由，也是最容易写错的地方。
//!
//! 键是 shaping 产出的 glyph id 而非 `char`——连字与阿拉伯语形态没有对应的单个字符。

use rsword_layout_gpu::{GlyphAtlas, GlyphKey, GlyphMetrics, RasterGlyph, Rasterizer};

/// 桩栅格化器：给每个字形一个固定大小的实心方块，便于断言。
struct Stub {
    size: u32,
    calls: usize,
}

impl Rasterizer for Stub {
    fn rasterize(&mut self, key: &GlyphKey) -> Option<RasterGlyph> {
        self.calls += 1;
        // glyph id 0 约定为空白（多数字体里 .notdef 之后的空槽），用来测空白路径。
        if key.glyph_id == 0 {
            return Some(RasterGlyph {
                metrics: GlyphMetrics {
                    width: 0,
                    height: 0,
                    left: 0.0,
                    top: 0.0,
                    advance: self.size as f32 / 2.0,
                },
                coverage: Vec::new(),
            });
        }
        let n = (self.size * self.size) as usize;
        Some(RasterGlyph {
            metrics: GlyphMetrics {
                width: self.size,
                height: self.size,
                left: 0.0,
                top: self.size as f32,
                advance: self.size as f32,
            },
            coverage: vec![255; n],
        })
    }
}

fn key(id: u32) -> GlyphKey {
    GlyphKey::new("face-a", id, 24)
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

    let q1 = a.get(&key(42), &mut r).expect("应放得下");
    let q2 = a.get(&key(42), &mut r).expect("应命中缓存");

    assert_eq!(r.calls, 1, "同一字形不该重复栅格化");
    assert_eq!(q1, q2, "缓存命中应返回同一 UV");
    assert_eq!(a.len(), 1);
}

#[test]
fn distinct_faces_are_distinct_entries() {
    // 同一 glyph id 在不同字体里是完全不同的形状，不能混用。
    let mut a = GlyphAtlas::new(256, 256);
    let mut r = Stub { size: 16, calls: 0 };

    a.get(&GlyphKey::new("face-a", 7, 24), &mut r).unwrap();
    a.get(&GlyphKey::new("face-b", 7, 24), &mut r).unwrap();

    assert_eq!(r.calls, 2);
    assert_eq!(a.len(), 2);
}

#[test]
fn distinct_sizes_are_distinct_entries() {
    let mut a = GlyphAtlas::new(256, 256);
    let mut r = Stub { size: 16, calls: 0 };

    a.get(&GlyphKey::new("face-a", 7, 24), &mut r).unwrap();
    a.get(&GlyphKey::new("face-a", 7, 48), &mut r).unwrap();

    assert_eq!(r.calls, 2, "不同字号要分别栅格化");
    assert_eq!(a.len(), 2);
}

#[test]
fn blank_glyph_cached_without_area() {
    let mut a = GlyphAtlas::new(64, 64);
    let mut r = Stub { size: 16, calls: 0 };

    let q = a.get(&key(0), &mut r).unwrap();
    a.get(&key(0), &mut r).unwrap();

    assert_eq!(q.width, 0.0);
    assert_eq!(q.height, 0.0);
    assert_eq!(r.calls, 1, "空白字形也要缓存，避免反复栅格化");
}

#[test]
fn oversized_glyph_is_rejected() {
    // 比整张图集还大的字形：宁可不画也不要画错。
    let mut a = GlyphAtlas::new(32, 32);
    let mut r = Stub { size: 64, calls: 0 };
    assert!(a.get(&key(1), &mut r).is_none());
}

#[test]
fn evicts_when_full_and_keeps_serving() {
    // 图集只放得下少量字形，塞远超容量的字形，全程不能失败。
    let mut a = GlyphAtlas::new(64, 64);
    let mut r = Stub { size: 16, calls: 0 };

    let mut served = 0;
    for id in 1..=20u32 {
        if a.get(&key(id), &mut r).is_some() {
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

    for id in 1..=3u32 {
        a.get(&key(id), &mut r);
    }
    a.clear_dirty();

    // 继续塞直到触发淘汰
    for id in 4..=10u32 {
        a.get(&key(id), &mut r);
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

    a.get(&key(1), &mut r).unwrap();
    let d = a.dirty().expect("写入后应报脏");
    assert!(d.width >= 16 && d.height >= 16);
}

#[test]
fn uv_within_unit_range() {
    let mut a = GlyphAtlas::new(128, 128);
    let mut r = Stub { size: 16, calls: 0 };
    let q = a.get(&key(9), &mut r).unwrap();
    for v in [q.u0, q.v0, q.u1, q.v1] {
        assert!((0.0..=1.0).contains(&v), "UV 必须归一化：{v}");
    }
    assert!(q.u1 > q.u0 && q.v1 > q.v0, "UV 区域不能退化");
}
