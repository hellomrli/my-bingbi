use super::Gb9704Style;
use odoc_model::{Block, ODoc};
use std::fmt::Write as _;

/// Produces a print-preview HTML representation used when no usable CJK font
/// is available for the embedded PDF engine.
pub fn print_html(document: &ODoc, style: &Gb9704Style) -> String {
    let mut html = String::new();
    push_page_style(&mut html, style);
    push_header(&mut html, document);
    push_body(&mut html, document);
    push_attachments(&mut html, document);
    push_signature(&mut html, document);
    push_footer(&mut html, document);
    html
}

fn push_page_style(html: &mut String, style: &Gb9704Style) {
    let right = style.page.width_mm - style.page.margin_left_mm - style.page.body_width_mm;
    let bottom = style.page.height_mm - style.page.margin_top_mm - style.page.body_height_mm;
    html.push_str("<!doctype html><meta charset=\"utf-8\"><style>");
    let _ = write!(
        html,
        "@page{{size:A4;margin:{}mm {}mm {}mm {}mm}}body{{font-family:'{}',serif;font-size:{}pt;line-height:{}pt;color:#111}}h1{{text-align:center;font-family:'{}';font-size:{}pt}}h2,h3,h4{{font-size:{}pt}}p{{margin:0 0 0.4em;text-indent:2em}}header{{color:#",
        style.page.margin_top_mm,
        right,
        bottom,
        style.page.margin_left_mm,
        css_string(&style.fonts.body.font),
        style.fonts.body.size_pt,
        style.page.line_spacing_pt,
        css_string(&style.fonts.title.font),
        style.fonts.title.size_pt,
        style.fonts.body.size_pt,
    );
    html.push_str(style.mark_color());
    html.push_str(";text-align:center;font-family:'");
    html.push_str(&css_string(&style.fonts.title.font));
    html.push_str("';font-size:");
    html.push_str(&style.fonts.title.size_pt.to_string());
    html.push_str("pt;border-bottom:1px solid currentColor;padding-bottom:4pt}</style>");
}

fn push_header(html: &mut String, document: &ODoc) {
    if let Some(header) = &document.header {
        if !header.issuing_authority_mark.trim().is_empty() {
            html.push_str("<header>");
            html.push_str(&html_escape(&header.issuing_authority_mark));
            html.push_str("</header>");
        }
        if let Some(number) = &header.document_number {
            html.push_str("<p style=\"text-align:center;text-indent:0\">");
            html.push_str(&html_escape(number));
            html.push_str("</p>");
        }
    }
    html.push_str("<h1>");
    html.push_str(&html_escape(&document.body.title.text));
    html.push_str("</h1>");
    for recipient in &document.body.main_recipients {
        html.push_str("<p style=\"text-indent:0\">");
        html.push_str(&html_escape(recipient));
        html.push_str("</p>");
    }
}

fn push_body(html: &mut String, document: &ODoc) {
    for block in &document.body.blocks {
        if let Block::Heading { content, level, .. } = block {
            let tag = match level {
                odoc_model::HeadingLevel::L1 => "h2",
                odoc_model::HeadingLevel::L2 => "h3",
                odoc_model::HeadingLevel::L3 | odoc_model::HeadingLevel::L4 => "h4",
            };
            html.push('<');
            html.push_str(tag);
            html.push('>');
            html.push_str(&html_escape(content));
            html.push_str("</");
            html.push_str(tag);
            html.push('>');
        } else {
            html.push_str("<p>");
            html.push_str(&html_escape(&block.plain_text()));
            html.push_str("</p>");
        }
    }
}

fn push_attachments(html: &mut String, document: &ODoc) {
    let mut attachments: Vec<_> = document.body.attachments.iter().collect();
    attachments.sort_by_key(|attachment| attachment.order);
    if attachments.is_empty() {
        return;
    }
    html.push_str("<p style=\"text-indent:0;margin-top:1em\">附件：</p>");
    for attachment in attachments {
        html.push_str("<p style=\"text-indent:0;margin-left:2em\">");
        html.push_str(&html_escape(&format!(
            "{}. {}",
            attachment.order, attachment.title
        )));
        if let Some(note) = attachment
            .note
            .as_deref()
            .filter(|note| !note.trim().is_empty())
        {
            html.push_str("<span>（");
            html.push_str(&html_escape(note));
            html.push_str("）</span>");
        }
        html.push_str("</p>");
    }
}

fn push_signature(html: &mut String, document: &ODoc) {
    if let Some(signer) = &document.body.signer {
        html.push_str("<p style=\"text-align:right;text-indent:0\">");
        html.push_str(&html_escape(&signer.authority));
        html.push_str("</p>");
    }
    if let Some(date) = &document.body.date {
        html.push_str("<p style=\"text-align:right;text-indent:0\">");
        html.push_str(&html_escape(&date.value));
        html.push_str("</p>");
    }
    if let Some(note) = document
        .body
        .note
        .as_deref()
        .filter(|note| !note.trim().is_empty())
    {
        html.push_str("<p style=\"text-indent:0;margin-top:1em\">");
        html.push_str(&html_escape(note));
        html.push_str("</p>");
    }
}

fn push_footer(html: &mut String, document: &ODoc) {
    let Some(footer) = &document.footer else {
        return;
    };
    html.push_str("<hr style=\"margin-top:2em;border:0;border-top:1px solid #333\">");
    if !footer.copy_recipients.is_empty() {
        let recipients = footer
            .copy_recipients
            .iter()
            .map(|recipient| html_escape(recipient))
            .collect::<Vec<_>>()
            .join("，");
        html.push_str("<p style=\"text-indent:0\">抄送：");
        html.push_str(&recipients);
        html.push_str("。</p>");
    }
    if let Some(authority) = footer
        .printing_authority
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        html.push_str("<p style=\"text-indent:0\">");
        html.push_str(&html_escape(authority));
        html.push_str("</p>");
    }
    if let Some(date) = footer
        .printing_date
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        html.push_str("<p style=\"text-align:right;text-indent:0\">");
        html.push_str(&html_escape(date));
        html.push_str("印发</p>");
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn css_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace(['\n', '\r'], " ")
        .replace(['<', '>'], "")
}

#[cfg(test)]
mod tests {
    use super::{Gb9704Style, print_html};
    use odoc_model::{Attachment, Block, Footer, ODoc};

    fn base_document() -> ODoc {
        let mut document = ODoc::empty();
        document.body.title.text = "关于开展检查的通知".to_owned();
        document
    }

    #[test]
    fn escapes_document_text() {
        let mut document = base_document();
        document.body.title.text = "<script>alert(1)</script>".to_owned();
        document.body.blocks = vec![Block::paragraph("5 < 6 & 7 > 4".to_owned())];

        let html = print_html(&document, &Gb9704Style::default());

        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("5 &lt; 6 &amp; 7 &gt; 4"));
    }

    #[test]
    fn renders_attachments_in_order_then_note_and_footer() {
        let mut document = base_document();
        document.body.attachments = vec![
            Attachment {
                id: "a2".to_owned(),
                order: 2,
                title: "材料二".to_owned(),
                note: Some("秘密".to_owned()),
            },
            Attachment {
                id: "a1".to_owned(),
                order: 1,
                title: "材料一".to_owned(),
                note: None,
            },
        ];
        document.body.note = Some("此件公开发布".to_owned());
        document.footer = Some(Footer {
            id: "f1".to_owned(),
            copy_recipients: vec!["×××局".to_owned(), "×××办".to_owned()],
            printing_authority: Some("×××办公室".to_owned()),
            printing_date: Some("2026年8月12日".to_owned()),
        });

        let html = print_html(&document, &Gb9704Style::default());

        assert!(html.contains("附件："));
        let first = html.find("1. 材料一").expect("attachment 1");
        let second = html.find("2. 材料二").expect("attachment 2");
        assert!(first < second, "attachments follow their order field");
        assert!(html.contains("（秘密）"));
        assert!(html.contains("此件公开发布"));
        assert!(html.contains("抄送：×××局，×××办。"));
        assert!(html.contains("×××办公室"));
        assert!(html.contains("2026年8月12日印发"));
    }

    #[test]
    fn omits_empty_footer_fields() {
        let mut document = base_document();
        document.footer = Some(Footer {
            id: "f1".to_owned(),
            copy_recipients: vec![],
            printing_authority: None,
            printing_date: None,
        });

        let html = print_html(&document, &Gb9704Style::default());

        assert!(!html.contains("抄送："));
        assert!(!html.contains("印发</p>"));
    }
}
