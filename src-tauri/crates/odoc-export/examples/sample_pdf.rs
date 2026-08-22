//! Generates a sample official-document PDF and writes it to `/tmp/bingbi_sample.pdf`
//! plus the intermediate Typst source to `/tmp/bingbi_sample.typ`.
//!
//! Run with:
//! `cargo run -p odoc-export --example sample_pdf`

use odoc_export::{Gb9704Style, odoc_to_typst, render_typst_pdf, resolve_font_set};
use odoc_model::{Block, HeadingLevel, ODoc};

fn main() {
    let mut document = ODoc::empty();
    document.header.as_mut().unwrap().issuing_authority_mark = "测试市人民政府文件".into();
    document.header.as_mut().unwrap().document_number = Some("测试府发〔2024〕1号".into());
    document.body.title.text = "关于零依赖 PDF 导出的通知".into();
    document.body.main_recipients = vec!["各县（市）区人民政府，市政府各部门：".into()];
    document.body.blocks = vec![
        Block::Paragraph {
            id: "p1".into(),
            content: "为验证内置排版引擎，特印发本通知。".into(),
        },
        Block::Heading {
            id: "h1".into(),
            level: HeadingLevel::L1,
            content: "一、总体要求".into(),
        },
        Block::Paragraph {
            id: "p2".into(),
            content: "坚持以人民为中心的发展思想，统筹推进各项工作。".into(),
        },
        Block::Heading {
            id: "h2".into(),
            level: HeadingLevel::L2,
            content: "（一）加强组织领导".into(),
        },
        Block::Paragraph {
            id: "p3".into(),
            content: "各地各部门要高度重视，明确责任分工。".into(),
        },
    ];
    document.body.signer = Some(odoc_model::Signer {
        id: "s1".into(),
        authority: "测试市人民政府".into(),
        representative: None,
    });
    document.body.date = Some(odoc_model::OfficialDate {
        id: "d1".into(),
        value: "2024年1月1日".into(),
    });

    let style = Gb9704Style::default();
    let source = odoc_to_typst(&document, &style);
    std::fs::write("/tmp/bingbi_sample.typ", &source).unwrap();

    let set = resolve_font_set();
    let mut fonts = Vec::new();
    for (name, path) in [
        (style.fonts.title.font.as_str(), set.title.as_ref()),
        (style.fonts.body.font.as_str(), set.body.as_ref()),
        (style.fonts.heading_l1.font.as_str(), set.hei.as_ref()),
        (style.fonts.heading_l2.font.as_str(), set.kai.as_ref()),
    ] {
        if let Some(path) = path {
            fonts.push((name.into(), path.clone()));
        }
    }
    eprintln!(
        "resolved fonts: {:?}",
        fonts
            .iter()
            .map(|(name, path)| (name, path.display().to_string()))
            .collect::<Vec<_>>()
    );

    let bytes = render_typst_pdf(&source, &fonts).expect("PDF render failed");
    std::fs::write("/tmp/bingbi_sample.pdf", &bytes).unwrap();
    println!(
        "PDF written to /tmp/bingbi_sample.pdf ({} bytes)",
        bytes.len()
    );
}
