//! Read configured fields through the exact renderer grammar, without rendering.
use super::*;

pub(in super::super) fn inspect_public(bytes: &[u8]) -> Result<Option<(String, u16)>> {
    let text = check_text(bytes)?;
    let shape = inspect_shape(text)?;
    let document: Document = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
    // Validate all TOML fields as the renderer does, without changing any tree.
    let _: Value = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
    if let Some(public) = document.host.public {
        raw_public(text, &shape, &public)?;
        Ok(Some((
            public.ip.get_ref().clone(),
            *public.port.get_ref() as u16,
        )))
    } else if shape.public || shape.ip.is_some() || shape.port.is_some() {
        Err(RecoveryError::Integrity)
    } else {
        Ok(None)
    }
}
