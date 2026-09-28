//! 摘要与规范化 JSON：原 `docx-layout-contract` 里 fontenv 用到的三个函数，原样搬入。
use serde::Serialize;
use sha2::{Digest, Sha256};

/// JSON object keys are ordered by serde_json's BTreeMap representation.
pub(crate) fn canonical_json(value: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(&serde_json::to_value(value).expect("finite validated data"))
        .expect("serializable value")
}

/// Length-prefixed domain and parts prevent cross-type and concatenation aliases.
pub(crate) fn digest(domain: &str, parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in std::iter::once(domain.as_bytes()).chain(parts.iter().copied()) {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

pub(crate) fn bytes_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
