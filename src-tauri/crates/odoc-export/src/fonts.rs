//! Cross-platform system font discovery for the Chinese official-document
//! (GB/T 9704) PDF export path.
//!
//! The exporter maps semantic font names (仿宋 / 黑体 / 楷体 / 小标宋) to real
//! font files on the current machine so that Typst (or any other backend) can
//! embed actual glyphs instead of relying on browser/system fallbacks.
//!
//! Discovery is purely filesystem based: we scan the well-known system font
//! directories recursively and infer a readable family name from the file name
//! (no font binary parsing, so no extra dependencies). The inferred names are
//! normalized to space-separated words (for example
//! `NotoSerifCJK-Regular.ttc` becomes `Noto Serif CJK`) so they are usable for
//! substring matching and, in practice, close to the family names Typst sees.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// 仿宋(正文)关键字,按优先级排序。
///
/// `simfang` / `stfangsong` 是 Windows 上仿宋字体文件的常见别名,补在规范关键字之后。
const BODY_KEYWORDS: &[&str] = &[
    "仿宋",
    "fangsong",
    "fangsong_gb2312",
    "simfang",
    "stfangsong",
];

/// 黑体(一级标题)关键字,按优先级排序。
const HEI_KEYWORDS: &[&str] = &[
    "黑体",
    "simhei",
    "hei",
    "Noto Sans CJK",
    "Source Han Sans",
];

/// 楷体(二级标题)关键字,按优先级排序。
///
/// `kaiti` / `ukai` / `wenkai` 放在泛化的 `kai` 之前,避免 `kai` 子串误匹配
/// `Kaithi`(印度系 Kaithi 文字,并非中文楷体)。
const KAI_KEYWORDS: &[&str] = &["楷体", "kaiti", "ukai", "wenkai", "kai"];

/// 标题(小标宋/宋体)关键字,按优先级排序。
///
/// `xiaobiaosong` / `fzxbs` 是方正小标宋的拼音/缩写别名。
const TITLE_KEYWORDS: &[&str] = &[
    "小标宋",
    "宋体",
    "simsun",
    "Noto Serif CJK",
    "Source Han Serif",
    "xiaobiaosong",
    "fzxbs",
];

/// 宋体系回退字体(标题/正文/楷体缺失时使用)。
const SERIF_FALLBACK_KEYWORDS: &[&str] = &["Noto Serif CJK", "Source Han Serif"];

/// 黑体系回退字体(黑体缺失时使用)。
const SANS_FALLBACK_KEYWORDS: &[&str] = &["Noto Sans CJK", "Source Han Sans"];

/// 公文字体解析结果:四个语义槽位各自对应的系统字体文件路径。
///
/// 任一槽位为 [`None`] 表示当前系统上找不到该语义字体,调用方应自行决定
/// 是否继续导出或提示用户安装字体。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontSet {
    /// 标题(小标宋/宋体)。
    pub title: Option<PathBuf>,
    /// 正文(仿宋)。
    pub body: Option<PathBuf>,
    /// 黑体(一级标题)。
    pub hei: Option<PathBuf>,
    /// 楷体(二级标题)。
    pub kai: Option<PathBuf>,
}

/// 扫描系统字体目录,返回 `(family 名, 字体文件路径)` 列表。
///
/// 结果按 family 名排序;同一 family 只保留一个路径,优先保留 Regular/Book
/// 权重,再退化为任意一个。
#[must_use]
pub fn discover_system_fonts() -> Vec<(String, PathBuf)> {
    let mut fonts = Vec::new();
    let mut visited = HashSet::new();
    for dir in system_font_dirs() {
        scan_font_dir(&dir, &mut fonts, &mut visited);
    }
    dedup_fonts(fonts)
}

/// 从系统字体里解析公文四字体,缺失的回退到通用 CJK 字体。
#[must_use]
pub fn resolve_font_set() -> FontSet {
    let fonts = discover_system_fonts();

    let serif = find_font(&fonts, SERIF_FALLBACK_KEYWORDS);
    let sans = find_font(&fonts, SANS_FALLBACK_KEYWORDS);

    FontSet {
        title: find_font(&fonts, TITLE_KEYWORDS).or_else(|| serif.clone()),
        body: find_font(&fonts, BODY_KEYWORDS).or_else(|| serif.clone()),
        hei: find_font(&fonts, HEI_KEYWORDS).or_else(|| sans.clone()),
        kai: find_font(&fonts, KAI_KEYWORDS).or_else(|| serif.clone()),
    }
}

/// 把一个逻辑字体名解析为系统里实际的字体文件路径。
///
/// 匹配顺序:先做 family 名的精确(大小写不敏感)匹配,再做 family/文件名的
/// 子串匹配,最后按字体名的语义回退——含「黑/hei/sans/gothic」等字样的名字
/// 回退到通用 CJK 无衬线(黑体),其余回退到 CJK 衬线(宋体)。找不到时返回 `None`。
#[must_use]
pub fn resolve_font_path(font_name: &str, fonts: &[(String, PathBuf)]) -> Option<PathBuf> {
    let needle = font_name.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }

    // 1. 精确 family 匹配。
    for (family, path) in fonts {
        if family.to_lowercase() == needle {
            return Some(path.clone());
        }
    }
    // 2. family 或文件名子串匹配。
    for (family, path) in fonts {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if family.to_lowercase().contains(&needle) || file_name.contains(&needle) {
            return Some(path.clone());
        }
    }
    // 3. 按名字语义回退到 CJK 字体。
    let looks_sans = ["黑", "hei", "sans", "gothic", "zenhei"]
        .iter()
        .any(|keyword| needle.contains(keyword));
    if looks_sans {
        find_font(fonts, SANS_FALLBACK_KEYWORDS)
    } else {
        find_font(fonts, SERIF_FALLBACK_KEYWORDS)
    }
}

/// 返回各平台需要扫描的系统字体目录。
///
/// 目录可能不存在(例如 `~/.local/share/fonts`),扫描时会静默跳过。
fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match env::consts::OS {
        "windows" => {
            dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
        }
        "macos" => {
            dirs.push(PathBuf::from("/System/Library/Fonts"));
            dirs.push(PathBuf::from("/Library/Fonts"));
            if let Some(home) = env::var_os("HOME") {
                dirs.push(PathBuf::from(home).join("Library/Fonts"));
            }
        }
        _ => {
            dirs.push(PathBuf::from("/usr/share/fonts"));
            dirs.push(PathBuf::from("/usr/local/share/fonts"));
            if let Some(home) = env::var_os("HOME") {
                let home = PathBuf::from(home);
                dirs.push(home.join(".fonts"));
                dirs.push(home.join(".local/share/fonts"));
            }
        }
    }
    dirs
}

/// 递归扫描 `dir`,把发现的字体追加到 `fonts`。
///
/// 使用 `visited`(已规范化路径集合)避免符号链接造成的无限递归;权限不足、
/// 路径不存在等情况一律静默跳过。
fn scan_font_dir(dir: &Path, fonts: &mut Vec<(String, PathBuf)>, visited: &mut HashSet<PathBuf>) {
    let Ok(canonical) = dir.canonicalize() else {
        return;
    };
    if !visited.insert(canonical) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // 用 metadata(跟随符号链接)判断目录/文件,配合 visited 防环。
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            scan_font_dir(&path, fonts, visited);
        } else if metadata.is_file() {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if is_font_file(file_name) {
                fonts.push((family_from_file_name(file_name), path));
            }
        }
    }
}

/// 判断文件名是否属于字体文件(`.ttf` / `.otf` / `.ttc` / `.otc`,大小写不敏感)。
fn is_font_file(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ext.eq_ignore_ascii_case("ttf")
                || ext.eq_ignore_ascii_case("otf")
                || ext.eq_ignore_ascii_case("ttc")
                || ext.eq_ignore_ascii_case("otc")
        })
}

/// 同一 family 去重:保留 Regular/Book 权重最优的路径,按 family 排序输出。
fn dedup_fonts(fonts: Vec<(String, PathBuf)>) -> Vec<(String, PathBuf)> {
    let mut best: HashMap<String, (u8, String, PathBuf)> = HashMap::new();
    for (family, path) in fonts {
        let key = family.to_lowercase();
        let rank = weight_rank(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        );
        match best.entry(key) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert((rank, family, path));
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if rank > slot.get().0 {
                    slot.insert((rank, family, path));
                }
            }
        }
    }
    let mut result: Vec<(String, PathBuf)> = best
        .into_values()
        .map(|(_, family, path)| (family, path))
        .collect();
    result.sort();
    result
}

/// 根据文件名里的权重标记给出去重优先级:Regular/Book > Bold/Medium > 其它。
fn weight_rank(file_name: &str) -> u8 {
    let lower = file_name.to_ascii_lowercase();
    if lower.contains("regular") || lower.contains("-book") || lower.contains("_book") {
        return 3;
    }
    if lower.contains("medium") || lower.contains("bold") {
        return 2;
    }
    u8::from(lower.contains("light") || lower.contains("thin") || lower.contains("black"))
}

/// 从文件名推断可读的 family 名。
///
/// 步骤:去掉扩展名 → 去掉 `[...]` 变量轴 → 按 `-`/`_`/空白切分 → 丢弃末尾的
/// 样式/权重词(Regular、Bold 等)→ 对每个词做驼峰拆分 → 用空格连接。
fn family_from_file_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map_or_else(|| file_name.to_string(), |s| s.to_string_lossy().into_owned());
    let stem = strip_variable_axis(&stem).to_string();

    let tokens: Vec<&str> = stem
        .split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|token| !token.is_empty())
        .collect();

    let mut family_tokens = tokens.as_slice();
    while family_tokens
        .last()
        .is_some_and(|token| is_style_token(token))
    {
        family_tokens = &family_tokens[..family_tokens.len() - 1];
    }
    if family_tokens.is_empty() {
        family_tokens = tokens.as_slice();
    }

    family_tokens
        .iter()
        .map(|token| split_camel_case(token))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 去掉变量字体的轴声明,例如 `Ubuntu[wdth,wght]` → `Ubuntu`。
fn strip_variable_axis(stem: &str) -> &str {
    stem.split_once('[').map_or(stem, |(base, _)| base)
}

/// 判断一个词是否为样式/权重后缀(Regular、Bold、Italic、Light 等)。
fn is_style_token(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "regular"
            | "book"
            | "normal"
            | "roman"
            | "italic"
            | "oblique"
            | "bold"
            | "bolditalic"
            | "boldoblique"
            | "semibold"
            | "demibold"
            | "extrabold"
            | "black"
            | "heavy"
            | "light"
            | "extralight"
            | "semilight"
            | "demilight"
            | "thin"
            | "ultra"
            | "ultralight"
            | "medium"
    )
}

/// 在驼峰边界插入空格,把 `NotoSerifCJK` 变成 `Noto Serif CJK`。
///
/// 规则:大写字母前插入空格,除非它是首字母、或处在连续大写(缩写)内部且
/// 后一个字符不是小写。
fn split_camel_case(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 4);
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_uppercase() {
            let next_is_lower = chars.get(index + 1).is_some_and(|next| next.is_lowercase());
            let insert_space = index
                .checked_sub(1)
                .and_then(|prev| chars.get(prev))
                .is_some_and(|prev| {
                    prev.is_lowercase()
                        || prev.is_ascii_digit()
                        || (prev.is_uppercase() && next_is_lower)
                });
            if insert_space {
                out.push(' ');
            }
        }
        out.push(ch);
    }
    out
}

/// 在已发现字体中按关键字优先级查找,返回第一个命中字体的路径。
///
/// 对每个关键字,依次在 family 名与文件名的(小写)子串里匹配,大小写不敏感。
/// 字体列表已按 family 排序,因此结果确定。
fn find_font(fonts: &[(String, PathBuf)], keywords: &[&str]) -> Option<PathBuf> {
    for keyword in keywords {
        let needle = keyword.to_lowercase();
        for (family, path) in fonts {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if family.to_lowercase().contains(&needle)
                || file_name.to_ascii_lowercase().contains(&needle)
            {
                return Some(path.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_name_is_inferred_from_file_name() {
        assert_eq!(
            family_from_file_name("NotoSerifCJK-Regular.ttc"),
            "Noto Serif CJK"
        );
        assert_eq!(
            family_from_file_name("NotoSerifCJK-Bold.ttc"),
            "Noto Serif CJK"
        );
        assert_eq!(
            family_from_file_name("SourceHanSansSC-Regular.otf"),
            "Source Han Sans SC"
        );
        assert_eq!(family_from_file_name("wqy-zenhei.ttc"), "wqy zenhei");
        assert_eq!(family_from_file_name("ukai.ttc"), "ukai");
        assert_eq!(family_from_file_name("FangSong.ttf"), "Fang Song");
        assert_eq!(
            family_from_file_name("DroidSansFallbackFull.ttf"),
            "Droid Sans Fallback Full"
        );
        assert_eq!(family_from_file_name("Ubuntu[wdth,wght].ttf"), "Ubuntu");
        // 驼峰规则会把小写→大写边界都切开,因此品牌名 DejaVu 也会被拆成 Deja Vu。
        assert_eq!(
            family_from_file_name("DejaVuSansMono-Bold.ttf"),
            "Deja Vu Sans Mono"
        );
    }

    #[test]
    fn camel_case_is_split() {
        assert_eq!(split_camel_case("NotoSerifCJK"), "Noto Serif CJK");
        assert_eq!(split_camel_case("SourceHanSans"), "Source Han Sans");
        assert_eq!(split_camel_case("wqy"), "wqy");
        assert_eq!(split_camel_case("SC"), "SC");
    }

    #[test]
    fn style_tokens_are_recognized() {
        assert!(is_style_token("Regular"));
        assert!(is_style_token("BoldItalic"));
        assert!(is_style_token("ExtraLight"));
        assert!(!is_style_token("NotoSerifCJK"));
        assert!(!is_style_token("zenhei"));
    }

    #[test]
    fn regular_weight_outranks_others() {
        assert!(weight_rank("NotoSerifCJK-Regular.ttc") > weight_rank("NotoSerifCJK-Bold.ttc"));
        assert!(weight_rank("X-Bold.ttf") > weight_rank("X-Thin.ttf"));
    }

    #[test]
    fn keyword_match_is_case_insensitive_on_family_and_file_name() {
        let fonts = vec![
            ("Noto Serif CJK".to_string(), PathBuf::from("/f/NotoSerifCJK-Regular.ttc")),
            ("wqy zenhei".to_string(), PathBuf::from("/f/wqy-zenhei.ttc")),
        ];
        assert_eq!(
            find_font(&fonts, &["noto serif cjk"]),
            Some(PathBuf::from("/f/NotoSerifCJK-Regular.ttc"))
        );
        assert_eq!(
            find_font(&fonts, &["hei"]),
            Some(PathBuf::from("/f/wqy-zenhei.ttc"))
        );
        assert_eq!(find_font(&fonts, &["仿宋"]), None);
    }

    #[test]
    fn specific_kai_keyword_wins_over_generic_kai() {
        // "kai" 会误匹配 Kaithi 文字字体,因此 ukai 必须排在其前面。
        let fonts = vec![
            ("Noto Sans Kaithi".to_string(), PathBuf::from("/f/NotoSansKaithi-Regular.ttf")),
            ("ukai".to_string(), PathBuf::from("/f/ukai.ttc")),
        ];
        assert_eq!(
            find_font(&fonts, KAI_KEYWORDS),
            Some(PathBuf::from("/f/ukai.ttc"))
        );
    }
}
