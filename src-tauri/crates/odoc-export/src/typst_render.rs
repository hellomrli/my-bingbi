//! Pure-Rust PDF rendering via the embedded Typst compiler.
//!
//! Compiles a Typst source string with the fonts registered by the caller into
//! a finished PDF, entirely in-process and with no external tool.

use std::path::PathBuf;

use typst::diag::{FileError, FileResult, SourceDiagnostic};
use typst::foundations::{Bytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};

use crate::ExportError;

/// A [`World`] that serves a single in-memory Typst source and a fixed set of
/// registered fonts.
///
/// [`World`]: typst::World
struct TypstWorld {
    /// The Typst standard library.
    library: LazyHash<Library>,
    /// The font book mapping family names to font indices.
    book: LazyHash<FontBook>,
    /// The main source file.
    source: Source,
    /// The registered fonts, indexed by the font book.
    fonts: Vec<Font>,
}

impl TypstWorld {
    /// Build a world from a source string and `(family, path)` font pairs.
    ///
    /// Each font file may contain a single face or a whole collection (`.ttc`);
    /// every face in a file is registered under the family name provided by the
    /// caller.
    fn new(source: &str, fonts: &[(String, PathBuf)]) -> Result<Self, ExportError> {
        let source = Source::new(
            FileId::new(None, VirtualPath::new("/main.typ")),
            source.to_owned(),
        );

        let mut book = FontBook::new();
        let mut loaded = Vec::new();
        for (family, path) in fonts {
            let data = std::fs::read(path)
                .map_err(|error| ExportError::FontRead(path.display().to_string(), error))?;
            let faces = Font::iter(Bytes::new(data)).collect::<Vec<_>>();
            if faces.is_empty() {
                return Err(ExportError::InvalidFont(path.display().to_string()));
            }
            // A `.ttc` collection can hold several faces (e.g. Noto CJK ships
            // JP/SC/TC/KR in one file). Register only the most suitable face so
            // that Simplified Chinese text is not rendered with the Japanese or
            // Traditional variant that happens to be listed first.
            let face = select_face(&faces);
            let mut info = face.info().clone();
            info.family.clone_from(family);
            book.push(info);
            loaded.push(face.clone());
        }

        Ok(Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            source,
            fonts: loaded,
        })
    }
}

/// Picks the most suitable face from a font file, preferring Simplified Chinese
/// (SC/CN/Hans) faces so that a CJK `.ttc` does not silently fall back to the
/// Japanese or Traditional variant listed first.
fn select_face(faces: &[Font]) -> &Font {
    if faces.len() <= 1 {
        return &faces[0];
    }
    for keyword in ["SC", "CN", "Hans", "Simplified"] {
        if let Some(face) = faces
            .iter()
            .find(|face| face.info().family.contains(keyword))
        {
            return face;
        }
    }
    &faces[0]
}

impl World for TypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().to_path_buf()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().as_rootless_path().to_path_buf()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

/// Compile a Typst source string into PDF bytes using the registered fonts.
///
/// The returned bytes are a complete PDF file. The caller is responsible for
/// providing enough fonts to cover every glyph in `source`; missing glyphs are
/// rendered as tofu by Typst rather than raising an error.
///
/// # Errors
///
/// Returns an error when a font file cannot be read or parsed, when Typst
/// compilation fails, or when the finished document cannot be serialized to
/// PDF.
pub fn render_typst_pdf(
    source: &str,
    fonts: &[(String, PathBuf)],
) -> Result<Vec<u8>, ExportError> {
    let world = TypstWorld::new(source, fonts)?;
    let document = match typst::compile::<PagedDocument>(&world).output {
        Ok(document) => document,
        Err(diagnostics) => {
            return Err(ExportError::TypstCompile(format_diagnostics(&diagnostics)));
        }
    };
    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|diagnostics| ExportError::TypstPdfExport(format_diagnostics(&diagnostics)))
}

/// Join diagnostics into a single human-readable message.
fn format_diagnostics(diagnostics: &[SourceDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let mut message = diagnostic.message.to_string();
            for hint in &diagnostic.hints {
                message.push_str("; hint: ");
                message.push_str(hint);
            }
            message
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect the CJK system fonts that exist on this machine.
    fn available_cjk_fonts() -> Vec<(String, PathBuf)> {
        [
            (
                "Noto Serif CJK SC",
                "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
            ),
            (
                "Noto Sans CJK SC",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            ),
            ("AR PL UKai CN", "/usr/share/fonts/truetype/arphic/ukai.ttc"),
        ]
        .into_iter()
        .filter(|(_, path)| std::path::Path::new(path).exists())
        .map(|(family, path)| (family.to_owned(), PathBuf::from(path)))
        .collect()
    }

    #[test]
    fn renders_a_chinese_pdf_with_system_fonts() {
        let fonts = available_cjk_fonts();
        let Some((family, _)) = fonts.first() else {
            eprintln!("skipping: no system CJK fonts are available");
            return;
        };

        let source = format!(
            "#set page(width: 21cm, height: 29.7cm)\n\
             #set text(font: \"{family}\", size: 12pt)\n\
             = 关于测试\n\n\
             这是一段中文正文，用于验证 Typst 渲染。\n"
        );

        let pdf = render_typst_pdf(&source, &fonts).expect("a PDF should be generated");
        assert!(!pdf.is_empty(), "PDF bytes should not be empty");
        assert!(pdf.starts_with(b"%PDF"), "PDF should start with the `%PDF` magic");
    }
}
