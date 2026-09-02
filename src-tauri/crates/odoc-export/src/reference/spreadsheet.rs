use super::super::ExportError;
use calamine::{Data, Range, Reader, Xls, Xlsx, open_workbook_from_rs};
use cfb::CompoundFile;
use std::io::{Cursor, Read as _};

/// calamine trusts the BIFF DIMENSIONS record and reserves `rows * cols` cells before reading
/// any cell data, so a crafted record can request an absurd allocation and abort the process.
/// Only hand a workbook stream to calamine when every declared dimension parses cleanly and
/// stays under this budget; anything else falls back to the bounded parser in `super::ole`.
const MAX_DIMENSION_CELLS: u64 = 4_000_000;

pub(super) fn extract_xlsx(bytes: &[u8]) -> Result<String, ExportError> {
    let reader = Cursor::new(bytes);
    let mut workbook: Xlsx<_> =
        open_workbook_from_rs(reader).map_err(|_| ExportError::InvalidReference)?;
    render_workbook(&mut workbook)
}

pub(super) fn extract_xls(bytes: &[u8]) -> Result<String, ExportError> {
    if !biff_dimensions_are_sane(bytes) {
        return Err(ExportError::InvalidReference);
    }
    let reader = Cursor::new(bytes);
    let mut workbook: Xls<_> =
        open_workbook_from_rs(reader).map_err(|_| ExportError::InvalidReference)?;
    render_workbook(&mut workbook)
}

/// Walks the BIFF record chain of the workbook stream and validates every DIMENSIONS record.
///
/// A stream whose records cannot be walked to the end is rejected, because its dimensions were
/// not all observed and calamine would still trust the unseen ones.
fn biff_dimensions_are_sane(bytes: &[u8]) -> bool {
    let Ok(mut compound) = CompoundFile::open(Cursor::new(bytes)) else {
        return false;
    };
    let Some(path) = compound
        .walk()
        .filter(cfb::Entry::is_stream)
        .find_map(|entry| {
            ["Workbook", "Book"]
                .iter()
                .any(|name| entry.name().eq_ignore_ascii_case(name))
                .then(|| entry.path().to_path_buf())
        })
    else {
        return false;
    };
    let Ok(stream) = compound.open_stream(&path) else {
        return false;
    };
    let mut stream = std::io::BufReader::new(stream);
    let mut buffer = Vec::new();
    if stream.read_to_end(&mut buffer).is_err() {
        return false;
    }
    let mut offset = 0;
    while offset + 4 <= buffer.len() {
        let record = u16::from_le_bytes([buffer[offset], buffer[offset + 1]]);
        let length = (u16::from_le_bytes([buffer[offset + 2], buffer[offset + 3]])) as usize;
        offset += 4;
        let Some(end) = offset.checked_add(length) else {
            return false;
        };
        if end > buffer.len() {
            return false;
        }
        let data = &buffer[offset..end];
        if record == 0x0200 {
            // BIFF8 stores the used range as u32 rows + u16 columns (14 bytes); BIFF2-7 use
            // u16 for both (10 bytes).
            let (rows, columns) = match data.len() {
                14 => (
                    u64::from(
                        u32::from_le_bytes(data[4..8].try_into().unwrap_or([0; 4])).saturating_sub(
                            u32::from_le_bytes(data[0..4].try_into().unwrap_or([0; 4])),
                        ),
                    )
                    .saturating_add(1),
                    u64::from(
                        u16::from_le_bytes(data[10..12].try_into().unwrap_or([0; 2]))
                            .saturating_sub(u16::from_le_bytes(
                                data[8..10].try_into().unwrap_or([0; 2]),
                            )),
                    )
                    .saturating_add(1),
                ),
                10 => (
                    u64::from(
                        u16::from_le_bytes(data[2..4].try_into().unwrap_or([0; 2])).saturating_sub(
                            u16::from_le_bytes(data[0..2].try_into().unwrap_or([0; 2])),
                        ),
                    )
                    .saturating_add(1),
                    u64::from(
                        u16::from_le_bytes(data[6..8].try_into().unwrap_or([0; 2])).saturating_sub(
                            u16::from_le_bytes(data[4..6].try_into().unwrap_or([0; 2])),
                        ),
                    )
                    .saturating_add(1),
                ),
                _ => return false,
            };
            if rows.saturating_mul(columns) > MAX_DIMENSION_CELLS {
                return false;
            }
        }
        offset = end;
    }
    offset == buffer.len()
}

fn render_workbook<RS, R>(workbook: &mut R) -> Result<String, ExportError>
where
    RS: std::io::Read + std::io::Seek,
    R: Reader<RS>,
{
    let worksheets = workbook.worksheets();
    if worksheets.is_empty() {
        return Err(ExportError::InvalidReference);
    }
    let show_sheet_names = worksheets.len() > 1;
    let mut output = String::new();
    for (sheet_name, range) in worksheets {
        let sheet = render_range(&range);
        if sheet.trim().is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        if show_sheet_names {
            output.push_str("【工作表：");
            output.push_str(sheet_name.trim());
            output.push_str("】\n");
        }
        output.push_str(&sheet);
    }
    if output.trim().is_empty() {
        Err(ExportError::InvalidReference)
    } else {
        Ok(output)
    }
}

fn render_range(range: &Range<Data>) -> String {
    let mut output = String::new();
    for row in range.rows() {
        let mut values = row.iter().map(cell_text).collect::<Vec<_>>();
        while values.last().is_some_and(String::is_empty) {
            values.pop();
        }
        if values.is_empty() {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&values.join("\t"));
    }
    output
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => {
            value.trim().to_owned()
        }
        Data::Int(value) => value.to_string(),
        Data::Float(value) => {
            if value.is_finite() && value.fract().abs() < f64::EPSILON {
                format!("{value:.0}")
            } else {
                value.to_string()
            }
        }
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => value.to_string(),
        Data::Error(value) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_sparse_rows_as_tab_separated_text() {
        let range = Range::from_sparse(vec![
            calamine::Cell::new((0, 0), Data::String("事项".to_owned())),
            calamine::Cell::new((0, 2), Data::String("时限".to_owned())),
            calamine::Cell::new((1, 0), Data::String("验收".to_owned())),
            calamine::Cell::new((1, 2), Data::Int(2026)),
        ]);
        assert_eq!(render_range(&range), "事项\t\t时限\n验收\t\t2026");
    }
}
