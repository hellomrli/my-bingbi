use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

const TWIPS_PER_MM: f64 = 1440.0 / 25.4;

#[derive(Debug, Error)]
pub enum StyleError {
    #[error("style configuration is malformed")]
    InvalidToml,
    #[error("style configuration contains an invalid value")]
    InvalidValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gb9704Style {
    pub page: PageStyle,
    pub fonts: FontStyles,
    pub header: HeaderStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageStyle {
    pub size: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub margin_top_mm: f64,
    pub margin_left_mm: f64,
    pub body_width_mm: f64,
    pub body_height_mm: f64,
    pub line_spacing_pt: f64,
    pub lines_per_page: u32,
    pub chars_per_line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontStyles {
    pub title: FontStyle,
    pub body: FontStyle,
    pub heading_l1: FontStyle,
    pub heading_l2: FontStyle,
    pub heading_l3: FontStyle,
    pub heading_l4: FontStyle,
    pub document_number: FontStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontStyle {
    pub font: String,
    pub size_pt: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderStyle {
    pub organization_mark_color: String,
    pub separator_line: bool,
}

impl Default for Gb9704Style {
    fn default() -> Self {
        let body = FontStyle::new("仿宋_GB2312", 16.0);
        Self {
            page: PageStyle {
                size: "A4".to_owned(),
                width_mm: 210.0,
                height_mm: 297.0,
                margin_top_mm: 37.0,
                margin_left_mm: 28.0,
                body_width_mm: 156.0,
                body_height_mm: 225.0,
                line_spacing_pt: 28.0,
                lines_per_page: 22,
                chars_per_line: 28,
            },
            fonts: FontStyles {
                title: FontStyle::new("方正小标宋简体", 22.0),
                body: body.clone(),
                heading_l1: FontStyle::new("黑体", 16.0),
                heading_l2: FontStyle::new("楷体_GB2312", 16.0),
                heading_l3: body.clone(),
                heading_l4: body.clone(),
                document_number: body,
            },
            header: HeaderStyle {
                organization_mark_color: "#C8362B".to_owned(),
                separator_line: true,
            },
        }
    }
}

impl FontStyle {
    fn new(font: &str, size_pt: f64) -> Self {
        Self {
            font: font.to_owned(),
            size_pt,
        }
    }

    pub(crate) fn half_points(&self) -> u32 {
        rounded_u32(self.size_pt * 2.0)
    }
}

impl Gb9704Style {
    /// Parses and validates a knowledge-pack `style/*.toml` resource.
    ///
    /// # Errors
    ///
    /// Returns [`StyleError`] when the TOML is malformed or a parameter is out of range.
    pub fn from_toml(value: &str) -> Result<Self, StyleError> {
        let style = toml::from_str::<Self>(value).map_err(|_| StyleError::InvalidToml)?;
        style.validate()?;
        Ok(style)
    }

    /// Checks that every page, font and margin parameter is within the GB/T 9704 range.
    ///
    /// # Errors
    ///
    /// Returns [`StyleError`] when a parameter is out of range.
    pub fn validate(&self) -> Result<(), StyleError> {
        let page_values = [
            self.page.width_mm,
            self.page.height_mm,
            self.page.margin_top_mm,
            self.page.margin_left_mm,
            self.page.body_width_mm,
            self.page.body_height_mm,
            self.page.line_spacing_pt,
        ];
        let page_valid = self.page.size.eq_ignore_ascii_case("A4")
            && page_values
                .into_iter()
                .all(|value| value.is_finite() && value > 0.0 && value < 1000.0)
            && self.page.margin_left_mm + self.page.body_width_mm <= self.page.width_mm
            && self.page.margin_top_mm + self.page.body_height_mm <= self.page.height_mm
            && (1..=100).contains(&self.page.lines_per_page)
            && (1..=200).contains(&self.page.chars_per_line);
        let fonts = [
            &self.fonts.title,
            &self.fonts.body,
            &self.fonts.heading_l1,
            &self.fonts.heading_l2,
            &self.fonts.heading_l3,
            &self.fonts.heading_l4,
            &self.fonts.document_number,
        ];
        let fonts_valid = fonts.into_iter().all(|font| {
            !font.font.trim().is_empty()
                && font.font.len() <= 256
                && !font.font.chars().any(char::is_control)
                && font.size_pt.is_finite()
                && (5.0..=96.0).contains(&font.size_pt)
        });
        let color = self
            .header
            .organization_mark_color
            .strip_prefix('#')
            .unwrap_or(&self.header.organization_mark_color);
        let color_valid = color.len() == 6 && color.bytes().all(|byte| byte.is_ascii_hexdigit());
        if page_valid && fonts_valid && color_valid {
            Ok(())
        } else {
            Err(StyleError::InvalidValue)
        }
    }

    /// Applies user-selected system font substitutions without changing sizes or semantics.
    #[must_use]
    pub fn with_font_mappings(mut self, mappings: &BTreeMap<String, String>) -> Self {
        for font in [
            &mut self.fonts.title,
            &mut self.fonts.body,
            &mut self.fonts.heading_l1,
            &mut self.fonts.heading_l2,
            &mut self.fonts.heading_l3,
            &mut self.fonts.heading_l4,
            &mut self.fonts.document_number,
        ] {
            if let Some(replacement) = mappings
                .get(&font.font)
                .filter(|value| !value.trim().is_empty())
            {
                font.font = replacement.trim().to_owned();
            }
        }
        self
    }

    pub(crate) fn page_width_twips(&self) -> u32 {
        mm_to_twips(self.page.width_mm)
    }

    pub(crate) fn page_height_twips(&self) -> u32 {
        mm_to_twips(self.page.height_mm)
    }

    pub(crate) fn margin_top_twips(&self) -> u32 {
        mm_to_twips(self.page.margin_top_mm)
    }

    pub(crate) fn margin_left_twips(&self) -> u32 {
        mm_to_twips(self.page.margin_left_mm)
    }

    pub(crate) fn margin_right_twips(&self) -> u32 {
        mm_to_twips(self.page.width_mm - self.page.margin_left_mm - self.page.body_width_mm)
    }

    pub(crate) fn margin_bottom_twips(&self) -> u32 {
        mm_to_twips(self.page.height_mm - self.page.margin_top_mm - self.page.body_height_mm)
    }

    pub(crate) fn line_spacing_twips(&self) -> u32 {
        rounded_u32(self.page.line_spacing_pt * 20.0)
    }

    pub(crate) fn mark_color(&self) -> &str {
        self.header
            .organization_mark_color
            .strip_prefix('#')
            .unwrap_or(&self.header.organization_mark_color)
    }
}

fn mm_to_twips(value: f64) -> u32 {
    rounded_u32(value * TWIPS_PER_MM)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the branches above bound the value to 0..=u32::MAX before the cast"
)]
fn rounded_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        value.round() as u32
    }
}
