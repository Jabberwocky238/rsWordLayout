//! Immutable font ownership and deterministic lookup; no Word synthesis policy.
//! ```compile_fail
//! use docx_layout::fontenv::FontEnvironment;
//! let env = FontEnvironment {};
//! ```
use crate::digest::{bytes_sha256, canonical_json, digest};
use serde::Serialize;
use skrifa::{FontRef, MetadataProvider, attribute::Style, string::StringId};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use unicode_normalization::UnicodeNormalization;

pub const NORMALIZATION_VERSION: &str =
    "nfkc-lower-no-whitespace-hyphen/1-unicode-normalization-0.1.24";
pub fn normalize_family(name: &str) -> String {
    name.nfkc()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FaceId {
    sha256: String,
    index: u32,
}
impl FaceId {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn index(&self) -> u32 {
        self.index
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct FaceInfo {
    id: FaceId,
    families: BTreeSet<String>,
    weight: u16,
    italic: bool,
}
impl FaceInfo {
    pub fn id(&self) -> &FaceId {
        &self.id
    }
    pub fn families(&self) -> &BTreeSet<String> {
        &self.families
    }
    pub fn weight(&self) -> u16 {
        self.weight
    }
    pub fn italic(&self) -> bool {
        self.italic
    }
}
#[derive(Clone, Debug)]
struct Face {
    info: FaceInfo,
    bytes: Arc<[u8]>,
}
#[derive(Default)]
pub struct FontEnvironmentBuilder {
    faces: BTreeMap<FaceId, Face>,
    aliases: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Debug)]
pub struct FontEnvironment {
    faces: BTreeMap<FaceId, Face>,
    aliases: BTreeMap<String, Vec<String>>,
    fingerprint: String,
}
impl FontEnvironmentBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, bytes: Vec<u8>, face_index: u32) -> Result<FaceId, &'static str> {
        let font = FontRef::from_index(&bytes, face_index).map_err(|_| "FONT_INVALID")?;
        let attributes = font.attributes();
        let families: BTreeSet<_> = [StringId::FAMILY_NAME, StringId::TYPOGRAPHIC_FAMILY_NAME]
            .into_iter()
            .flat_map(|id| font.localized_strings(id))
            .map(|s| normalize_family(&s.to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        if families.is_empty() {
            return Err("FONT_FAMILY_MISSING");
        }
        let id = FaceId {
            sha256: bytes_sha256(&bytes),
            index: face_index,
        };
        let info = FaceInfo {
            id: id.clone(),
            families,
            weight: attributes.weight.value() as u16,
            italic: matches!(attributes.style, Style::Italic),
        };
        self.faces.entry(id.clone()).or_insert(Face {
            info,
            bytes: bytes.into(),
        });
        Ok(id)
    }
    pub fn set_aliases(&mut self, family: &str, candidates: &[String]) {
        self.aliases.insert(
            normalize_family(family),
            candidates.iter().map(|s| normalize_family(s)).collect(),
        );
    }
    pub fn clear(&mut self) {
        self.faces.clear();
        self.aliases.clear();
    }
    pub fn freeze(&self) -> FontEnvironment {
        let metadata: Vec<_> = self.faces.values().map(|f| &f.info).collect();
        let fingerprint = digest(
            "font-environment/1",
            &[
                NORMALIZATION_VERSION.as_bytes(),
                &canonical_json(&metadata),
                &canonical_json(&self.aliases),
            ],
        );
        FontEnvironment {
            faces: self.faces.clone(),
            aliases: self.aliases.clone(),
            fingerprint,
        }
    }
}
#[derive(Debug)]
pub struct FontSelection<'a> {
    pub face: Option<&'a FaceInfo>,
    pub diagnostics: Vec<&'static str>,
}
impl FontEnvironment {
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn faces(&self) -> impl Iterator<Item = &FaceInfo> {
        self.faces.values().map(|f| &f.info)
    }
    pub fn data(&self, id: &FaceId) -> Option<&[u8]> {
        self.faces.get(id).map(|f| f.bytes.as_ref())
    }
    pub fn covers(&self, id: &FaceId, codepoint: char) -> bool {
        self.faces.get(id).is_some_and(|f| {
            FontRef::from_index(&f.bytes, id.index)
                .expect("validated font")
                .charmap()
                .map(codepoint)
                .is_some_and(|g| g.to_u32() != 0)
        })
    }
    pub fn candidates(&self, family: &str, weight: u16, italic: bool) -> Vec<&FaceInfo> {
        let name = normalize_family(family);
        let mut candidates: Vec<_> = self
            .faces()
            .filter(|f| f.families.contains(&name))
            .collect();
        candidates.sort_by_key(|f| (f.italic != italic, f.weight.abs_diff(weight), &f.id));
        candidates
    }
    /// Caller supplies the ordered slot/default/fallback families. Slot rules remain gated.
    pub fn select(
        &self,
        families: &[String],
        weight: u16,
        italic: bool,
        codepoint: char,
    ) -> FontSelection<'_> {
        let mut seen = BTreeSet::new();
        let mut missing_glyph = false;
        for (priority, family) in families.iter().enumerate() {
            let name = normalize_family(family);
            let names = std::iter::once(&name).chain(self.aliases.get(&name).into_iter().flatten());
            for (alias, family) in names.enumerate() {
                for face in self.candidates(family, weight, italic) {
                    if !seen.insert(face.id.clone()) {
                        continue;
                    }
                    if self.covers(&face.id, codepoint) {
                        return FontSelection {
                            face: Some(face),
                            diagnostics: if missing_glyph {
                                vec!["FONT_FALLBACK_GLYPH"]
                            } else if priority == 0 && alias == 0 {
                                vec![]
                            } else {
                                vec!["FONT_SUBSTITUTED"]
                            },
                        };
                    }
                    missing_glyph = true;
                }
            }
        }
        for face in self.faces() {
            if self.covers(&face.id, codepoint) {
                return FontSelection {
                    face: Some(face),
                    diagnostics: vec!["FONT_FALLBACK_GLYPH"],
                };
            }
        }
        FontSelection {
            face: None,
            diagnostics: vec!["FONT_MISSING"],
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const REGULAR: &[u8] = include_bytes!("../../fixtures/fonts/regular.ttf");
    const ITALIC: &[u8] = include_bytes!("../../fixtures/fonts/italic.ttf");
    const FALLBACK: &[u8] = include_bytes!("../../fixtures/fonts/fallback.ttf");
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn f06_order_ownership_and_coverage() {
        let mut a = FontEnvironmentBuilder::new();
        let mut b = FontEnvironmentBuilder::new();
        let mut host = REGULAR.to_vec();
        a.add(host.clone(), 0).unwrap();
        host.fill(0);
        for bytes in [ITALIC, FALLBACK] {
            a.add(bytes.to_vec(), 0).unwrap();
        }
        for bytes in [FALLBACK, ITALIC, REGULAR] {
            b.add(bytes.to_vec(), 0).unwrap();
        }
        let frozen = a.freeze();
        a.clear();
        assert_eq!(frozen.fingerprint(), b.freeze().fingerprint());
        let italic = frozen.candidates("Ｋernel-Sans", 400, true);
        assert!(italic[0].italic());
        assert_eq!(italic[0].weight(), 700);
        let selected = frozen.select(&["Kernel Sans".into()], 400, false, '\u{4e00}');
        assert!(selected.face.unwrap().families.contains("hiddenfallback"));
        assert_eq!(selected.diagnostics, ["FONT_FALLBACK_GLYPH"]);
        assert_eq!(
            frozen.select(&[], 400, false, '\u{10ffff}').diagnostics,
            ["FONT_MISSING"]
        );
        assert_eq!(frozen.faces().count(), 3);
        b.add(REGULAR.to_vec(), 0).unwrap();
        assert_eq!(b.freeze().faces().count(), 3);
        b.set_aliases("Alias", &["Kernel Sans".into()]);
        assert_ne!(frozen.fingerprint(), b.freeze().fingerprint());
        assert_eq!(
            b.freeze()
                .select(&["Alias".into()], 400, false, 'A')
                .diagnostics,
            ["FONT_SUBSTITUTED"]
        );
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn f06_collection_face_identity() {
        let bytes = include_bytes!("../../fixtures/fonts/two-faces.ttc");
        let mut builder = FontEnvironmentBuilder::new();
        let a = builder.add(bytes.to_vec(), 0).unwrap();
        let b = builder.add(bytes.to_vec(), 1).unwrap();
        assert_ne!(a, b);
        assert_eq!(a.sha256(), b.sha256());
        assert!(builder.add(bytes.to_vec(), 2).is_err());
        assert!(builder.add(vec![0; 20], 0).is_err());
    }
}
