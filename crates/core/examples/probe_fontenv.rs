//! 探针：确认 docx-layout 的 fontenv 能用在本仓库里。
//!
//! 验证全 Unicode fallback 需要的三件事：
//!   1. 能读系统字体文件并识别族名/字重/斜体
//!   2. select() 按码位走完 fallback 链，并说明为什么选了它
//!   3. fingerprint() 稳定，保证同一字体集给出同一布局

use docx_layout::fontenv::{FontEnvironmentBuilder, normalize_family};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("normalize_family(\"Times New Roman\") = {:?}", normalize_family("Times New Roman"));

    let mut b = FontEnvironmentBuilder::new();
    let mut loaded = 0usize;

    // 覆盖面互补的两族：西文与 CJK。
    let candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    ];
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else { continue };
        match b.add(bytes, 0) {
            Ok(id) => {
                loaded += 1;
                println!("已载入 {path}\n   sha256={}… index={}", &id.sha256()[..16], id.index());
            }
            Err(e) => println!("载入失败 {path}: {e}"),
        }
    }
    if loaded == 0 {
        println!("没有找到可用字体，跳过后续检查");
        return Ok(());
    }

    let env = b.freeze();
    println!("\nfingerprint = {}", env.fingerprint());

    println!("\n可用 face：");
    for f in env.faces() {
        let fams: Vec<_> = f.families().iter().take(2).cloned().collect();
        println!("   weight={} italic={} families={:?}", f.weight(), f.italic(), fams);
    }

    // 关键：按码位做 fallback，并报告是原生命中还是替代。
    println!("\n按码位选择（请求 Times New Roman，看它怎么回退）：");
    let wanted = vec!["Times New Roman".to_string()];
    for ch in ['A', '中', 'あ', '가', 'م', '🙂'] {
        let sel = env.select(&wanted, 400, false, ch);
        let who = sel
            .face
            .and_then(|f| f.families().iter().next().cloned())
            .unwrap_or_else(|| "无覆盖".to_string());
        println!("   {ch:?} -> {who}  {:?}", sel.diagnostics);
    }
    Ok(())
}
