//! Safe, dependency-light text extraction for files used as drafting references.

mod ole;
mod pdf;
mod spreadsheet;
mod zip_xml;

use super::{ExportError, read_docx};

const MAX_REFERENCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_REFERENCE_TEXT_BYTES: usize = 2 * 1024 * 1024;
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const OLE_MAGIC: &[u8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";

/// Extracts readable text from a reference file without importing it as an editable document.
///
/// The file name selects the expected container while a matching file signature is still
/// required for binary formats. This prevents a renamed binary from being handed to the wrong
/// parser. Every parser operates in memory with bounded input, decompressed-part and output sizes.
///
/// # Errors
///
/// Returns [`ExportError::UnsupportedReferenceFormat`] for an unknown extension,
/// [`ExportError::PackageTooLarge`] for an empty/oversized input or extracted result, and
/// [`ExportError::InvalidReference`] when the declared format cannot be safely read.
pub fn extract_reference_text(file_name: &str, bytes: &[u8]) -> Result<String, ExportError> {
    if bytes.is_empty() || bytes.len() > MAX_REFERENCE_BYTES {
        return Err(ExportError::PackageTooLarge);
    }
    let extension = file_name
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension)
        .to_ascii_lowercase();
    let text = match extension.as_str() {
        "docx" => {
            require_magic(bytes, ZIP_MAGIC)?;
            read_docx(bytes)?.to_plain_text()
        }
        "pdf" => {
            require_pdf_magic(bytes)?;
            pdf::extract(bytes)?
        }
        "ofd" => {
            require_magic(bytes, ZIP_MAGIC)?;
            zip_xml::extract_ofd(bytes)?
        }
        "xlsx" => {
            require_magic(bytes, ZIP_MAGIC)?;
            spreadsheet::extract_xlsx(bytes).or_else(|_| zip_xml::extract_xlsx(bytes))?
        }
        "xls" => {
            require_magic(bytes, OLE_MAGIC)?;
            spreadsheet::extract_xls(bytes).or_else(|_| ole::extract_xls(bytes))?
        }
        "doc" => {
            require_magic(bytes, OLE_MAGIC)?;
            ole::extract_doc(bytes)?
        }
        "wps" => {
            if bytes.starts_with(ZIP_MAGIC) {
                zip_xml::extract_wps(bytes)?
            } else if bytes.starts_with(OLE_MAGIC) {
                ole::extract_wps(bytes)?
            } else {
                return Err(ExportError::InvalidReference);
            }
        }
        _ => return Err(ExportError::UnsupportedReferenceFormat),
    };
    normalize_and_validate(&text)
}

fn require_magic(bytes: &[u8], magic: &[u8]) -> Result<(), ExportError> {
    if bytes.starts_with(magic) {
        Ok(())
    } else {
        Err(ExportError::InvalidReference)
    }
}

fn require_pdf_magic(bytes: &[u8]) -> Result<(), ExportError> {
    let header = &bytes[..bytes.len().min(1024)];
    if header
        .windows(b"%PDF-".len())
        .any(|window| window == b"%PDF-")
    {
        Ok(())
    } else {
        Err(ExportError::InvalidReference)
    }
}

fn normalize_and_validate(text: &str) -> Result<String, ExportError> {
    let mut normalized = String::with_capacity(text.len().min(MAX_REFERENCE_TEXT_BYTES));
    let mut previous_newline = false;
    for character in text.replace("\r\n", "\n").replace('\r', "\n").chars() {
        let character = match character {
            '\u{0000}'..='\u{0008}'
            | '\u{000b}'
            | '\u{000c}'
            | '\u{000e}'..='\u{001f}'
            | '\u{007f}' => ' ',
            value => value,
        };
        if character == '\n' {
            if previous_newline {
                continue;
            }
            previous_newline = true;
        } else if !character.is_whitespace() || character == '\t' {
            previous_newline = false;
        }
        normalized.push(character);
        if normalized.len() > MAX_REFERENCE_TEXT_BYTES {
            return Err(ExportError::PackageTooLarge);
        }
    }
    let normalized = normalized.trim().to_owned();
    if normalized.is_empty() {
        Err(ExportError::InvalidReference)
    } else {
        Ok(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    #[test]
    fn rejects_unknown_or_renamed_reference_files() {
        assert!(matches!(
            extract_reference_text("note.bin", b"hello"),
            Err(ExportError::UnsupportedReferenceFormat)
        ));
        assert!(matches!(
            extract_reference_text("note.pdf", b"not a pdf"),
            Err(ExportError::InvalidReference)
        ));
    }

    #[test]
    fn normalizes_unsafe_control_characters() {
        assert_eq!(
            normalize_and_validate("正文\0内容\r\n下一行").unwrap(),
            "正文 内容\n下一行"
        );
    }

    #[test]
    fn extracts_deflated_ofd_text() {
        let bytes = zip_package(&[(
            "OFD.xml",
            br#"<ofd:OFD xmlns:ofd="urn:ofd"><ofd:DocBody><ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot></ofd:DocBody></ofd:OFD>"#,
        ), (
            "Doc_0/Pages/Page_0/Content.xml",
            b"<ofd:Content xmlns:ofd=\"urn:ofd\"><ofd:TextObject><ofd:TextCode>\xe9\xa1\xb9\xe7\x9b\xae\xe5\xb7\xb2\xe5\xae\x8c\xe6\x88\x90</ofd:TextCode></ofd:TextObject></ofd:Content>",
        )]);
        assert_eq!(
            extract_reference_text("材料.ofd", &bytes).unwrap(),
            "项目已完成"
        );
    }

    #[test]
    fn extracts_xlsx_shared_strings() {
        let bytes = zip_package(&[
            (
                "xl/workbook.xml",
                br#"<workbook xmlns:r="r"><sheets><sheet name="Sheet1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                b"<sst><si><t>\xe6\x95\xb4\xe6\x94\xb9\xe4\xba\x8b\xe9\xa1\xb9</t></si><si><r><t>\xe6\x9c\x88\xe5\xba\x95\xe5\xae\x8c\xe6\x88\x90</t></r></si></sst>",
            ),
            (
                "xl/worksheets/sheet1.xml",
                br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let text = extract_reference_text("表格.xlsx", &bytes).unwrap();
        assert_eq!(text, "整改事项\t月底完成");
    }

    #[test]
    fn extracts_deflated_docx_text() {
        let bytes = zip_package(&[(
            "word/document.xml",
            b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>\xe4\xbc\x9a\xe8\xae\xae\xe5\x86\xb3\xe5\xae\x9a</w:t></w:r></w:p></w:body></w:document>",
        )]);
        assert_eq!(
            extract_reference_text("会议.docx", &bytes).unwrap(),
            "会议决定"
        );
    }

    #[test]
    fn extracts_wps_zip_text() {
        let bytes = zip_package(&[(
            "wps/document.xml",
            b"<document><body><p><span>WPS \xe4\xbc\x9a\xe8\xae\xae\xe6\x9d\x90\xe6\x96\x99</span></p></body></document>",
        )]);
        assert_eq!(
            extract_reference_text("会议.wps", &bytes).unwrap(),
            "WPS 会议材料"
        );
    }

    fn zip_package(parts: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = ZipWriter::new(&mut output);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (name, contents) in parts {
                archive.start_file(*name, options).unwrap();
                archive.write_all(contents).unwrap();
            }
            archive.finish().unwrap();
        }
        output.into_inner()
    }
}
