use base64::{engine::general_purpose::STANDARD, Engine as _};
use edge_tts_rust::{Boundary, EdgeTtsClient, SpeakOptions};
use serde::Serialize;
use thiserror::Error;

const MAX_SPEECH_CHARACTERS: usize = 4_000;
const PORTUGUESE_VOICE: &str = "pt-BR-FranciscaNeural";

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("o texto para leitura está vazio")]
    Empty,
    #[error("a resposta é longa demais para leitura em voz alta")]
    TooLong,
    #[error("o Edge TTS retornou um áudio vazio ou inválido")]
    InvalidAudio,
    #[error("não foi possível gerar a voz do Aurora: {0}")]
    Synthesis(#[from] edge_tts_rust::Error),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechBoundary {
    pub offset_ms: u64,
    pub duration_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechResult {
    pub audio_base64: String,
    pub mime_type: &'static str,
    pub boundaries: Vec<SpeechBoundary>,
}

pub async fn synthesize_speech(text: &str) -> Result<SpeechResult, TtsError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(TtsError::Empty);
    }
    if text.chars().count() > MAX_SPEECH_CHARACTERS {
        return Err(TtsError::TooLong);
    }

    let client = EdgeTtsClient::new()?;
    let result = client
        .synthesize(
            text,
            SpeakOptions {
                voice: PORTUGUESE_VOICE.to_owned(),
                boundary: Boundary::Sentence,
                ..SpeakOptions::default()
            },
        )
        .await?;
    if !is_mp3(&result.audio) {
        return Err(TtsError::InvalidAudio);
    }

    Ok(SpeechResult {
        audio_base64: STANDARD.encode(result.audio),
        mime_type: "audio/mpeg",
        boundaries: result
            .boundaries
            .into_iter()
            .map(|boundary| SpeechBoundary {
                offset_ms: boundary.offset_ticks / 10_000,
                duration_ms: boundary.duration_ticks / 10_000,
                text: boundary.text,
            })
            .collect(),
    })
}

fn is_mp3(audio: &[u8]) -> bool {
    if audio.len() < 4 {
        return false;
    }
    if audio.starts_with(b"ID3") {
        return true;
    }
    audio.windows(3).take(4_096).any(|header| {
        let version = (header[1] >> 3) & 0b11;
        let layer = (header[1] >> 1) & 0b11;
        let bitrate = header[2] >> 4;
        let sample_rate = (header[2] >> 2) & 0b11;
        header[0] == 0xff
            && header[1] & 0xe0 == 0xe0
            && version != 0b01
            && layer != 0
            && !matches!(bitrate, 0 | 0x0f)
            && sample_rate != 0b11
    })
}

#[cfg(test)]
mod tests {
    use super::{is_mp3, synthesize_speech};

    #[test]
    fn rejects_non_mp3_payloads_before_sending_them_to_webview() {
        assert!(!is_mp3(b"<html>service unavailable</html>"));
        assert!(is_mp3(b"ID3\x04\x00\x00\x00\x00\x00\x00"));
        assert!(is_mp3(&[0xff, 0xfb, 0x90, 0x64]));
    }

    #[test]
    #[ignore = "faz uma chamada real ao serviço Edge Read Aloud"]
    fn synthesizes_portuguese_audio_with_sentence_boundaries() {
        let result = tauri::async_runtime::block_on(synthesize_speech(
            "Aurora online. O áudio está funcionando.",
        ))
        .expect("a síntese Edge TTS deve responder");
        assert!(result.audio_base64.len() > 1_000);
        assert_eq!(result.mime_type, "audio/mpeg");
        assert!(!result.boundaries.is_empty());
    }
}
