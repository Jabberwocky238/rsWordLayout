//! 浏览器侧的字体注册与选择（wasm32）。
//!
//! 把三个东西绑在同一套 face 标识上，缺一不可：
//!
//! - `docx_layout::fontenv` —— 按码位查覆盖（选 face 走本模块自己的槽规则，不走
//!   `FontEnvironment::select`，所以不产生它的 `FONT_MISSING`；缺字由会话的诊断数）；
//! - `RustybuzzShaper`      —— 整形，产出 glyph id；
//! - `SkrifaRasterizer`     —— 按 glyph id 栅格化。
//!
//! 标识统一用 `fontenv` 的内容哈希与 TTC 序号，不靠文件名，
//! 同一份字体在三处必然对得上。
//!
//! 字体不编进 wasm，由 JS 运行时 fetch 后交进来（见 `web/public/fonts/README.md`）。

use std::collections::{BTreeMap, BTreeSet};

use super::{FontSlots, FontSpec, RustybuzzShaper, SkrifaRasterizer, SlotKind};
use crate::layout::ShapedRun;
use crate::layout::TextShaper;
use crate::layout::{OBJECT_PLACEHOLDER, TWIPS_PER_POINT, Twips};
use docx_layout::fontenv::{FaceId, FontEnvironment, FontEnvironmentBuilder, normalize_family};
use skrifa::{FontRef, MetadataProvider, string::StringId};

/// 已注册的字体集合。
///
/// # 两级字体：正文字体与回退链
///
/// 为一个字符选 face 的顺序（[`FontRegistry::select_face_for`]）：
///
/// 1. 槽里写的族名 / face 名，且该 face 盖得住这个码位——**Word 的槽规则**；
/// 2. 否则，**eastAsia 槽**的字符按**声明顺序**走回退链（[`FontRegistry::add_fallback`]）；
/// 3. 否则在**正文字体**（[`FontRegistry::add`]）里找第一个盖得住的（fontenv 的内容哈希序，与原来相同）；
/// 4. 都盖不住：CJK 字符给名义 1 em 的 `.notdef`（见 [`FontRegistry::shape_text`]），
///    其余字符照旧跳过。
///
/// ascii / hAnsi / cs 槽的字符**不查回退链**，顺序与没有回退链时逐位相同。
/// 控制字符、格式字符（Cf）与默认可忽略码位（[`is_invisible`]）只走第 1、3 步：
/// 不查回退链，也不给名义字形——回退字体给它们配的字形会带进自己的纵向量，
/// 把一行撑高（Droid 映射了 U+202A–U+202D、U+0000 与 U+3164，后两个还是 1 em）。
///
/// 回退链对应手机上的真实情形（**实测的是行数**）：`fixtures/*.docx` 的 CJK 夹具
/// （`han22`、`mix-cjk`、`kinsoku*`、`han-24`、`han-paper`）eastAsia 槽写 SimSun，
/// 手机上没有 SimSun，Word 照样把 30 个「汉」排成 22 + 8（`han22` 的窄路径 jsonl）——
/// 它换了一个 CJK 字体。换的是哪一个**未测**：手机进程映射里能看到 Noto Sans CJK、
/// Noto Serif CJK、MiSans VF（word_analyse `reports/maps-word-13336.txt`），
/// 但那份快照不全（连 Calibri 都不在里面），不能当 Word 排版用的字体清单。
///
/// 回退链单列一级而不是混进正文字体，是因为 fontenv 的「任意覆盖」一步按**内容哈希**排序，
/// 与装字体的先后无关：回退字体一旦混进去，哈希碰巧排前的 Noto 会连拉丁字母也一并抢走
/// （实测：`breakme` 没有 rFonts，以 `--font calibri --font Noto` 排每行 45 字，
/// Word 是 51——`reports/diff/breakme.word.narrow.jsonl`）。
///
/// 第 2 步排在第 3 步前面（**假定**，按槽规则推的，没有 Word 的数）：eastAsia 槽的字体装不上时，
/// Word 用它的替代字体排这个槽的字符，哪怕西文正文字体也画得出——`w:hint="eastAsia"` 下的
/// `“`（U+201C）就该用 CJK 字体的宽度，不是 Calibri 的。能分辨的夹具是 hint=eastAsia 的 `“`：
/// 它先检验的是这个顺序，其次才是替代字体是谁（Noto SC 1000、Noto JP 474、MiSans 378 per 1000 em）。
/// 「字体装不上」这里按「槽里的族名画不出这个字符」算，不另查族名装没装——也是假定。
pub struct FontRegistry {
    builder: FontEnvironmentBuilder,
    env: Option<FontEnvironment>,
    shaper: RustybuzzShaper,
    raster: SkrifaRasterizer,
    /// 字体表中实际存在的额外名称 → face；不从后缀猜测字体族。
    names: BTreeMap<String, BTreeSet<FaceId>>,
    /// face 标识（内容哈希与 TTC 序号）→ shaper 内的下标。
    /// `ShapedRun::face_index` 是下标，paint 层要靠它查回 `FaceId`。
    index_of: std::collections::HashMap<String, usize>,
    /// face 标识 → `FaceId`，给 [`FontRegistry::face_covers`] 直接查 cmap，不必逐个 face 比标识。
    id_of: std::collections::HashMap<String, FaceId>,
    /// 回退链，按声明顺序。只给 eastAsia 槽、且槽里的字体画不出的字符查。
    fallback: Vec<String>,
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FontRegistry {
    pub fn new() -> FontRegistry {
        FontRegistry {
            builder: FontEnvironmentBuilder::new(),
            env: None,
            shaper: RustybuzzShaper::new(),
            raster: SkrifaRasterizer::new(),
            names: BTreeMap::new(),
            index_of: std::collections::HashMap::new(),
            id_of: std::collections::HashMap::new(),
            fallback: Vec::new(),
        }
    }

    /// 注册一份正文字体。返回它的 face 标识（内容哈希；非零 TTC 序号加 `:index`）。
    ///
    /// 同一份字节交三处：fontenv 用来查覆盖，shaper 用来整形，rasterizer 用来栅格化。
    /// 已作为回退字体注册过的 face 再经这里注册，就升为正文字体。
    pub fn add(&mut self, bytes: Vec<u8>, index: u32) -> Result<String, &'static str> {
        let face = self.register(bytes, index)?;
        self.fallback.retain(|f| f != &face);
        Ok(face)
    }

    /// 把一份字体接到回退链末尾。
    ///
    /// 与 [`FontRegistry::add`] 的差别只在选 face 时的位次：回退字体**不参与**
    /// 正文字体之间的「任意覆盖」查找，只给 eastAsia 槽里画不出的字符按注册的先后查
    /// （见 [`FontRegistry`] 的顺序说明）。文档按族名点到它时（如 `w:eastAsia="Droid Sans Fallback"`），
    /// 它照常被槽规则选中——那是文档自己要的字体，不是回退。
    ///
    /// 已是正文字体的 face 不降级。
    pub fn add_fallback(&mut self, bytes: Vec<u8>, index: u32) -> Result<String, &'static str> {
        let known = self.index_of.len();
        let face = self.register(bytes, index)?;
        if self.index_of.len() > known {
            self.fallback.push(face.clone());
        }
        Ok(face)
    }

    /// 回退链上的 face，按查找顺序。
    pub fn fallback_faces(&self) -> &[String] {
        &self.fallback
    }

    fn register(&mut self, bytes: Vec<u8>, index: u32) -> Result<String, &'static str> {
        let id = self.builder.add(bytes.clone(), index)?;
        let face = face_key(&id);
        if self.index_of.contains_key(&face) {
            return Ok(face);
        }
        let font = FontRef::from_index(&bytes, index).map_err(|_| "FONT_INVALID")?;
        for name in [
            StringId::FULL_NAME,
            StringId::POSTSCRIPT_NAME,
            StringId::COMPATIBLE_FULL_NAME,
            StringId::WWS_FAMILY_NAME,
        ]
        .into_iter()
        .flat_map(|id| font.localized_strings(id))
        .map(|s| normalize_family(&s.to_string()))
        .filter(|s| !s.is_empty())
        {
            self.names.entry(name).or_default().insert(id.clone());
        }
        let at = self.shaper.add_face(face.clone(), bytes.clone(), index);
        self.index_of.insert(face.clone(), at);
        self.id_of.insert(face.clone(), id);
        self.raster.add_face(face.clone(), bytes, index);
        // 每次新增都要重新冻结：env 是不可变快照。
        self.env = Some(self.builder.freeze());
        Ok(face)
    }

    pub fn is_empty(&self) -> bool {
        self.env.is_none()
    }

    /// 字体环境指纹：字体集变了它就变，可用来判断布局是否需要重算。
    pub fn fingerprint(&self) -> Option<&str> {
        self.env.as_ref().map(|e| e.fingerprint())
    }

    /// 按请求的字体族与码位选一个 face。
    ///
    /// 返回 `None` 表示所有已注册字体（含回退链）都覆盖不了这个码位——
    /// 调用方不能拿一个画不出它的 face 去整形；要名义度量走
    /// [`FontRegistry::shape_text`]，它对 CJK 字符给 1 em 的 `.notdef`。
    /// 按 **Word 的槽规则** 为一个字符选 face。
    ///
    /// 顺序不能反（完整的四步见 [`FontRegistry`]）：
    ///   1. 按字符所属区查 `w:rFonts` 的对应槽（ascii / hAnsi / eastAsia / cs）——
    ///      这是 Word 的真实规则，不是 fallback；
    ///   2. 槽里的字体装不了或画不出该字符时：eastAsia 槽的字符先按声明顺序走回退链；
    ///   3. 再在正文字体里找第一个能覆盖的。
    ///
    /// 只做第 3 步会选错字体：中文该走 eastAsia 槽指定的宋体，
    /// 而 fallback 会挑第一个能覆盖它的字体，两者常常不同。
    pub fn select_face_for(&self, font: &FontSpec, ch: char) -> Option<String> {
        let east_asia = font.slots.slot_for(ch) == SlotKind::EastAsia;
        self.select(font.family_for(ch), ch, font.bold, font.italic, east_asia)
    }

    /// 只给族名时选 face。没有 `FontSpec` 就没有 `w:hint`：
    /// 字符归不归 eastAsia 槽（查不查回退链）按不带 hint 的分区算。
    pub fn select_face(&self, family: &str, ch: char, bold: bool, italic: bool) -> Option<String> {
        let east_asia = FontSlots::default().slot_for(ch) == SlotKind::EastAsia;
        self.select(family, ch, bold, italic, east_asia)
    }

    fn select(
        &self,
        family: &str,
        ch: char,
        bold: bool,
        italic: bool,
        east_asia: bool,
    ) -> Option<String> {
        let env = self.env.as_ref()?;
        if let Some(face) = self.slot_face(family, ch, bold, italic) {
            return Some(face);
        }
        // 回退链只接 eastAsia 槽的字符，且不接不成字形的字符（见 `is_invisible`）。
        // 判据与 [`FontRegistry::fallback_candidates`] 共用，两边不能各写一份。
        if east_asia
            && !is_invisible(ch)
            && let Some(face) = self.fallback.iter().find(|f| self.face_covers(f, ch))
        {
            return Some(face.clone());
        }
        // 与 `env.select` 的最后一步同序（fontenv 的 face 表按内容哈希排），
        // 只是把回退链排除在外：没有回退字体时，结果与原来逐位相同。
        // 族名那一步上面已经做过；本仓库不设 fontenv 别名，所以这里不必再查。
        env.faces()
            .filter(|f| !self.is_fallback(f.id()))
            .find(|f| env.covers(f.id(), ch))
            .map(|f| face_key(f.id()))
    }

    /// 第 1 步：槽里写的族名 / face 名，且该 face 盖得住 `ch`。落空就是「槽里的字体画不出」。
    fn slot_face(&self, family: &str, ch: char, bold: bool, italic: bool) -> Option<String> {
        let env = self.env.as_ref()?;
        let weight = if bold { 700 } else { 400 };
        // 族名优先，避免常规 face 的 full name 恰好等于族名时盖住粗体/斜体。
        if let Some(face) = env
            .candidates(family, weight, italic)
            .into_iter()
            .find(|f| env.covers(f.id(), ch))
        {
            return Some(face_key(face.id()));
        }
        let ids = self.names.get(&normalize_family(family))?;
        let mut candidates: Vec<_> = env.faces().filter(|f| ids.contains(f.id())).collect();
        candidates.sort_by_key(|f| (f.italic() != italic, f.weight().abs_diff(weight), f.id()));
        candidates
            .into_iter()
            .find(|f| env.covers(f.id(), ch))
            .map(|f| face_key(f.id()))
    }

    fn is_fallback(&self, id: &FaceId) -> bool {
        self.fallback
            .iter()
            .any(|f| self.id_of.get(f).is_some_and(|known| known == id))
    }

    /// 度量与整形实际用哪个 face：选得中就是它，选不中的 CJK 字符落到名义 face。
    ///
    /// 第二项为 `true` 表示是名义的——该字符没有任何字体画得出，
    /// 给的是名义 face 的 `.notdef` 与 1 em 推进量（见 [`FontRegistry::nominal_face`]）。
    /// 非 CJK 字符与不成字形的字符选不中时返回 `None`，与原来一样跳过
    /// （非 CJK 字符在手机上缺字时画成什么、多宽**未测**）。
    pub fn face_for_char(&self, font: &FontSpec, ch: char) -> Option<(String, bool)> {
        if let Some(face) = self.select_face_for(font, ch) {
            return Some((face, false));
        }
        takes_nominal(ch)
            .then(|| self.nominal_face(font))
            .flatten()
            .map(|face| (face, true))
    }

    /// 名义 `.notdef` 借用的 face：**这个 run 自己的西文 face**——ascii 槽
    /// （没写就 hAnsi 槽，再没写就 `family`）画 `A` 用的那个。它也画不出 `A` 时
    /// （只装了没有拉丁字母的字体），取注册顺序里第一个正文字体，再没有就取第一个 face。
    /// 借哪个 face 是**假定**（Word 从不画这个 `.notdef`：手机上总有 CJK 字体），
    /// 取 run 自己的西文 face，是为了让纵向量跟着 run 走，而不是跟着装字体的先后走。
    ///
    /// 只借它的 `.notdef` 字形与纵向量；推进量不取 `.notdef` 的，统一 1 em——
    /// 手机上 SimSun 缺失时 Word 的汉字与 1 em 相容：12pt 下 `han22` 22 字一行
    /// （`reports/diff/han22.word.narrow.jsonl`）、`han-paper` 43 字一行
    /// （`reports/rsword-diff/han-size.md`），把字宽夹在 237.9–242.2 twips（1 em = 240，±0.9%）；
    /// 24pt 的 `han-24` 11 字一行（同一份报告）只夹到 444.1–484.5 twips（−7.5% / +0.9%）。
    /// 实测的是行数，不是字宽。Calibri 的 `.notdef` 只有 0.507 em，照它排是 43 字一行。
    ///
    /// **纵向量是占位**：借的是西文 face 的升部、降部，没有 Word 的数（jsonl 的行高为空）。
    /// Mac 上量到的 N7 规则（`docs/PREREG-2026-09-18-cjk-plain.md` §5.1：OS/2 声明了东亚代码页的
    /// 字体，行距 ×1.3）在 Android 上成不成立**未知**；若成立，名义路径的行距比任何真 CJK 回退字体
    /// 都短约 30%（Calibri 没有东亚代码页）。
    pub fn nominal_face(&self, font: &FontSpec) -> Option<String> {
        let latin = font
            .slots
            .ascii
            .as_deref()
            .or(font.slots.h_ansi.as_deref())
            .unwrap_or(&font.family);
        self.select(latin, 'A', font.bold, font.italic, false)
            .or_else(|| {
                let ids = self.shaper.face_ids();
                ids.iter()
                    .find(|f| !self.fallback.contains(f))
                    .or(ids.first())
                    .cloned()
            })
    }

    /// 一段文字里**正文字体与回退链都盖不住**的 eastAsia 槽字符，按出现顺序。
    ///
    /// 只数回退链管的那些字符（eastAsia 槽、成字形的）：控制字符（制表符、换行）、
    /// 对象占位符 U+FFFC、格式字符与默认可忽略码位不成字形，不算缺字；
    /// 其他槽的缺字（阿拉伯文、泰文……）回退链不接，也不算。
    /// `layout-trace` 的默认回退链据此决定要不要装（显式的回退链按
    /// [`FontRegistry::fallback_candidates`]，理由见那里）。
    pub fn uncovered_chars<'t>(
        &'t self,
        text: &'t str,
        font: &'t FontSpec,
    ) -> impl Iterator<Item = char> + 't {
        text.chars().filter(move |&ch| {
            font.slots.slot_for(ch) == SlotKind::EastAsia
                && !is_invisible(ch)
                && self.select_face_for(font, ch).is_none()
        })
    }

    /// 一段文字里**会查回退链**的字符，按出现顺序：eastAsia 槽里成字形、槽里的族名 /
    /// face 名画不出的（第 1 步落空）——**不管别的正文字体画不画得出**。
    ///
    /// 与 [`FontRegistry::select_face_for`] 的第 2 步是同一个判据（共用 `slot_face` 与
    /// [`is_invisible`]）：回退链上的 face 盖得住其中哪个字符，注册之后那个字符就改用它。
    /// [`FontRegistry::uncovered_chars`] 是它的子集（哪个字体都画不出的那些）。
    /// 按子集决定装不装回退链，槽规则要交给回退链的字符（如 `w:hint="eastAsia"` 下
    /// 西文字体也画得出的 `“`）会因为文档别处恰好有、恰好没有一个缺字而换字体；
    /// `layout-trace` 的显式回退链因此按这个判据装。
    pub fn fallback_candidates<'t>(
        &'t self,
        text: &'t str,
        font: &'t FontSpec,
    ) -> impl Iterator<Item = char> + 't {
        text.chars().filter(move |&ch| {
            font.slots.slot_for(ch) == SlotKind::EastAsia
                && !is_invisible(ch)
                && self
                    .slot_face(font.family_for(ch), ch, font.bold, font.italic)
                    .is_none()
        })
    }

    /// 实际装进来的族名，排序去重。
    ///
    /// 用途只有一个，但很要紧：**核字体有没有被替换**。量具方法 §6.2 实测，
    /// 度量兼容克隆（Liberation Serif ↔ Times New Roman 等）替换后几何一字不差，
    /// 2618 条记录 max |Δ| = 0.000000pt——**任何几何自检都发现不了，只有字体名能**。
    ///
    /// 注意不能拿 [`FontRegistry::select_face`] 代替：它带 fallback，
    /// 族根本没装也会返回一个能盖住该码位的 face，于是核查永远通过。
    pub fn families(&self) -> Vec<String> {
        let Some(env) = self.env.as_ref() else {
            return Vec::new();
        };
        let mut out: Vec<String> = env
            .faces()
            .flat_map(|f| f.families().iter().cloned())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// 某个族或完整 face 名是否真的装进来了（按 fontenv 的名称归一化比较）。
    /// 只核对字体表中的名称；码位 fallback 不能让缺失字体通过核查。
    pub fn covers_family(&self, family: &str) -> bool {
        let want = normalize_family(family);
        self.names.contains_key(&want)
            || self
                .env
                .as_ref()
                .is_some_and(|env| env.faces().any(|f| f.families().contains(&want)))
    }

    /// 按 face 标识取字体字节与 TTC 序号。
    ///
    /// 纵向量（`hhea` / `OS/2`）要直接解析字体表，而整形器只给推进量，
    /// 所以这里把字节露出来。返回 `None` 表示该 face 没注册。
    pub fn face_data(&self, face: &str) -> Option<(&[u8], u32)> {
        let env = self.env.as_ref()?;
        env.faces()
            .find(|f| face_key(f.id()) == face)
            .and_then(|f| env.data(f.id()).map(|b| (b, f.id().index())))
    }

    /// 全部 face 标识，顺序与 `ShapedRun::face_index` 一致。
    pub fn face_ids(&self) -> Vec<String> {
        self.shaper.face_ids()
    }

    pub fn rasterizer_mut(&mut self) -> &mut SkrifaRasterizer {
        &mut self.raster
    }

    /// 整形一段文字。
    ///
    /// 按码位切段：同一段文字里中英文混排会选到不同 face，必须分段整形，
    /// 否则用一个 face 去 shape 它覆盖不了的字符只会得到 .notdef。
    ///
    /// `w:caps` / `w:smallCaps` 在这里生效（[`super::caps::display_chars_with`]）：
    /// 真度量的 `measure` 与绘制层都调这一个函数，所以量到的宽度与画出的字形
    /// 是同一份整形结果。小型大写一段里有两个字号，**字号变了也要切段**。
    /// 字形的 `source` 仍是原文的 UTF-16 区间——变换改的是显示字符，不是源偏移；
    /// 选 face 也按源字符（[`FontRegistry::face_for_display`]）。哪个 face 都画不出的大写
    /// 退回源字符（假定，见 `display_chars_with`），不让它因为没有 face 被整段丢掉。
    ///
    /// 没有任何字体盖得住的 CJK 字符（[`FontRegistry::face_for_char`] 标为名义的）
    /// 不整形，直接给名义 face 的 `.notdef`（glyph 0）、推进量 1 em（[`nominal_glyphs`]）：
    /// 否则它推进量为零，一整段汉字会挤进一行（`han22` 只装 Calibri 时 30 字 1 行，
    /// Word 是 22 + 8）。轨迹里这些字形 `glyphId` 为 0，顶层 `notdefGlyphs` 计数。
    /// 其余盖不住的字符仍然跳过，但各自的 UTF-16 源位置照占。
    ///
    /// 绘制层也走这里，所以没装 CJK 字体时画出来的是 `.notdef` 方框（WebGL 的绘制同样如此），
    /// 原来是什么都不画。这是有意的：占了 1 em 的字不画，比画一个方框更难发现缺字。
    pub fn shape_text(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        if self.env.is_none() || text.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        // （显示字符，源 UTF-16 起点）；段终点是最后一个显示字符的源终点。
        let mut run: Vec<(char, u32)> = Vec::new();
        // （（face，是否名义），字号）。
        type Key = (Option<(String, bool)>, u64);
        let mut run_key: Option<Key> = None;
        let mut run_end = 0;
        let append =
            |out: &mut Vec<ShapedRun>, key: &Option<Key>, run: &[(char, u32)], end: u32| {
                let Some((Some((face, nominal)), size)) = key else {
                    return;
                };
                let Some(&i) = self.index_of.get(face) else {
                    return;
                };
                if *nominal {
                    out.extend(nominal_glyphs(i, run, end, *size));
                    return;
                }
                out.extend(self.shaper.shape_clusters_with_face_centipoints(
                    i,
                    run,
                    end,
                    *size,
                    font.kerning,
                ));
            };

        // 名义 face 只取决于 run 的字体，一段里至多算一次。
        let nominal = std::cell::OnceCell::new();
        let faces = |source_ch, ch| self.face_for_display(font, source_ch, ch);
        for (d, face) in super::caps::display_chars_with(text, font, faces) {
            // 与 `face_for_char` 同一口径（纵向量按它取）：按源字符定要不要名义字形。
            let face = match face {
                Some(face) => Some((face, false)),
                None if takes_nominal(d.source_ch) => nominal
                    .get_or_init(|| self.nominal_face(font))
                    .clone()
                    .map(|face| (face, true)),
                None => None,
            };
            // 按槽选，而不是整段用同一个 family。
            let key = (face, d.size_centipoints);
            if run_key.as_ref() != Some(&key) {
                if !run.is_empty() {
                    append(&mut out, &run_key, &run, run_end);
                }
                run.clear();
            }
            run_key = Some(key);
            run.push((d.ch, d.source.0));
            run_end = d.source.1;
        }
        if !run.is_empty() {
            append(&mut out, &run_key, &run, run_end);
        }
        out
    }

    /// 一个显示字符用哪个 face：**槽与 face 按源字符选**，那个 face 画不出显示字符
    /// （缺大写字形）时才按显示字符查覆盖，槽仍是源字符的槽。
    ///
    /// 按源字符选，是因为 `FontSpec::slot_for` 按码位分区，而大小写映射会跨区：
    /// `ı` → `I`、`ſ` → `S` 从 hAnsi 落进 ascii，ascii 与 hAnsi 槽是两个字体时就会换字体。
    /// 纵向量（`RealMetrics::vertical_raw`）也按源字符选 face，两边因此一致。
    /// Word 在 caps 下按存储的字符还是显示的字符套槽规则**未测**——这一条是假定。
    ///
    /// 不做变换的字符（`Caps::None` 下全部）与原来逐位相同：直接 `select_face_for`。
    /// 返回 `None` 是哪个 face 都画不出 `ch`；变换过的字符这时由
    /// [`super::caps::display_chars_with`] 退回源字符再问一次。
    fn face_for_display(&self, font: &FontSpec, source_ch: char, ch: char) -> Option<String> {
        let face = self.select_face_for(font, source_ch);
        if ch == source_ch || face.as_deref().is_some_and(|f| self.face_covers(f, ch)) {
            return face;
        }
        // 槽（含查不查回退链）仍按源字符。
        let east_asia = font.slots.slot_for(source_ch) == SlotKind::EastAsia;
        self.select(
            font.family_for(source_ch),
            ch,
            font.bold,
            font.italic,
            east_asia,
        )
    }

    /// 一个源字符在 [`FontRegistry::shape_text`] 里的下场，给会话的缺字诊断数。
    ///
    /// 与 `shape_text` 同一口径：选中 face 就是 [`CharCoverage::Face`]；选不中的 CJK
    /// 画名义 `.notdef`（[`CharCoverage::Nominal`]）；选不中的其他成字形字符被跳过、
    /// 不占宽度（[`CharCoverage::Dropped`]）；不成字形的字符（[`is_invisible`]）不算缺字。
    /// 按**源字符**判，`w:caps` 下显示字符换了 face 的情形（`face_for_display`）不在内。
    pub(crate) fn char_coverage(&self, font: &FontSpec, ch: char) -> CharCoverage {
        if is_invisible(ch) {
            return CharCoverage::Invisible;
        }
        match self.face_for_char(font, ch) {
            Some((face, false)) => CharCoverage::Face(face),
            Some((_, true)) => CharCoverage::Nominal,
            None => CharCoverage::Dropped,
        }
    }

    /// `face` 属于 `family`（按族名或字体表里的完整名 / PostScript 名，同 [`FontRegistry::covers_family`]）
    /// 时，给它的（字重，是否斜体）；不属于或没注册返回 `None`。
    pub(crate) fn family_face_style(&self, face: &str, family: &str) -> Option<(u16, bool)> {
        let id = self.id_of.get(face)?;
        let want = normalize_family(family);
        let info = self.env.as_ref()?.faces().find(|f| f.id() == id)?;
        let named = self.names.get(&want).is_some_and(|ids| ids.contains(id));
        (named || info.families().contains(&want)).then(|| (info.weight(), info.italic()))
    }

    /// 按 face 标识查它的 cmap 是否覆盖 `ch`。
    fn face_covers(&self, face: &str, ch: char) -> bool {
        match (self.env.as_ref(), self.id_of.get(face)) {
            (Some(env), Some(id)) => env.covers(id, ch),
            _ => false,
        }
    }
}

/// [`FontRegistry::char_coverage`] 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CharCoverage {
    /// 有 face 画它（face 标识）。
    Face(String),
    /// 谁都画不出的 CJK：名义 1 em 的 `.notdef`。
    Nominal,
    /// 谁都画不出、也没有名义宽度：跳过，不占宽度。
    Dropped,
    /// 不成字形（控制、格式、默认可忽略码位）：本来就不画，不算缺字。
    Invisible,
}

/// 不成字形的字符：控制字符（Cc）、对象占位符 U+FFFC、格式字符（Cf）、默认可忽略码位
/// （Default_Ignorable_Code_Point），外加行 / 段分隔符 U+2028 / U+2029。
///
/// 它们不查回退链、不给名义字形，也不算缺字——只有槽里的字体与正文字体画它们时
/// 才有字形，与没有回退链时逐位相同。回退字体给它们配的字形会带进那个字体的纵向量：
/// Droid 映射了 U+202A–U+202D，一段 Calibri 西文里夹两个双向格式符，行距就涨 3.5–3.7pt
/// （G4 审查的 `latin-cf.docx`）；MiSans 映射了 U+000A，`breakme` 第 576 位的换行符
/// 因此多出 3.48pt 的宽度（同一审查）。表是按 Unicode 16 的 Cf 与 Default_Ignorable_Code_Point
/// 手抄的（Cf 逐段核过）。
/// Word 缺字时怎么画它们**未测**。
fn is_invisible(ch: char) -> bool {
    ch.is_control()
        || ch == OBJECT_PLACEHOLDER
        || matches!(ch as u32,
            0x00AD                // 软连字符
            | 0x034F              // 组合字素连接符
            | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x0890..=0x0891 | 0x08E2
            | 0x115F..=0x1160     // 谚文初声、中声填充符
            | 0x17B4..=0x17B5 | 0x180B..=0x180F
            | 0x200B..=0x200F     // 零宽空格、零宽（不）连接符、方向标记
            | 0x2028..=0x2029     // 行 / 段分隔符
            | 0x202A..=0x202E     // 双向嵌入与覆盖
            | 0x2060..=0x206F     // 词连接符、不可见运算符、双向隔离
            | 0x3164 | 0xFFA0     // 谚文填充符
            | 0xFE00..=0xFE0F     // 变体选择符
            | 0xFEFF | 0xFFF0..=0xFFFB
            | 0x110BD | 0x110CD | 0x13430..=0x1343F | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF   // 标签字符、变体选择符补充
        )
}

/// 选不中 face 时给不给名义 1 em：只给 CJK（[`super::linebreak::is_cjk`]）里成字形的字符。
///
/// 依据：12pt 与 24pt 的汉字行数与 1 em 相容（见 [`FontRegistry::nominal_face`]），
/// 仓库的桩度量也给 `is_cjk` 1000‰。**谚文**也在 `is_cjk` 里、也给 1 em，这是已知的偏差：
/// 手机上的 Noto Sans CJK 谚文是 920/1000 em，而默认的回退字体 Droid Sans Fallback
/// 11172 个谚文音节只有 3 个、也没有 `〈〉`（U+3008 / U+3009），所以默认配置下这些字
/// 落到名义宽度上，谚文比 Noto 宽 8.7%。组合符号（Mn）见 [`nominal_glyphs`]。
fn takes_nominal(ch: char) -> bool {
    super::linebreak::is_cjk(ch) && !is_invisible(ch)
}

/// `is_cjk` 里的组合符号（Mn）：U+302A–U+302D 的声调符号、U+3099 / U+309A 的浊音与半浊音符。
/// 名义字形给它们零宽——Noto Sans CJK 里它们也是零宽（实测字体表，不是 Word 的数）。
fn is_cjk_combining_mark(ch: char) -> bool {
    matches!(ch as u32, 0x302A..=0x302D | 0x3099..=0x309A)
}

/// 名义字形：每个字符一个 `.notdef`（glyph 0），推进量 1 em，组合符号（Mn）为零。
///
/// `chars` 与 [`RustybuzzShaper::shape_clusters_with_face_centipoints`] 同形：
/// （显示字符，源 UTF-16 起点），`end` 是最后一个的源终点。每个字形的源区间是它自己的字符
/// （代理对占两个单位）；组合符号并进前一个字符的 cluster，与整形器按字素合并同一口径——
/// 字距按 cluster 加，紧急断行也不会把它甩到下一行行首。
///
/// 取整手法与 [`RustybuzzShaper`] 相同：精确累计值留在 f64 里，
/// 单个推进量取相邻取整位置之差，前缀和不漂。
fn nominal_glyphs(
    face_index: usize,
    chars: &[(char, u32)],
    end: u32,
    size_centipoints: u64,
) -> Vec<ShapedRun> {
    let em_pt = size_centipoints as f64 / 100.0;
    let mut out: Vec<ShapedRun> = Vec::with_capacity(chars.len());
    let mut acc = 0.0f64;
    let mut acc_twips: Twips = 0;
    // 当前 cluster 第一个字形的下标。
    let mut cluster = 0;
    for (i, &(ch, start)) in chars.iter().enumerate() {
        let stop = chars.get(i + 1).map_or(end, |&(_, next)| next);
        let mark = is_cjk_combining_mark(ch) && !out.is_empty();
        let advance_pt = if is_cjk_combining_mark(ch) {
            0.0
        } else {
            em_pt
        };
        acc += advance_pt * f64::from(TWIPS_PER_POINT);
        let next = acc.round() as Twips;
        let source = if mark {
            let first = out[cluster].source.map_or(start, |(s, _)| s);
            for glyph in &mut out[cluster..] {
                glyph.source = Some((first, stop));
            }
            (first, stop)
        } else {
            cluster = out.len();
            (start, stop)
        };
        out.push(ShapedRun {
            face_index,
            glyph_id: 0,
            source: Some(source),
            x_advance: next - acc_twips,
            x_advance_pt: advance_pt,
            x_offset: 0,
            y_offset: 0,
            size_centipoints: Some(size_centipoints),
        });
        acc_twips = next;
    }
    out
}

fn face_key(id: &FaceId) -> String {
    if id.index() == 0 {
        id.sha256().to_string()
    } else {
        format!("{}:{}", id.sha256(), id.index())
    }
}

/// `Shaper` 的直通实现，便于单独使用。
impl TextShaper for FontRegistry {
    fn shape(&self, text: &str, font: &FontSpec) -> Vec<ShapedRun> {
        self.shape_text(text, font)
    }
}
