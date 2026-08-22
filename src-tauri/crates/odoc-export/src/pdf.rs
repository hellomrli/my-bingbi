use super::Gb9704Style;
use odoc_model::{Block, ODoc};
use std::fmt::Write as _;

/// Produces a print-preview HTML representation used when no usable CJK font
/// is available for the embedded PDF engine.
pub fn print_html(document: &ODoc, style: &Gb9704Style) -> String {
    let mut html = String::from("<!doctype html><meta charset=\"utf-8\"><style>");
    let right = style.page.width_mm - style.page.margin_left_mm - style.page.body_width_mm;
    let bottom = style.page.height_mm - style.page.margin_top_mm - style.page.body_height_mm;
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
    html
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
