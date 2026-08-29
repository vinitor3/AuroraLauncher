use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NicknameError {
    #[error("o nick deve ter de 3 a 16 caracteres e usar apenas letras, números ou _")]
    InvalidFormat,
}

/// Normaliza um nick no formato aceito pelo Aurora e pelo perfil offline do jogo.
pub fn validate_nickname(nickname: &str) -> Result<String, NicknameError> {
    let nickname = nickname.trim();
    let valid = (3..=16).contains(&nickname.len())
        && nickname
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if valid {
        Ok(nickname.to_ascii_lowercase())
    } else {
        Err(NicknameError::InvalidFormat)
    }
}

/// UUID v3 compatível com a convenção `OfflinePlayer:<nick>`.
pub fn offline_uuid_for_nickname(nickname: &str) -> Result<Uuid, NicknameError> {
    let normalized = validate_nickname(nickname)?;
    Ok(Uuid::new_v3(
        &Uuid::NAMESPACE_URL,
        format!("OfflinePlayer:{normalized}").as_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_is_case_insensitive_and_deterministic() {
        let first = offline_uuid_for_nickname("Aurora_Player").unwrap();
        let second = offline_uuid_for_nickname("aurora_player").unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn nicknames_are_limited_to_the_safe_profile_format() {
        assert!(validate_nickname("Jo").is_err());
        assert!(validate_nickname("aurora-player").is_err());
        assert_eq!(validate_nickname("Aurora_01").unwrap(), "aurora_01");
    }
}
