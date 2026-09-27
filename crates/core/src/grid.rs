//! Declared section document grids. These inputs do not yet affect layout metrics.

use serde_json::{Value, json};

use crate::Twips;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridKind {
    Default,
    Lines,
    LinesAndChars,
    SnapToChars,
}

impl GridKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Lines => "lines",
            Self::LinesAndChars => "linesAndChars",
            Self::SnapToChars => "snapToChars",
        }
    }
}

/// A section's native `docGrid` declaration, without inferred defaults or inheritance.
/// Missing, empty, and malformed declarations remain distinct. Accessors parse the
/// pinned parser's JSON types; a parsed pitch still needs positive-value validation
/// before any future grid calculation. `charSpace` is not a twip measurement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentGrid {
    declared: Option<Value>,
}

impl DocumentGrid {
    pub fn from_json(declared: Option<&Value>) -> Self {
        Self {
            declared: declared.cloned(),
        }
    }

    pub fn declared(&self) -> Option<&Value> {
        self.declared.as_ref()
    }

    fn field(&self, name: &str) -> Result<Option<&Value>, String> {
        match &self.declared {
            None => Ok(None),
            Some(Value::Object(fields)) => Ok(fields.get(name)),
            Some(value) => Err(format!("docGrid must be an object, got {value}")),
        }
    }

    pub fn kind(&self) -> Result<Option<GridKind>, String> {
        match self.field("kind")? {
            None => Ok(None),
            Some(Value::String(value)) => match value.as_str() {
                "default" => Ok(Some(GridKind::Default)),
                "lines" => Ok(Some(GridKind::Lines)),
                "linesAndChars" => Ok(Some(GridKind::LinesAndChars)),
                "snapToChars" => Ok(Some(GridKind::SnapToChars)),
                _ => Err(format!(
                    "docGrid.kind is not a recognized parser value: {value}"
                )),
            },
            Some(value) => Err(format!("docGrid.kind cannot be interpreted: {value}")),
        }
    }

    pub fn line_pitch(&self) -> Result<Option<Twips>, String> {
        self.integer("linePitch")
    }

    pub fn char_space(&self) -> Result<Option<i32>, String> {
        self.integer("charSpace")
    }

    fn integer(&self, name: &str) -> Result<Option<i32>, String> {
        self.field(name)?
            .map(|value| {
                value
                    .as_i64()
                    .and_then(|number| i32::try_from(number).ok())
                    .ok_or_else(|| {
                        format!("docGrid.{name} requires a signed 32-bit integer, got {value}")
                    })
            })
            .transpose()
    }

    pub(crate) fn diagnostics(&self) -> Vec<String> {
        let Some(declared) = &self.declared else {
            return Vec::new();
        };
        if !declared.is_object() {
            return vec![format!(
                "docGrid must be an object, got {declared}; input retained but not applied"
            )];
        }
        let mut notes = vec!["docGrid input retained but not applied to grid layout".into()];
        let kind = self.kind();
        match &kind {
            Err(error) => notes.push(error.clone()),
            Ok(None) => notes.push("docGrid.kind absent: default behavior is unresolved".into()),
            Ok(Some(GridKind::LinesAndChars | GridKind::SnapToChars)) => {
                notes.push(
                    "docGrid character-grid kind is unsupported; not converted to lines".into(),
                );
            }
            _ => {}
        }
        match self.line_pitch() {
            Err(error) => notes.push(error),
            Ok(Some(pitch)) if pitch <= 0 => notes.push(format!(
                "docGrid.linePitch={pitch} is nonpositive; retained without line-grid arithmetic"
            )),
            Ok(None) if matches!(kind, Ok(Some(GridKind::Lines | GridKind::LinesAndChars))) => {
                notes.push("docGrid.linePitch absent for an active line-grid kind; cannot calculate a line grid".into());
            }
            _ => {}
        }
        match self.char_space() {
            Err(error) => notes.push(error),
            Ok(Some(_)) => notes.push("docGrid.charSpace is retained as a raw integer; character-grid spacing is unsupported and is not converted to twips".into()),
            Ok(None) => {}
        }
        notes
    }

    pub(crate) fn trace_metadata(&self) -> Value {
        json!({
            "present": self.declared.is_some(),
            "declared": self.declared,
            "applied": false,
            "status": if self.declared.is_some() { "retained-not-applied" } else { "absent" },
            "kind": input_state(self.kind().map(|kind| kind.map(|kind| json!(kind.as_str())))),
            "linePitchTwips": input_state(self.line_pitch().map(|pitch| pitch.map(|pitch| json!(pitch)))),
            "charSpaceRaw": input_state(self.char_space().map(|space| space.map(|space| json!(space)))),
        })
    }
}

fn input_state(value: Result<Option<Value>, String>) -> Value {
    match value {
        Ok(None) => json!({"state": "absent"}),
        Ok(Some(value)) => json!({"state": "declared", "value": value}),
        Err(error) => json!({"state": "invalid", "error": error}),
    }
}
