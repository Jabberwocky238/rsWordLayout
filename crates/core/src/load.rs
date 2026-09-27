//! 打开 docx，取 rsword 的模型 JSON——之前先补解析器的一处缺口。
//!
//! # 缺口：同一 `w:r` 里并排多个 `w:rPr`
//!
//! 这不合 schema（`CT_R` 只允许一个 `w:rPr`，且在最前），但真有这样的文件，
//! word_analyse 的 `latinspace.docx`、`latinscale.docx` 就是：
//!
//! ```xml
//! <w:r><w:rPr><w:spacing w:val="20"/></w:rPr>
//!      <w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="24"/></w:rPr>
//!      <w:t>…</w:t></w:r>
//! ```
//!
//! 两边的做法**不一样**：
//!
//! - rsWordParser（rev 399e36a）`model::build_run` 每遇到一个 `w:rPr` 就整份重读、覆盖，
//!   **只留最后一个**——上面那个 run 的 `spacing` 在 JSON 里根本不存在。
//!   它自己的编辑层 `MutationPlan::rpr_of` 却取**第一个**，解析器内部也不一致；
//! - 手机 Word **用上了第一个**：`latinspace` 窄路径 24 行起点与「Calibri 12pt + 每字符 20 twips」
//!   逐项相同，`latinscale` 16 行与「80% 缩放」相容。第二个 `w:rPr` 用没用上**判不了**：
//!   它写的 Calibri 12pt 恰好就是这条路径的默认（`breakme` 没有 `w:rFonts`/`w:sz`，
//!   也按 Calibri 12pt 排）。
//!
//! # 这里怎么补
//!
//! 把并排的 `w:rPr` **拼成一个**（去掉中间的 `</w:rPr><w:rPr>`），换掉主 part，再取 JSON。
//! 拼接之后由解析器自己的读法定优先级：
//! 同一 `w:rPr` 里重复的属性**先出现的算数**，于是结果是「逐属性先到先得」。
//! 这与上面的实测相容；两个 `w:rPr` 写了**同一属性**时 Word 取哪个，**没测**。
//!
//! 换主 part **不走会话的编辑接口**（`SessionTable::apply` 的 `replacePartXml`）：一调用 `apply`，
//! 解析器整套编辑引擎就链接进每个入口（layout-trace、svg、wasm、cffi），而用得上的只是极少数
//! 不合 schema 的文件（word_analyse 215 份夹具里 2 份）。仓库要 wasm 小（字体运行时取、不编进去，
//! 见 `crates/webgl/web/src/main.js`）。实测字节数（release，同一台机器；加字符间距与本模块之前 →
//! 本模块走 `apply` → 现在这样）：
//!
//! - webgl `wasm32-unknown-unknown`：4,971,905 → 9,352,680 → 5,008,021；
//! - cffi 动态库：2,864,832 → 7,953,504 → 2,905,856；
//! - layout-trace（`fontenv`）：4,172,000 → 9,329,968 → 4,211,088。
//!
//! 现在的差额（约 36–41KB）包括字符间距的全部新代码，不只本模块。换主 part 的做法见
//! [`document_with_main_part`]。
//!
//! 这是垫片，不是正解：正解在解析器（`build_run` 合并而不是覆盖，并报诊断）。
//! 解析器修好、rev 升上来之后，[`load_document`] 退化成直通，本模块可以删掉
//! （`tests/letter_spacing.rs` 的 `parser_alone_keeps_only_the_last_run_props` 届时会失败，提醒这件事）。
//!
//! # 实际改到的范围
//!
//! 比「同一 `w:r` 里并排的 `w:rPr`」宽，也比它窄：
//!
//! - **只动主 part**。页眉页脚、脚注、批注里的并排 `w:rPr` 不补（桥接层本来就不读它们）；
//! - **按文本扫，不看父元素**：`w:pPr` 里并排的 `w:rPr`（段落标记的属性）也会被并；
//!   XML 注释与 CDATA 里恰好出现的 `</w:rPr><w:rPr>` 也会被删掉；
//! - 第一个 `w:rPr` 以 `w:rPrChange` 结尾时，第二个的子元素接在 `w:rPrChange` **后面**，
//!   不合 schema 的顺序。解析器按元素名逐个读子元素（`build/props.rs` 生成的读法），不查顺序，
//!   读数不受影响；
//! - 两个 `w:rPr` 写了同一属性时，胜者从解析器原来的「后到的」变成「先到的」（见上，没测）；
//! - 合并过的文件，合并点之后的 DOM 节点号每处少一（`Para::source_node` 与直接打开的不同）。
//!
//! # 尽力而为
//!
//! 第一次 `document()` 成功之后的每一步（扫主 part、再开一遍包、换主 part、重建、投影）失败，
//! 都交回**未合并**的 JSON，原因记在 [`LoadedDocument::merge_error`]：补丁失手不该让
//! 本来打得开的文档变成打不开。

use rsword::EditSession;
use rsword::bind::native::{DocumentOpts, SessionTable, document_json};
use rsword::package::Package;
use serde_json::Value;

/// [`load_document`] 的产物。
#[derive(Debug, Clone)]
pub struct LoadedDocument {
    /// `SessionTable::document()` 的 JSON，并排的 `w:rPr` 已合并（合并失败时是原样的）。
    pub json: Value,
    /// 合并掉的 `w:rPr` 边界个数。非零说明原文件不合 schema，调用方应当报出来。
    pub merged_run_props: usize,
    /// 合并那一步失败的原因。`Some` 时 `json` 是未合并的原样结果，调用方应当报出来。
    pub merge_error: Option<String>,
}

/// 打开一份 docx，返回模型 JSON。见模块文档。
///
/// 常见情形（没有并排的 `w:rPr`）只多扫一遍会话里主 part 的源文本，JSON 不变，也不再开第二遍包。
/// 只有打不开、或第一次取 JSON 就失败时才返回 `Err`；之后的失败见 [`LoadedDocument::merge_error`]。
pub fn load_document(bytes: &[u8]) -> Result<LoadedDocument, Box<dyn std::error::Error>> {
    let mut sessions = SessionTable::default();
    let id = sessions.open(bytes, None)?;
    let result = load_open(&mut sessions, &id, bytes);
    sessions.close(&id);
    result
}

fn load_open(
    sessions: &mut SessionTable,
    id: &str,
    bytes: &[u8],
) -> Result<LoadedDocument, Box<dyn std::error::Error>> {
    let json: Value = serde_json::from_str(&sessions.document(id, None)?)?;
    Ok(best_effort(json, |json| merge_open(sessions, id, bytes, json)))
}

/// 合并一步的结果：`Ok(None)` 是没有可合并的（或不改写），`Ok(Some)` 是合并后的 JSON 与处数。
type MergeResult = Result<Option<(Value, usize)>, Box<dyn std::error::Error>>;

/// 合并失败就退回 `json` 原样，并记下原因。单独拆出来是为了能测失败那一支——
/// 真实文件里合并后的 XML 总是良构的（拼接的是良构文档里相邻的一对兄弟），很难让解析器在这里失手。
fn best_effort(json: Value, merge: impl FnOnce(&Value) -> MergeResult) -> LoadedDocument {
    match merge(&json) {
        Ok(Some((merged, n))) => {
            LoadedDocument { json: merged, merged_run_props: n, merge_error: None }
        }
        Ok(None) => LoadedDocument { json, merged_run_props: 0, merge_error: None },
        Err(error) => {
            LoadedDocument { json, merged_run_props: 0, merge_error: Some(error.to_string()) }
        }
    }
}

fn merge_open(sessions: &SessionTable, id: &str, bytes: &[u8], json: &Value) -> MergeResult {
    // 扫的是会话已经解析好的主 part 源文本：常见情形不再开一遍包、不再解析，也不拷贝。
    // 没转码的 part，这段文本与 zip 里的原字节相同（BOM 也在）。`inspect` 在 399e36a 里是
    // `#[doc(hidden)]` 的只读访问器；公开的 `part_bytes` 要为改过的 part 备着序列化器
    // （webgl 的 wasm 实测多 43,395 字节），还要把整个 zip 拷一份、再解压一遍主 part。
    let merged = sessions.inspect(id, |session, _| {
        let dom = session.dom();
        // 转码过的（UTF-16 编码的 part）不改写：宁可保留解析器原样的结果。
        if dom.transcoded() {
            return None;
        }
        let xml = dom.src();
        merge_sibling_run_props(xml.strip_prefix('\u{feff}').unwrap_or(xml))
    })?;
    let Some((merged_xml, merged)) = merged else {
        return Ok(None);
    };
    Ok(Some((document_with_main_part(bytes, &merged_xml, json)?, merged)))
}

/// 把 `bytes` 的主 part 换成 `xml` 之后的模型 JSON，与 `SessionTable::document()` 同形。
///
/// 不经会话的 `apply`（见模块文档）。走的是会话打开时的同一串调用，中间插一步换主 part：
/// `SessionTable::open` 是 `EditSession::open` = `EditSession::from_package(Package::open(bytes)?)`
/// （重建模型、给修订编号），`document()` 是 `document_json` 再加几个顶层键；`replacePartXml`
/// 这条编辑落到包上也是 `Package::replace_part_xml`。所以结果就是「主 part 本来就是 `xml`」的
/// 文件新开会话取的 JSON。不用公开的 `EditSession::replace_part_xml`：它沿用第一次打开时的
/// 修订编号，合并点之后节点号变了的修订会换新号，与新开的不同。
///
/// `SessionTable::document()` 在 `document_json` 之上另加的顶层键（399e36a 里是 BIND-10 的
/// `totalBlocks` 与 `truncated`）从 `session_json`（同一个包未合并时的结果）抄过来：
/// 合并只删 `w:rPr` 之间的标签，不增删块，这些键两边相同。媒体句柄不用抄：
/// `document_json` 与新开的会话按同一顺序给图片 part 编号。
/// `tests/letter_spacing.rs` 的 `load_document_merge_matches_the_session_on_a_merged_file`
/// 把整份 JSON 与「主 part 本来就是合并后 XML」的文件经会话取的结果比对，钉住同形。
///
/// `EditSession::from_package` 在 399e36a 里是 `#[doc(hidden)]` 的公开函数；rev 升级时若它没了，
/// 这里编译不过，不会悄悄变样。
fn document_with_main_part(
    bytes: &[u8],
    xml: &str,
    session_json: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut pkg = Package::open(bytes)?;
    let main = pkg.main_part();
    pkg.replace_part_xml(main, xml)?;
    let session = EditSession::from_package(pkg)?;
    let mut json =
        document_json(session.package(), session.document(), DocumentOpts { display: false }).0;
    if let (Some(out), Some(extra)) = (json.as_object_mut(), session_json.as_object()) {
        for (key, value) in extra {
            if !out.contains_key(key) {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(json)
}

/// 把**紧挨着的兄弟** `rPr` 拼成一个：返回（新 XML，合并处数）；没有可合并的就 `None`。
///
/// 只认同一前缀的 `rPr`（`m:rPr` 后面跟 `w:rPr` 是 OMML 的正常写法，不动），
/// 中间只许有空白。第二个若是空元素 `<w:rPr/>`，直接删掉它——解析器「留最后一个」
/// 会让它把前面的属性全部清空。第二个带属性时不动（`w:rPr` 没有属性，见到了说明
/// 不是这里设想的情形，不猜）。
///
/// 这是按文本扫的，不是 XML 解析：注释与 CDATA 里恰好出现 `</w:rPr><w:rPr>` 也会被改。
/// 作为垫片够用；正解见模块文档。
pub fn merge_sibling_run_props(xml: &str) -> Option<(String, usize)> {
    // 待删区间，按起点升序、互不重叠。
    let mut drops: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while let Some(offset) = xml[at..].find("</") {
        let start = at + offset;
        let Some((qname, end)) = end_tag(xml, start) else {
            at = start + 2;
            continue;
        };
        if qname.rsplit(':').next() != Some("rPr") {
            at = end;
            continue;
        }
        // 往后看：跳过空白与空的 `<P:rPr/>`，直到遇到一个带内容的 `<P:rPr>` 或别的东西。
        let mut cursor = end;
        let mut empties: Vec<(usize, usize)> = Vec::new();
        let mut open_at = None;
        loop {
            let next = skip_ws(xml, cursor);
            match start_tag_without_attrs(xml, next, qname) {
                Some((tag_end, true)) => {
                    empties.push((cursor, tag_end));
                    cursor = tag_end;
                }
                Some((tag_end, false)) => {
                    open_at = Some(tag_end);
                    break;
                }
                None => break,
            }
        }
        match open_at {
            // `</P:rPr> … <P:rPr>`：整段删掉，前后两个的子元素就接在同一个 `rPr` 里。
            // 夹在中间的空元素一并落在这段里。
            Some(tag_end) => {
                drops.push((start, tag_end));
                at = tag_end;
            }
            None => {
                drops.extend(empties);
                at = cursor.max(end);
            }
        }
    }
    if drops.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(xml.len());
    let mut copied = 0;
    for &(from, to) in &drops {
        out.push_str(&xml[copied..from]);
        copied = to;
    }
    out.push_str(&xml[copied..]);
    Some((out, drops.len()))
}

/// `start` 处若是结束标签 `</qname>`（`>` 前可有空白），返回（qname，标签之后的偏移）。
fn end_tag(xml: &str, start: usize) -> Option<(&str, usize)> {
    let rest = xml.get(start + 2..)?;
    let name_len = rest.find(|c: char| c == '>' || c.is_ascii_whitespace())?;
    let qname = &rest[..name_len];
    if qname.is_empty() {
        return None;
    }
    let after = skip_ws(xml, start + 2 + name_len);
    xml[after..].starts_with('>').then_some((qname, after + 1))
}

/// `at` 处若是**不带属性**的开始标签 `<qname>` 或 `<qname/>`，返回（标签之后的偏移，是否自闭合）。
fn start_tag_without_attrs(xml: &str, at: usize, qname: &str) -> Option<(usize, bool)> {
    let rest = xml.get(at..)?.strip_prefix('<')?.strip_prefix(qname)?;
    // 名字之后只许空白再接 `>` 或 `/>`：`<w:rPrChange>` 这类同前缀的别的元素、
    // 带属性的 `<w:rPr a="…">` 都在这里落空。
    let after = skip_ws(xml, xml.len() - rest.len());
    let tail = &xml[after..];
    if tail.starts_with("/>") {
        Some((after + 2, true))
    } else if tail.starts_with('>') {
        Some((after + 1, false))
    } else {
        None
    }
}

fn skip_ws(xml: &str, at: usize) -> usize {
    at + xml[at..].len() - xml[at..].trim_start_matches(|c: char| c.is_ascii_whitespace()).len()
}

#[cfg(test)]
mod tests {
    use super::{LoadedDocument, best_effort};

    #[test]
    fn a_failed_merge_falls_back_to_the_unmerged_json() {
        let json = serde_json::json!({"mainPart": 3, "main": []});
        let LoadedDocument { json: got, merged_run_props, merge_error } =
            best_effort(json.clone(), |_| Err("replacePartXml 失败".into()));
        assert_eq!(got, json, "合并失败时交回原样的 JSON");
        assert_eq!(merged_run_props, 0);
        assert_eq!(merge_error.as_deref(), Some("replacePartXml 失败"));

        // 对照：没有可合并的，原样且不报错；合并成功，换成合并后的。
        let none = best_effort(json.clone(), |_| Ok(None));
        assert_eq!((none.json, none.merged_run_props, none.merge_error), (json.clone(), 0, None));
        let merged = serde_json::json!({"mainPart": 3, "main": [1]});
        let some = best_effort(json, |_| Ok(Some((merged.clone(), 2))));
        assert_eq!((some.json, some.merged_run_props, some.merge_error), (merged, 2, None));
    }
}
