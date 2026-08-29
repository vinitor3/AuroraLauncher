//! Identidade local do Aurora.
//!
//! O Firebase autentica a conta; este módulo apenas normaliza o nick e produz
//! o UUID determinístico que mantém a mesma identidade entre sessões locais.

mod uuid_gen;

pub use uuid_gen::{offline_uuid_for_nickname, validate_nickname, NicknameError};
