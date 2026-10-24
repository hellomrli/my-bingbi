use crate::ExportError;
use quick_xml::events::Event;

/// Decode one text fragment without expanding DTDs or resolving external entities.
pub(crate) fn decode_text(event: &Event<'_>) -> Result<String, ExportError> {
    match event {
        Event::Text(text) => text.decode().map(|text| text.into_owned()).map_err(|_| ExportError::InvalidXml),
        Event::CData(text) => text.decode().map(|text| text.into_owned()).map_err(|_| ExportError::InvalidXml),
        Event::GeneralRef(reference) => {
            if let Some(character) = reference.resolve_char_ref().map_err(|_| ExportError::InvalidXml)? {
                return Ok(character.to_string());
            }
            let name = reference.decode().map_err(|_| ExportError::InvalidXml)?;
            quick_xml::escape::resolve_predefined_entity(&name)
                .map(str::to_owned)
                .ok_or(ExportError::InvalidXml)
        }
        _ => Err(ExportError::InvalidXml),
    }
}
