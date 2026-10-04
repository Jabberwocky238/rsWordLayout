//! 打开 docx，保留声明值 JSON，并为共用布局生成有效属性。
//!
//! [`LoadedDocument::layout_document`] 使用 pinned 解析器的 typed `Resolver`，
//! 合成 docDefaults、默认段落样式、basedOn、字符样式与直接属性，并解析主题字体。
//! 原始 `json` 不被改写。制表位额外按位置合并，因为解析器的通用数组合并会整表覆盖。
//!
//! 解析器的编辑模型会把段落样式隐藏的整段归为 `protected/invisible`。
//! 布局使用同一 DOM 与仅清除分类阶段 vanish 的样式副本恢复这些段，随后用原始
//! 样式表计算有效属性，保留隐藏文字的源位置，也支持 toggle 合成后重新可见的文字。
//! 这不是完整文档布局：表格等保护块仍报告为不支持，复杂文种度量尚未接入。
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
//! 上述历史差额（约 36–41KB）包括字符间距的全部新代码，未计入后续有效属性接入。换主 part 的做法见
//! [`document_with_main_part`]。
//!
//! 这是垫片，不是正解：正解在解析器（`build_run` 合并而不是覆盖，并报诊断）。
//! 解析器修好、rev 升上来之后，可以删除并排属性合并垫片，保留有效属性装载
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
use rsword::bind::native::{DocumentOpts, ProjCx, SessionTable, ToJson, document_json};
use std::borrow::Cow;
use std::collections::BTreeMap;

use rsword::model::{Block, Document, Inline, ProtectedKind, TextKind};
use rsword::package::{Package, RelType};
use rsword::resolve::{EffectiveRunProps, Resolver};
use rsword::semantic::props::{Codec, Ctx, ParaProps, RunProps, StyleType, codec::OnOff};
use rsword::xml::{Dom, LocalName, QName};
use serde_json::Value;

use crate::bridge::{EffectiveProperties, merge_layout_tabs, project_paragraphs};
use crate::layout::Para;
use crate::DocumentCompatibility;

/// [`load_document`] 的产物。
#[derive(Debug, Clone)]
pub struct LoadedDocument {
    /// `SessionTable::document()` 的 JSON，并排的 `w:rPr` 已合并（合并失败时是原样的）。
    pub json: Value,
    /// 合并掉的 `w:rPr` 边界个数。非零说明原文件不合 schema，调用方应当报出来。
    pub merged_run_props: usize,
    /// 合并那一步失败的原因。`Some` 时 `json` 是未合并的原样结果，调用方应当报出来。
    pub merge_error: Option<String>,
    effective: EffectiveProperties,
    compatibility: DocumentCompatibility,
    compatibility_warnings: Vec<Value>,
}

impl LoadedDocument {
    /// 共用页面格式器的输入，包含有效段落属性与文档声明的节几何。
    pub fn layout_document(&self) -> crate::LayoutDocument {
        let json = self.layout_json();
        let mut document = crate::document::document_from_json_with(&json, Some(&self.effective));
        document.compatibility = self.compatibility;
        document.source_warnings.extend(self.compatibility_warnings.iter().cloned());
        // 分节的起页由文档格式器处理；这里只保留继承或直接声明的段落起页。
        for para in &mut document.paras {
            para.page_break_before = para.source_node
                .and_then(|node| self.effective.paras.get(&node))
                .and_then(|props| props.get("pageBreakBefore"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        }
        document
    }

    /// 带样式继承、文档默认值与主题字体的布局段落。
    ///
    /// `json` 仍是解析器的声明值投影；有效属性由同一次解析的 typed 模型生成，
    /// 不靠样式名称推测。第二项计数不支持的主流块，与 `paras_from_document` 相同。
    pub fn paragraphs(&self) -> (Vec<Para>, usize) {
        project_paragraphs(&self.layout_json(), Some(&self.effective))
    }

    fn layout_json(&self) -> Cow<'_, Value> {
        if self.effective.recovered_blocks.is_empty() {
            return Cow::Borrowed(&self.json);
        }
        let mut json = self.json.clone();
        if let Some(main) = json.get_mut("main").and_then(Value::as_array_mut) {
            for block in main {
                if let Some(recovered) = block.get("node").and_then(Value::as_u64)
                    .and_then(|node| u32::try_from(node).ok())
                    .and_then(|node| self.effective.recovered_blocks.get(&node))
                {
                    *block = recovered.clone();
                }
            }
        }
        Cow::Owned(json)
    }
}

/// 打开一份 docx，返回模型 JSON。见模块文档。
///
/// 常见情形不再开第二遍包：扫描主 part 并从同一 typed 模型计算有效属性，声明值 JSON 不变。
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
    let effective = sessions.inspect(id, |session, _| effective_properties(session))?;
    let (compatibility, warnings) = sessions.inspect(id, |session, _| document_compatibility(session))?;
    let mut loaded = best_effort(json, effective, compatibility, |json| merge_open(sessions, id, bytes, json));
    loaded.compatibility_warnings = warnings;
    Ok(loaded)
}

/// The pinned parser (399e36a) keeps compat booleans in `raw_unmodeled` and
/// omits them from native JSON. Read the original settings DOM, including false,
/// without modifying declared JSON or pulling in the editing API.
fn document_compatibility(session: &EditSession) -> (DocumentCompatibility, Vec<Value>) {
    let mut result = DocumentCompatibility::default();
    let Some(compat) = session.document().settings.as_ref().and_then(|s| s.compat.as_ref()) else {
        return (result, Vec::new());
    };
    let package = session.package();
    let settings_part = package.related(package.main_part(), RelType::Settings).next()
        .or_else(|| package.find_name("word/settings.xml"));
    let Some(dom) = settings_part.and_then(|part| package.part(part).dom()) else {
        return (result, Vec::new());
    };
    let mut diagnostics = Vec::new();
    let mut flag = |name| {
        let &node = compat.raw_unmodeled.iter()
            .find(|&&node| dom.name(node) == Some(QName::w(name)))?;
        let mut context = Ctx::new(dom, &mut diagnostics);
        context.enter(node);
        Some(dom.attr_value(node, QName::w(LocalName::Val))
            .is_none_or(|value| OnOff::parse(&value, &mut context)))
    };
    result.split_page_break_and_para_mark = flag(LocalName::SplitPgBreakAndParaMark);
    result.no_column_balance = flag(LocalName::NoColumnBalance);
    let cx = ProjCx { pkg: package, display: false };
    (result, diagnostics.iter().map(|warning| warning.to_json(&cx)).collect())
}

/// 合并一步的结果：`Ok(None)` 无改写；`Ok(Some)` 带重建后的 JSON、有效属性与合并处数。
type MergeResult = Result<Option<(Value, EffectiveProperties, usize)>, Box<dyn std::error::Error>>;

/// 合并失败就退回 `json` 原样，并记下原因。单独拆出来是为了能测失败那一支——
/// 真实文件里合并后的 XML 总是良构的（拼接的是良构文档里相邻的一对兄弟），很难让解析器在这里失手。
fn best_effort(
    json: Value,
    effective: EffectiveProperties,
    compatibility: DocumentCompatibility,
    merge: impl FnOnce(&Value) -> MergeResult,
) -> LoadedDocument {
    match merge(&json) {
        Ok(Some((merged, effective, n))) => {
            LoadedDocument { json: merged, merged_run_props: n, merge_error: None, effective, compatibility, compatibility_warnings: Vec::new() }
        }
        Ok(None) => LoadedDocument { json, merged_run_props: 0, merge_error: None, effective, compatibility, compatibility_warnings: Vec::new() },
        Err(error) => {
            LoadedDocument { json, merged_run_props: 0, merge_error: Some(error.to_string()), effective, compatibility, compatibility_warnings: Vec::new() }
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
    let (json, effective) = document_with_main_part(bytes, &merged_xml, json)?;
    Ok(Some((json, effective, merged)))
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
) -> Result<(Value, EffectiveProperties), Box<dyn std::error::Error>> {
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
    Ok((json, effective_properties(&session)))
}

fn effective_properties(session: &EditSession) -> EffectiveProperties {
    let document = session.document();
    let resolver = Resolver::new(document);
    let cx = ProjCx { pkg: session.package(), display: false };
    let default_style = resolver.default_style(StyleType::Paragraph).and_then(|s| s.id());
    let mut effective = EffectiveProperties::default();
    // 399e36a 的编辑模型会把段落样式 vanish 的整段归为 protected/invisible，
    // 连字符样式或 toggle 合成后可见的 run 也不保留。仅为布局重建这些段的 typed IR：
    // 分类阶段屏蔽样式 vanish，实际属性仍由未改动的原始 Resolver 计算。
    let recovered = recover_style_hidden_paragraphs(session);
    // Table cells use the same paragraph resolution. Table-style layers are not
    // applied; the table projection rejects tables that declare a style.
    let mut blocks: Vec<&Block> = Vec::new();
    for original in &document.main {
        let block = recovered.get(&original.node()).unwrap_or(original);
        if let Block::Text(para) = block
            && !matches!(original, Block::Text(_))
        {
            effective.recovered_blocks.insert(para.node.0, block.to_json(&cx));
        }
        blocks.push(block);
    }
    while let Some(block) = blocks.pop() {
        let para = match block {
            Block::Text(para) => para,
            Block::Table(table) => {
                blocks.extend(table.rows.iter().flat_map(|row| &row.cells).flat_map(|cell| &cell.blocks));
                continue;
            }
            _ => continue,
        };
        let style = para.style_id.as_deref().or(default_style);
        let list = match &para.kind {
            TextKind::ListItem { list } => Some(list),
            _ => None,
        };
        effective.paras.insert(para.node.0, resolver.para(style, list, &para.props).props.to_json(&cx));
        let chain = style.map(|id| resolver.chain(id, StyleType::Paragraph)).unwrap_or_default();
        let tabs = document.styles.as_ref().and_then(|styles| styles.doc_default_ppr())
            .into_iter()
            .chain(chain.iter().rev().filter_map(|style| style.ppr.as_ref()))
            .chain(std::iter::once(&para.props))
            .filter_map(|props| props.tabs.as_ref())
            .map(|tabs| tabs.to_json(&cx));
        effective.tabs.insert(para.node.0, merge_layout_tabs(tabs));
        let empty_props = RunProps::default();
        let mark_props = para.para_mark_props().unwrap_or(&empty_props);
        let mark = resolver.run(style, mark_props.style.as_deref(), mark_props);
        effective.marks.insert(para.node.0, layout_run_props(&resolver, &mark, &cx));
        let mut pending: Vec<&Inline> = para.inlines.iter().collect();
        while let Some(inline) = pending.pop() {
            match inline {
                Inline::Run(run) => {
                    let props = resolver.run(style, run.props.style.as_deref(), &run.props);
                    let mut json = layout_run_props(&resolver, &props, &cx);
                    if let Some(fit) = fit_text_json(session.dom(), &run.props)
                        && let Some(out) = json.as_object_mut()
                    {
                        out.insert("fitText".to_string(), fit);
                    }
                    effective.runs.insert(run.node.0, json);
                }
                Inline::Field { result, .. } => pending.extend(result),
                Inline::Atom(_) => {}
            }
        }
    }
    // 正文以表格结尾时 Word 补的那个空段（见 `LayoutDocument::implied_final_para`）：
    // 缺省段落样式、没有直接属性。
    let node = crate::bridge::IMPLIED_PARA_NODE;
    effective.paras.insert(node, resolver.para(default_style, None, &ParaProps::default()).props.to_json(&cx));
    let chain = default_style.map(|id| resolver.chain(id, StyleType::Paragraph)).unwrap_or_default();
    let tabs = document.styles.as_ref().and_then(|styles| styles.doc_default_ppr())
        .into_iter()
        .chain(chain.iter().rev().filter_map(|style| style.ppr.as_ref()))
        .filter_map(|props| props.tabs.as_ref())
        .map(|tabs| tabs.to_json(&cx));
    effective.tabs.insert(node, merge_layout_tabs(tabs));
    let mark = resolver.run(default_style, None, &RunProps::default());
    effective.marks.insert(node, layout_run_props(&resolver, &mark, &cx));
    effective
}

fn recover_style_hidden_paragraphs(session: &EditSession) -> BTreeMap<rsword::xml::NodeId, Block> {
    let document = session.document();
    let candidates = document.main.iter().filter_map(|block| match block {
        Block::Protected(block) if block.kind == ProtectedKind::Invisible => Some(block.node),
        _ => None,
    }).collect::<std::collections::BTreeSet<_>>();
    if candidates.is_empty() {
        return BTreeMap::new();
    }
    let mut styles = document.styles.clone();
    if let Some(styles) = &mut styles {
        for style in &mut styles.styles {
            if let Some(props) = &mut style.rpr {
                props.vanish = None;
            }
        }
    }
    let (blocks, _) = Document::build_main(
        session.dom(), styles.as_ref(), &session.package().part(document.main_part).rels,
    );
    blocks.into_iter()
        .filter(|block| matches!(block, Block::Text(_)) && candidates.contains(&block.node()))
        .map(|block| (block.node(), block))
        .collect()
}

/// run 自己写的 `w:fitText` → `{"val": twips, "id": n}`（`id` 可缺）。
///
/// 钉住的解析器（399e36a）只建模了单元格的 `w:tcFitText`，run 级的 `w:fitText` 留在
/// `RunProps::raw_unmodeled` 里、声明值 JSON 也不带，所以照 [`document_compatibility`] 的办法
/// 直接读原 DOM。只读直接属性：字符样式里的 `w:fitText` 看不到（Word 未测，夹具里也没有）。
/// `w:val` 只认整数 twips；带单位的写法（`ST_UniversalMeasure`）不认，按没写处理。
fn fit_text_json(dom: &Dom, props: &RunProps) -> Option<Value> {
    let &node = props.raw_unmodeled.iter()
        .find(|&&node| dom.name(node) == Some(QName::w(LocalName::FitText)))?;
    let number = |name| dom.attr_value(node, QName::w(name)).and_then(|v| v.trim().parse::<i64>().ok());
    let mut out = serde_json::Map::new();
    out.insert("val".to_string(), number(LocalName::Val)?.into());
    if let Some(id) = number(LocalName::Id) {
        out.insert("id".to_string(), id.into());
    }
    Some(Value::Object(out))
}

/// 保持 run JSON 字段形状，主题引用另解析为布局实际读取的四个字体槽。
fn layout_run_props(resolver: &Resolver<'_>, props: &EffectiveRunProps, cx: &ProjCx<'_>) -> Value {
    let mut json = props.props.to_json(cx);
    let fonts = resolver.fonts(&props.props);
    let default_fonts = resolver.doc_default_fonts();
    if let Some(out) = json.as_object_mut() {
        let slots = out.entry("fonts").or_insert_with(|| serde_json::json!({}));
        if let Some(slots) = slots.as_object_mut() {
            for (key, value) in [
                ("ascii", fonts.ascii),
                ("hAnsi", fonts.h_ansi),
                ("eastAsia", fonts.east_asia.or(default_fonts.east_asia)),
                ("cs", fonts.cs),
            ] {
                if let Some(value) = value {
                    slots.insert(key.to_string(), Value::String(value));
                }
            }
        }
    }
    json
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
    use crate::bridge::EffectiveProperties;
    use crate::DocumentCompatibility;

    #[test]
    fn a_failed_merge_falls_back_to_the_unmerged_json() {
        let json = serde_json::json!({"mainPart": 3, "main": []});
        let compatibility = DocumentCompatibility {
            split_page_break_and_para_mark: Some(true),
            no_column_balance: Some(false),
        };
        let LoadedDocument { json: got, merged_run_props, merge_error, compatibility: got_compatibility, .. } =
            best_effort(json.clone(), EffectiveProperties::default(), compatibility, |_| Err("replacePartXml 失败".into()));
        assert_eq!(got, json, "合并失败时交回原样的 JSON");
        assert_eq!(merged_run_props, 0);
        assert_eq!(merge_error.as_deref(), Some("replacePartXml 失败"));
        assert_eq!(got_compatibility, compatibility);

        // 对照：没有可合并的，原样且不报错；合并成功，换成合并后的。
        let none = best_effort(json.clone(), EffectiveProperties::default(), compatibility, |_| Ok(None));
        assert_eq!((none.json, none.merged_run_props, none.merge_error), (json.clone(), 0, None));
        assert_eq!(none.compatibility, compatibility);
        let merged = serde_json::json!({"mainPart": 3, "main": [1]});
        let some = best_effort(json, EffectiveProperties::default(), compatibility, |_| Ok(Some((merged.clone(), EffectiveProperties::default(), 2))));
        assert_eq!((some.json, some.merged_run_props, some.merge_error), (merged, 2, None));
        assert_eq!(some.compatibility, compatibility);
    }
}
