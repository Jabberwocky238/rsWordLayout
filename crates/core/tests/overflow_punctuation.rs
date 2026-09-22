use rsword_layout_core::font::OverflowPunctuationContext;
use rsword_layout_core::{BreakOpportunity, FontMetrics, FontSpec, SimpleMetrics, TextMetrics};
use std::cell::Cell;

const OBSERVED: [char; 4] = ['\u{3002}', '\u{ff0c}', '\u{ff09}', '\u{3001}'];

fn assert_observed_boundary(metrics: &impl FontMetrics, font: &FontSpec) {
    let prefix = "\u{4e2d}".repeat(37);
    let width = metrics.measure(&prefix, font).advance;
    for punctuation in OBSERVED {
        let text = format!("{prefix}{punctuation}\u{6587}");
        let (ordinary, _) = metrics
            .fit_with_overflow_punctuation(&text, font, width, false)
            .unwrap();
        let (overflow, measured) = metrics
            .fit_with_overflow_punctuation(&text, font, width, true)
            .unwrap();
        assert_eq!(text[..ordinary].chars().count(), 36, "{punctuation}");
        assert_eq!(text[..overflow].chars().count(), 38, "{punctuation}");
        assert_eq!(measured, metrics.measure(&text[..overflow], font));
        assert!(measured.advance > width);
    }
}

#[test]
fn observed_punctuation_changes_36_characters_to_38() {
    assert_observed_boundary(&SimpleMetrics, &FontSpec::new("synthetic", 20));
}

#[cfg(feature = "fontenv")]
#[test]
fn bundled_cjk_font_has_the_same_overflow_boundary() {
    use rsword_layout_core::{RealMetrics, font::FontRegistry};

    let mut registry = FontRegistry::new();
    registry
        .add(
            include_bytes!("../../../fixtures/fonts/DroidSansFallbackFull.ttf").to_vec(),
            0,
        )
        .unwrap();
    assert!(registry.covers_family("Droid Sans Fallback"));
    assert_observed_boundary(
        &RealMetrics::new(&registry),
        &FontSpec::new("Droid Sans Fallback", 20),
    );
}

#[test]
fn a_prefix_that_does_not_fit_cannot_overflow() {
    let font = FontSpec::new("synthetic", 20);
    let text = format!("{}\u{3002}\u{6587}", "\u{4e2d}".repeat(37));
    let width = SimpleMetrics.measure(&"\u{4e2d}".repeat(37), &font).advance - 1;
    assert_eq!(
        SimpleMetrics.fit_with_overflow_punctuation(&text, &font, width, true),
        SimpleMetrics.fit(&text, &font, width)
    );
}

#[test]
fn punctuation_that_fits_uses_the_normal_break() {
    let font = FontSpec::new("synthetic", 20);
    let text = "\u{4e2d}\u{3002}\u{6587}";
    let width = SimpleMetrics.measure("\u{4e2d}\u{3002}", &font).advance;
    assert_eq!(
        SimpleMetrics.fit_with_overflow_punctuation(text, &font, width, true),
        SimpleMetrics.fit(text, &font, width)
    );
}

#[test]
fn consecutive_closing_punctuation_does_not_gain_overflow() {
    let font = FontSpec::new("synthetic", 20);
    for suffix in [
        "\u{3002}\u{ff1b}",
        "\u{ff09}\u{3002}",
        "\u{ff0c}\u{3001}",
        "\u{3002})",
    ] {
        let text = format!("\u{4e2d}{suffix}\u{6587}");
        for width in [200, 400] {
            assert_eq!(
                SimpleMetrics.fit_with_overflow_punctuation(&text, &font, width, true),
                SimpleMetrics.fit(&text, &font, width),
                "{suffix}, width {width}"
            );
        }
    }
}

#[test]
fn western_and_unobserved_punctuation_keep_existing_fit() {
    let font = FontSpec::new("synthetic", 20);
    for text in [
        "A\u{3002}\u{6587}",
        "A,B",
        "\u{4e2d}.\u{6587}",
        "\u{3002}\u{6587}",
        "",
    ] {
        assert_eq!(
            SimpleMetrics.fit_with_overflow_punctuation(text, &font, 100, true),
            SimpleMetrics.fit(text, &font, 100),
            "{text}"
        );
    }
    for punctuation in [
        '\u{ff1b}', '\u{ff1a}', '\u{ff1f}', '\u{ff01}', '\u{3011}', '\u{300b}', '\u{300d}',
        '\u{300f}',
    ] {
        let text = format!("\u{4e2d}{punctuation}\u{6587}");
        assert_eq!(
            SimpleMetrics.fit_with_overflow_punctuation(&text, &font, 200, true),
            SimpleMetrics.fit(&text, &font, 200),
            "{punctuation}"
        );
    }
}

#[test]
fn end_of_fragment_and_supplementary_cjk_use_byte_offsets() {
    let font = FontSpec::new("synthetic", 20);
    let text = "\u{20000}\u{3002}";
    let (offset, measured) = SimpleMetrics
        .fit_with_overflow_punctuation(text, &font, 200, true)
        .unwrap();
    assert_eq!(offset, text.len());
    assert_eq!(measured.advance, 400);
    assert_eq!(
        SimpleMetrics.fit_with_overflow_punctuation(text, &font, 0, true),
        None
    );
}

struct CustomFit;

impl FontMetrics for CustomFit {
    fn measure(&self, _: &str, _: &FontSpec) -> TextMetrics {
        panic!("the original fit override must handle this request")
    }

    fn break_opportunities(&self, _: &str) -> Vec<BreakOpportunity> {
        panic!("the original fit override must handle this request")
    }

    fn fit(&self, _: &str, _: &FontSpec, _: i32) -> Option<(usize, TextMetrics)> {
        Some((
            1,
            TextMetrics {
                advance: 17,
                ..TextMetrics::default()
            },
        ))
    }
}

#[test]
fn disabled_and_ineligible_calls_preserve_custom_fit_overrides() {
    let font = FontSpec::new("synthetic", 20);
    assert_eq!(
        CustomFit.fit_with_overflow_punctuation("\u{4e2d}\u{3002}", &font, 20, false),
        CustomFit.fit("\u{4e2d}\u{3002}", &font, 20)
    );
    assert_eq!(
        CustomFit.fit_with_overflow_punctuation("ABC", &font, 20, true),
        CustomFit.fit("ABC", &font, 20)
    );
}

#[test]
fn fragment_edges_use_adjacent_run_context() {
    let font = FontSpec::new("synthetic", 20);
    let context = OverflowPunctuationContext {
        previous: Some('\u{4e2d}'),
        next: None,
    };
    let (end, measured) = SimpleMetrics
        .fit_with_overflow_punctuation_context("\u{3002}\u{6587}", &font, 0, true, context)
        .unwrap();
    assert_eq!(end, "\u{3002}".len());
    assert_eq!(measured.advance, 200);
    for previous in [None, Some('A'), Some('\u{3002}')] {
        assert_eq!(
            SimpleMetrics.fit_with_overflow_punctuation_context(
                "\u{3002}\u{6587}",
                &font,
                0,
                true,
                OverflowPunctuationContext {
                    previous,
                    next: None
                },
            ),
            None,
        );
    }
    for next in [Some('\u{ff1b}'), Some(')')] {
        let text = "\u{4e2d}\u{3002}";
        assert_eq!(
            SimpleMetrics.fit_with_overflow_punctuation_context(
                text,
                &font,
                200,
                true,
                OverflowPunctuationContext {
                    previous: None,
                    next
                },
            ),
            SimpleMetrics.fit(text, &font, 200),
        );
    }
}

#[derive(Default)]
struct CountingMetrics {
    measures: Cell<usize>,
    break_bytes: Cell<usize>,
}

impl FontMetrics for CountingMetrics {
    fn measure(&self, text: &str, font: &FontSpec) -> TextMetrics {
        self.measures.set(self.measures.get() + 1);
        SimpleMetrics.measure(text, font)
    }

    fn break_opportunities(&self, text: &str) -> Vec<BreakOpportunity> {
        self.break_bytes.set(self.break_bytes.get() + text.len());
        SimpleMetrics.break_opportunities(text)
    }
}

#[test]
fn overflow_checks_do_not_refit_every_later_punctuation_prefix() {
    let font = FontSpec::new("synthetic", 20);
    for repetitions in [20, 400, 2000] {
        let text = "\u{4e2d}\u{3002}\u{6587}".repeat(repetitions);
        let metrics = CountingMetrics::default();
        assert_eq!(
            metrics.fit_with_overflow_punctuation(&text, &font, 400, true),
            SimpleMetrics.fit(&text, &font, 400),
        );
        assert!(
            metrics.measures.get() <= 5,
            "{} measurements",
            metrics.measures.get()
        );
        assert!(metrics.break_bytes.get() <= text.len() * 2);
    }
}

#[test]
fn paragraph_wrapping_does_not_add_an_extra_complexity_factor() {
    use rsword_layout_core::{Color, Engine, Margins, PageSetup, Para, Run, Size};

    for repetitions in [40, 80, 160] {
        let run = Run {
            text: "\u{4e2d}\u{3002}".repeat(repetitions),
            font: FontSpec::new("synthetic", 20),
            color: Color::BLACK,
            placeholders: vec![],
            rise: 0,
        };
        let setup = PageSetup {
            size: Size::new(400, 100000),
            margins: Margins::uniform(0),
        };
        let counters: Vec<_> = [false, true]
            .into_iter()
            .map(|overflow_punct| {
                let metrics = CountingMetrics::default();
                Engine::new(&metrics, setup).layout(&[Para {
                    runs: vec![run.clone()],
                    overflow_punct,
                    ..Para::default()
                }]);
                (metrics.measures.get(), metrics.break_bytes.get())
            })
            .collect();
        assert!(counters[1].0 <= counters[0].0 * 2, "{counters:?}");
        assert!(counters[1].1 <= counters[0].1 * 2, "{counters:?}");
    }
}
