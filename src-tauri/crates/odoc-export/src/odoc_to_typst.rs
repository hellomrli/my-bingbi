//! Renders an [`ODoc`] as Typst markup.
//!
//! The produced source is plain Typst text: it references fonts by family name
//! (as configured in the [`Gb9704Style`]) and leaves the family-name-to-file
//! resolution to the rendering layer. All user-provided text is escaped so that
//! Typst markup characters cannot break out of the intended structure.

use std::fmt::Write as _;

use odoc_model::{Block, HeadingLevel, ListStyle, ODoc};

use crate::style::{FontStyle, Gb9704Style};

/// Converts an `ODoc` into a complete Typst source string.
///
/// The output starts with the page/text/paragraph setup derived from `style`
/// and then renders the header mark, title, recipients, body blocks,
/// attachments, signer/date and the footer. Empty optional fields are skipped.
pub fn odoc_to_typst(document: &ODoc, style: &Gb9704Style) -> String {
    let mut out = String::with_capacity(2048);
    write_page_setup(&mut out, style);

    if let Some(header) = &document.header {
        write_header(&mut out, header.issuing_authority_mark.as_str(), style);
        if let Some(number) = &header.document_number {
            write_centered(&mut out, number, &style.fonts.document_number, style);
        }
    }

    if !document.body.title.text.trim().is_empty() {
        write_centered(
            &mut out,
            &document.body.title.text,
            &style.fonts.title,
            style,
        );
    }

    if !document.body.main_recipients.is_empty() {
        let recipients = document.body.main_recipients.join("、");
        write_flush_paragraph(&mut out, &recipients, style);
    }

    for block in &document.body.blocks {
        write_block(&mut out, block, style);
    }

    if !document.body.attachments.is_empty() {
        let mut attachment_text = String::from("附件：");
        let titles = document
            .body
            .attachments
            .iter()
            .map(|attachment| attachment.title.as_str())
            .collect::<Vec<_>>()
            .join("；");
        attachment_text.push_str(&titles);
        write_flush_paragraph(&mut out, &attachment_text, style);
    }

    if let Some(signer) = &document.body.signer {
        write_signer(&mut out, signer, document.body.date.as_ref(), style);
    }

    if let Some(note) = &document.body.note {
        if !note.trim().is_empty() {
            write_flush_paragraph(&mut out, note, style);
        }
    }

    if let Some(footer) = &document.footer {
        write_footer(&mut out, footer, style);
    }

    out
}

fn write_page_setup(out: &mut String, style: &Gb9704Style) {
    let right_margin = style.page.width_mm - style.page.margin_left_mm - style.page.body_width_mm;
    let bottom_margin = style.page.height_mm - style.page.margin_top_mm - style.page.body_height_mm;
    let _ = writeln!(
        out,
        "#set page(paper: \"a4\", margin: (top: {}mm, bottom: {}mm, left: {}mm, right: {}mm))",
        style.page.margin_top_mm, bottom_margin, style.page.margin_left_mm, right_margin
    );
    let _ = writeln!(
        out,
        "#set text(font: \"{}\", size: {}pt)",
        escape_font_name(&style.fonts.body.font),
        style.fonts.body.size_pt
    );
    let _ = writeln!(
        out,
        "#set par(leading: {}pt, first-line-indent: (amount: 2em, all: true))",
        style.page.line_spacing_pt
    );
    let _ = writeln!(out);
}

fn write_header(out: &mut String, mark: &str, style: &Gb9704Style) {
    if mark.trim().is_empty() {
        return;
    }
    let color = style.header.organization_mark_color.as_str();
    let title_font = &style.fonts.title;
    let _ = writeln!(out, "#align(center)[");
    let _ = writeln!(out, "  #set par(first-line-indent: 0pt)");
    let _ = writeln!(
        out,
        "  #text(font: \"{}\", size: {}pt, fill: rgb(\"#{}\"))[{}]",
        escape_font_name(&title_font.font),
        title_font.size_pt,
        color.trim_start_matches('#'),
        escape(mark)
    );
    let _ = writeln!(out, "]");
    let _ = writeln!(out);
}

fn write_centered(out: &mut String, text: &str, font: &FontStyle, style: &Gb9704Style) {
    let _ = writeln!(out, "#align(center)[");
    let _ = writeln!(
        out,
        "  #set par(first-line-indent: 0pt, leading: {}pt)",
        style.page.line_spacing_pt
    );
    let _ = writeln!(
        out,
        "  #text(font: \"{}\", size: {}pt)[{}]",
        escape_font_name(&font.font),
        font.size_pt,
        escape(text)
    );
    let _ = writeln!(out, "]");
    let _ = writeln!(out);
}

/// A paragraph with no first-line indent (flush left), used for recipients,
/// attachment notes and similar top-level lines.
fn write_flush_paragraph(out: &mut String, text: &str, style: &Gb9704Style) {
    let _ = writeln!(out, "#block[");
    let _ = writeln!(
        out,
        "  #set par(first-line-indent: 0pt, leading: {}pt)",
        style.page.line_spacing_pt
    );
    let _ = writeln!(out, "  {}", escape(text));
    let _ = writeln!(out, "]");
    let _ = writeln!(out);
}

fn write_block(out: &mut String, block: &Block, style: &Gb9704Style) {
    match block {
        Block::Heading { content, level, .. } => write_heading(out, *level, content, style),
        Block::Paragraph { content, .. } => {
            if !content.trim().is_empty() {
                let _ = writeln!(out, "{}", escape(content));
                let _ = writeln!(out);
            }
        }
        Block::List {
            style: list_style,
            items,
            ..
        } => write_list(out, *list_style, items),
        Block::Table { rows, .. } => write_table(out, rows),
        Block::Quote { content, .. } => {
            let _ = writeln!(out, "#block[");
            let _ = writeln!(
                out,
                "  #set par(first-line-indent: 0pt, leading: {}pt)",
                style.page.line_spacing_pt
            );
            let _ = writeln!(
                out,
                "  #text(style: \"italic\")[\u{201c}{}\u{201d}]",
                escape(content)
            );
            let _ = writeln!(out, "]");
            let _ = writeln!(out);
        }
    }
}

fn write_heading(out: &mut String, level: HeadingLevel, content: &str, style: &Gb9704Style) {
    let font = heading_font(level, style);
    // Level 1/2 headings sit flush left in GB/T 9704; level 3/4 use the body
    // indent like regular paragraphs.
    if matches!(level, HeadingLevel::L1 | HeadingLevel::L2) {
        let _ = writeln!(out, "#block[");
        let _ = writeln!(
            out,
            "  #set par(first-line-indent: 0pt, leading: {}pt)",
            style.page.line_spacing_pt
        );
        let _ = writeln!(
            out,
            "  #text(font: \"{}\", size: {}pt)[{}]",
            escape_font_name(&font.font),
            font.size_pt,
            escape(content)
        );
        let _ = writeln!(out, "]");
    } else {
        let _ = writeln!(
            out,
            "#text(font: \"{}\", size: {}pt)[{}]",
            escape_font_name(&font.font),
            font.size_pt,
            escape(content)
        );
    }
    let _ = writeln!(out);
}

fn heading_font(level: HeadingLevel, style: &Gb9704Style) -> &FontStyle {
    match level {
        HeadingLevel::L1 => &style.fonts.heading_l1,
        HeadingLevel::L2 => &style.fonts.heading_l2,
        HeadingLevel::L3 => &style.fonts.heading_l3,
        HeadingLevel::L4 => &style.fonts.heading_l4,
    }
}

fn write_list(out: &mut String, style: ListStyle, items: &[odoc_model::ListItem]) {
    let marker = match style {
        ListStyle::Ordered => '+',
        ListStyle::Unordered => '-',
    };
    for item in items {
        let _ = writeln!(out, "{} {}", marker, escape(&item.content));
    }
    let _ = writeln!(out);
}

fn write_table(out: &mut String, rows: &[odoc_model::TableRow]) {
    let Some(columns) = rows.first().map(|row| row.cells.len()) else {
        return;
    };
    if columns == 0 {
        return;
    }
    let _ = writeln!(out, "#table(");
    let _ = writeln!(out, "  columns: {columns},");
    for row in rows {
        for cell in row.cells.iter().take(columns) {
            let _ = writeln!(out, "  [{}],", escape(&cell.content));
        }
    }
    let _ = writeln!(out, ")");
    let _ = writeln!(out);
}

fn write_signer(
    out: &mut String,
    signer: &odoc_model::Signer,
    date: Option<&odoc_model::OfficialDate>,
    style: &Gb9704Style,
) {
    let mut lines = Vec::new();
    if !signer.authority.trim().is_empty() {
        lines.push(signer.authority.clone());
    }
    if let Some(date) = date {
        if !date.value.trim().is_empty() {
            lines.push(date.value.clone());
        }
    }
    if lines.is_empty() {
        return;
    }
    let _ = writeln!(out, "#align(right)[");
    let _ = writeln!(
        out,
        "  #set par(first-line-indent: 0pt, leading: {}pt)",
        style.page.line_spacing_pt
    );
    for line in lines {
        let _ = writeln!(out, "  {}", escape(&line));
    }
    let _ = writeln!(out, "]");
    let _ = writeln!(out);
}

fn write_footer(out: &mut String, footer: &odoc_model::Footer, style: &Gb9704Style) {
    let _ = writeln!(out, "#v(1fr)");
    let _ = writeln!(out, "#line(length: 100%)");
    let _ = writeln!(
        out,
        "#set par(first-line-indent: 0pt, leading: {}pt)",
        style.page.line_spacing_pt
    );

    if !footer.copy_recipients.is_empty() {
        let _ = writeln!(out, "抄送：{}", escape(&footer.copy_recipients.join("、")));
    }
    if let Some(authority) = &footer.printing_authority {
        if !authority.trim().is_empty() {
            let _ = writeln!(out, "{}", escape(authority));
        }
    }
    if let Some(date) = &footer.printing_date {
        if !date.trim().is_empty() {
            let _ = writeln!(out, "{}", escape(date));
        }
    }
    let _ = writeln!(out);
}

/// Escapes Typst markup characters in user text so they render literally.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '#' | '$' | '*' | '_' | '[' | ']' | '<' | '>' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Escapes a font family name for use inside a Typst string literal.
fn escape_font_name(font: &str) -> String {
    font.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_model::{Block, ListItem, ODoc, TableCell, TableRow};

    fn style() -> Gb9704Style {
        Gb9704Style::default()
    }

    fn paragraph(content: &str) -> Block {
        Block::Paragraph {
            id: String::new(),
            content: content.to_owned(),
        }
    }

    #[test]
    fn escapes_markup_characters() {
        assert_eq!(
            escape("#号 $价 *星 _线 [方]"),
            "\\#号 \\$价 \\*星 \\_线 \\[方\\]"
        );
        assert_eq!(escape("普通中文。"), "普通中文。");
    }

    #[test]
    fn emits_page_setup_and_title() {
        let mut document = ODoc::empty();
        document.body.title.text = "关于测试的通知".to_owned();
        document.body.blocks = vec![paragraph("正文第一段。")];
        let source = odoc_to_typst(&document, &style());
        assert!(source.contains("#set page(paper: \"a4\""));
        assert!(source.contains("关于测试的通知"));
        assert!(source.contains("正文第一段。"));
    }

    #[test]
    fn renders_list_and_table() {
        let mut document = ODoc::empty();
        document.body.blocks = vec![
            Block::List {
                id: String::new(),
                style: ListStyle::Ordered,
                items: vec![ListItem {
                    id: String::new(),
                    content: "第一项".to_owned(),
                }],
            },
            Block::Table {
                id: String::new(),
                rows: vec![TableRow {
                    id: String::new(),
                    cells: vec![TableCell {
                        id: String::new(),
                        content: "单元格".to_owned(),
                    }],
                }],
            },
        ];
        let source = odoc_to_typst(&document, &style());
        assert!(source.contains("+ 第一项"));
        assert!(source.contains("#table("));
        assert!(source.contains("单元格"));
    }

    #[test]
    fn generated_typst_compiles_to_a_pdf() {
        let mut document = ODoc::empty();
        document.header.as_mut().unwrap().issuing_authority_mark = "测试市人民政府".to_owned();
        document.header.as_mut().unwrap().document_number = Some("测试府发〔2024〕1号".to_owned());
        document.body.title.text = "关于零依赖导出的通知".to_owned();
        document.body.main_recipients = vec!["各县（市）区人民政府".to_owned()];
        document.body.blocks = vec![
            Block::Heading {
                id: String::new(),
                level: HeadingLevel::L1,
                content: "一、总体要求".to_owned(),
            },
            paragraph("这是正文第一段，验证 Typst 源码能被编译成 PDF。"),
            Block::List {
                id: String::new(),
                style: ListStyle::Ordered,
                items: vec![ListItem {
                    id: String::new(),
                    content: "第一项".to_owned(),
                }],
            },
        ];
        document.body.signer = Some(odoc_model::Signer {
            id: String::new(),
            authority: "测试市人民政府".to_owned(),
            representative: None,
        });
        document.body.date = Some(odoc_model::OfficialDate {
            id: String::new(),
            value: "2024年1月1日".to_owned(),
        });

        let style = style();
        let source = odoc_to_typst(&document, &style);

        // Resolve the four semantic font slots to real files; skip when the
        // host has no CJK fonts at all.
        let set = crate::resolve_font_set();
        let mut fonts: Vec<(String, std::path::PathBuf)> = Vec::new();
        for (name, path) in [
            (style.fonts.title.font.as_str(), set.title.as_ref()),
            (style.fonts.body.font.as_str(), set.body.as_ref()),
            (style.fonts.heading_l1.font.as_str(), set.hei.as_ref()),
            (style.fonts.heading_l2.font.as_str(), set.kai.as_ref()),
        ] {
            if let Some(path) = path {
                fonts.push((name.to_owned(), path.clone()));
            }
        }
        if fonts.is_empty() {
            eprintln!("skipping generated_typst_compiles_to_a_pdf: no CJK fonts found");
            return;
        }

        let bytes =
            crate::render_typst_pdf(&source, &fonts).expect("Typst compilation should succeed");
        assert!(bytes.starts_with(b"%PDF"), "output must be a PDF");
    }
}
