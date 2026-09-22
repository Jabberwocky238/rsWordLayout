//! AAT optional ligatures can be enabled by the font's default chain flags.
//! Build a tiny morx state machine over the checked-in DejaVu font, so this
//! exercises real AAT shaping without requiring Apple's proprietary Zapfino.

#![cfg(feature = "shape")]

use rsword_layout_core::RustybuzzShaper;
use rustybuzz::{Face, UnicodeBuffer, ttf_parser::Tag};

const DEJAVU: &[u8] = include_bytes!("../../../fixtures/fonts/DejaVuSans.ttf");

fn u16s(bytes: &mut Vec<u8>, values: &[u16]) {
    for value in values {
        bytes.extend(value.to_be_bytes());
    }
}

fn u32s(bytes: &mut Vec<u8>, values: &[u32]) {
    for value in values {
        bytes.extend(value.to_be_bytes());
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn aat_font(on_selector: u16) -> Vec<u8> {
    let face = Face::from_slice(DEJAVU, 0).unwrap();
    let d = face.glyph_index('d').unwrap().0;
    let i = face.glyph_index('i').unwrap().0;
    let ligature = face.glyph_index('\u{fb01}').unwrap().0;

    // AAT feature type 1 is ligatures, with an even on-selector and odd off.
    let mut feat = Vec::new();
    u32s(&mut feat, &[0x0001_0000]);
    u16s(&mut feat, &[1, 0, 0, 0, 1, 1]);
    u32s(&mut feat, &[24]);
    u16s(&mut feat, &[0, 256, on_selector, 257]);

    // Extended state header: six classes, three states, three entries.
    let mut state = Vec::new();
    u32s(&mut state, &[6, 28, 48, 84, 102, 110, 112]);
    // Format-6 class lookup: d -> class 4, i -> class 5.
    u16s(&mut state, &[6, 4, 2, 8, 1, 0, d, 4, i, 5]);
    u16s(
        &mut state,
        &[
            0, 0, 0, 0, 1, 0, // Start of text.
            0, 0, 0, 0, 1, 0, // Start of line.
            0, 0, 0, 0, 1, 2, // Saw d: i completes the ligature.
        ],
    );
    u16s(
        &mut state,
        &[
            0, 0, 0, // Reset.
            2, 0x8000, 0, // Push d, enter state 2.
            0, 0xa000, 0, // Push i, perform ligature action 0.
        ],
    );
    // Both component lookups address the zero entry; the second stores gid fi.
    u32s(
        &mut state,
        &[
            (-(i as i32) as u32) & 0x3fff_ffff,
            0xc000_0000 | ((-(d as i32) as u32) & 0x3fff_ffff),
        ],
    );
    u16s(&mut state, &[0, ligature]);

    let mut subtable = Vec::new();
    u32s(&mut subtable, &[12 + state.len() as u32, 2, 1]);
    subtable.extend(state);

    let mut morx = Vec::new();
    u32s(&mut morx, &[0x0002_0000, 1]);
    // Enable the ligature by default, and expose both feature selectors.
    u32s(&mut morx, &[1, 40 + subtable.len() as u32, 2, 1]);
    u16s(&mut morx, &[1, on_selector]);
    u32s(&mut morx, &[1, u32::MAX]);
    u16s(&mut morx, &[1, on_selector + 1]);
    u32s(&mut morx, &[0, !1]);
    morx.extend(subtable);

    // Rebuild an ordinary sfnt, replacing OpenType substitution with AAT.
    let mut tables: Vec<_> = face
        .raw_face()
        .table_records
        .into_iter()
        .filter(|record| record.tag != Tag::from_bytes(b"GSUB"))
        .map(|record| {
            (
                record.tag,
                face.raw_face().table(record.tag).unwrap().to_vec(),
            )
        })
        .collect();
    tables.push((Tag::from_bytes(b"feat"), feat));
    tables.push((Tag::from_bytes(b"morx"), morx));
    tables.sort_by_key(|(tag, _)| *tag);

    let count = tables.len() as u16;
    let power = count.ilog2();
    let mut font = Vec::new();
    u32s(&mut font, &[0x0001_0000]);
    u16s(
        &mut font,
        &[
            count,
            (1 << power) * 16,
            power as u16,
            count * 16 - (1 << power) * 16,
        ],
    );
    font.resize(12 + tables.len() * 16, 0);
    let mut head_offset = 0;
    for (index, (tag, mut data)) in tables.into_iter().enumerate() {
        if tag == Tag::from_bytes(b"head") {
            head_offset = font.len();
            data[8..12].fill(0);
        }
        let record = 12 + index * 16;
        for (field, value) in [tag.0, checksum(&data), font.len() as u32, data.len() as u32]
            .into_iter()
            .enumerate()
        {
            font[record + field * 4..record + field * 4 + 4].copy_from_slice(&value.to_be_bytes());
        }
        font.extend(data);
        while font.len() % 4 != 0 {
            font.push(0);
        }
    }
    let adjustment = 0xb1b0_afba_u32.wrapping_sub(checksum(&font));
    font[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    font
}

#[test]
fn optional_aat_ligatures_are_off_even_when_the_font_enables_them_by_default() {
    // Common, rare, contextual and historical AAT ligature selectors.
    for selector in [2, 4, 18, 20] {
        let bytes = aat_font(selector);
        let face = Face::from_slice(&bytes, 0).unwrap();
        assert!(face.tables().gsub.is_none());
        assert!(face.tables().morx.is_some());
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str("di ");
        buffer.guess_segment_properties();
        let default = rustybuzz::shape(&face, &[], buffer);
        assert_eq!(
            default.len(),
            2,
            "selector {selector}: fixture must form a ligature"
        );
        assert_eq!(
            default.glyph_infos()[0].glyph_id,
            u32::from(face.glyph_index('\u{fb01}').unwrap().0)
        );
        let expected: Vec<_> = "di "
            .chars()
            .map(|ch| u32::from(face.glyph_index(ch).unwrap().0))
            .collect();

        let mut shaper = RustybuzzShaper::new();
        shaper.add_face("AAT fixture", bytes, 0);
        for kerning in [false, true] {
            let actual = shaper.shape_with_face(0, "di ", 24, kerning);
            assert_eq!(
                actual
                    .iter()
                    .map(|glyph| glyph.glyph_id)
                    .collect::<Vec<_>>(),
                expected,
                "selector {selector}: optional ligatures must be disabled"
            );
            assert_eq!(
                actual.iter().map(|glyph| glyph.source).collect::<Vec<_>>(),
                [Some((0, 1)), Some((1, 2)), Some((2, 3))]
            );
        }
    }
}

#[test]
fn required_aat_ligatures_are_preserved() {
    let mut shaper = RustybuzzShaper::new();
    shaper.add_face("AAT fixture", aat_font(0), 0);
    let actual = shaper.shape_with_face(0, "di ", 24, false);
    assert_eq!(actual.len(), 2);
    assert_eq!(
        actual.iter().map(|glyph| glyph.source).collect::<Vec<_>>(),
        [Some((0, 2)), Some((2, 3))]
    );
}
