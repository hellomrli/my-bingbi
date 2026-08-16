use super::super::ExportError;
use calamine::{Data, Range, Reader, Xls, Xlsx, open_workbook_from_rs};
use std::io::Cursor;

pub(super) fn extract_xlsx(bytes: &[u8]) -> Result<String, ExportError> {
    let reader = Cursor::new(bytes);
    let mut workbook: Xlsx<_> =
        open_workbook_from_rs(reader).map_err(|_| ExportError::InvalidReference)?;
    render_workbook(&mut workbook)
}

pub(super) fn extract_xls(bytes: &[u8]) -> Result<String, ExportError> {
    let reader = Cursor::new(bytes);
    let mut workbook: Xls<_> =
        open_workbook_from_rs(reader).map_err(|_| ExportError::InvalidReference)?;
    render_workbook(&mut workbook)
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
