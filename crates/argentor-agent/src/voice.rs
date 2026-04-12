//! Voice support — Speech-to-Text (STT) and Text-to-Speech (TTS).
//!
//! This module provides a non-invasive, additive layer over the existing
//! agent pipeline. Regular chat flows continue to use
//! [`argentor_core::Message`] unchanged, while voice-capable backends
//! implement the [`SttBackend`] and [`TtsBackend`] traits.
//!
//! # Quick start
//!
//! ```ignore
//! use argentor_agent::voice::{AudioFormat, AudioInput, TranscriptionRequest, VoiceConfig};
//!
//! let req = TranscriptionRequest {
//!     audio: AudioInput::FilePath("/tmp/hello.wav".into()),
//!     language: Some("en".into()),
//!     prompt: None,
//! };
//!
//! let cfg = VoiceConfig::default();
//! assert_eq!(cfg.speed, 1.0);
//! assert_eq!(cfg.format, AudioFormat::Mp3);
//! ```
//!
//! # Providers
//!
//! See [`crate::voice_backends`] for Whisper, Deepgram, OpenAI TTS, and
//! ElevenLabs implementations.

use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// AudioFormat
// ---------------------------------------------------------------------------

/// Audio container / codec format for voice input and output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    /// Waveform audio (`audio/wav`).
    Wav,
    /// MPEG-1 Audio Layer III (`audio/mpeg`).
    Mp3,
    /// Ogg container, typically with Vorbis or Opus (`audio/ogg`).
    Ogg,
    /// Free Lossless Audio Codec (`audio/flac`).
    Flac,
    /// WebM container, typically with Opus (`audio/webm`).
    Webm,
    /// MPEG-4 Audio (`audio/mp4`).
    M4a,
}

impl AudioFormat {
    /// IANA media type for this format.
    pub fn mime_type(&self) -> &str {
        match self {
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::Ogg => "audio/ogg",
            AudioFormat::Flac => "audio/flac",
            AudioFormat::Webm => "audio/webm",
            AudioFormat::M4a => "audio/mp4",
        }
    }

    /// Canonical file extension (without leading dot).
    pub fn extension(&self) -> &str {
        match self {
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Flac => "flac",
            AudioFormat::Webm => "webm",
            AudioFormat::M4a => "m4a",
        }
    }

    /// Infer an [`AudioFormat`] from a file path's extension.
    ///
    /// Returns `None` if the extension is missing or not recognized.
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| match e.to_ascii_lowercase().as_str() {
                "wav" => Some(AudioFormat::Wav),
                "mp3" => Some(AudioFormat::Mp3),
                "ogg" | "oga" => Some(AudioFormat::Ogg),
                "flac" => Some(AudioFormat::Flac),
                "webm" => Some(AudioFormat::Webm),
                "m4a" | "mp4" => Some(AudioFormat::M4a),
                _ => None,
            })
    }
}

// ---------------------------------------------------------------------------
// AudioInput
// ---------------------------------------------------------------------------

/// An audio input — inline base64, a local file, or a URL the backend can
/// download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AudioInput {
    /// Base64-encoded audio bytes plus the declared [`AudioFormat`].
    Base64 {
        /// Audio container / codec.
        format: AudioFormat,
        /// Base64-encoded payload (no `data:` prefix).
        data: String,
    },
    /// Local filesystem path. Format is inferred from the extension.
    FilePath(String),
    /// Publicly reachable HTTP(S) URL the provider will fetch.
    Url(String),
}

impl AudioInput {
    /// Returns `true` when this input is a [`AudioInput::Base64`] variant.
    pub fn is_base64(&self) -> bool {
        matches!(self, AudioInput::Base64 { .. })
    }

    /// Returns `true` when this input is a [`AudioInput::FilePath`] variant.
    pub fn is_file(&self) -> bool {
        matches!(self, AudioInput::FilePath(_))
    }

    /// Returns `true` when this input is a [`AudioInput::Url`] variant.
    pub fn is_url(&self) -> bool {
        matches!(self, AudioInput::Url(_))
    }

    /// Inline format if explicitly known.
    ///
    /// For [`AudioInput::Base64`] the declared format is returned. For
    /// [`AudioInput::FilePath`] the extension is inspected. URLs always
    /// return `None`.
    pub fn format_hint(&self) -> Option<AudioFormat> {
        match self {
            AudioInput::Base64 { format, .. } => Some(*format),
            AudioInput::FilePath(p) => AudioFormat::from_path(p),
            AudioInput::Url(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// VoiceConfig
// ---------------------------------------------------------------------------

/// Configuration for a TTS synthesis request.
///
/// `voice_id` is provider-specific (e.g. `"alloy"` for OpenAI TTS, a UUID
/// for ElevenLabs). Defaults target sane, neutral output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// Provider-specific voice identifier.
    pub voice_id: String,
    /// ISO 639 / BCP-47 language tag (e.g. `"en"`, `"es"`, `"es-AR"`).
    pub language: String,
    /// Playback speed multiplier. Clamped to `[0.5, 2.0]` by backends.
    pub speed: f32,
    /// Pitch adjustment in the range `[-1.0, 1.0]`. `0.0` is neutral.
    pub pitch: f32,
    /// Desired output [`AudioFormat`].
    pub format: AudioFormat,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            voice_id: "default".to_string(),
            language: "en".to_string(),
            speed: 1.0,
            pitch: 0.0,
            format: AudioFormat::Mp3,
        }
    }
}

impl VoiceConfig {
    /// Build a new config with the given voice ID, defaulting the other
    /// fields.
    pub fn new(voice_id: impl Into<String>) -> Self {
        Self {
            voice_id: voice_id.into(),
            ..Self::default()
        }
    }

    /// Builder — set the language.
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Builder — set the speed (clamped to `[0.5, 2.0]`).
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed.clamp(0.5, 2.0);
        self
    }

    /// Builder — set the pitch (clamped to `[-1.0, 1.0]`).
    pub fn with_pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch.clamp(-1.0, 1.0);
        self
    }

    /// Builder — set the output format.
    pub fn with_format(mut self, format: AudioFormat) -> Self {
        self.format = format;
        self
    }

    /// Returns `true` when `speed`, `pitch`, and `language` fall within
    /// their accepted ranges.
    pub fn is_valid(&self) -> bool {
        (0.5..=2.0).contains(&self.speed)
            && (-1.0..=1.0).contains(&self.pitch)
            && !self.language.is_empty()
            && !self.voice_id.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Transcription types
// ---------------------------------------------------------------------------

/// An STT (transcription) request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionRequest {
    /// Audio payload to transcribe.
    pub audio: AudioInput,
    /// Optional language hint (ISO code). When `None`, the backend
    /// auto-detects.
    pub language: Option<String>,
    /// Optional domain-specific prompt to bias vocabulary.
    pub prompt: Option<String>,
}

impl TranscriptionRequest {
    /// Convenience constructor with only the audio input.
    pub fn new(audio: AudioInput) -> Self {
        Self {
            audio,
            language: None,
            prompt: None,
        }
    }

    /// Builder — set a language hint.
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    /// Builder — set a biasing prompt.
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }
}

/// Result of a transcription call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// Full transcript concatenating all segments.
    pub text: String,
    /// Auto-detected language, if the backend reports one.
    pub language_detected: Option<String>,
    /// Total audio duration in seconds.
    pub duration_seconds: Option<f64>,
    /// Per-segment breakdown. May be empty if the backend doesn't return
    /// timestamps.
    #[serde(default)]
    pub segments: Vec<TranscriptSegment>,
}

impl TranscriptionResult {
    /// Minimal result containing only the transcript text.
    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            language_detected: None,
            duration_seconds: None,
            segments: Vec::new(),
        }
    }

    /// `true` when the transcript has at least one segment.
    pub fn has_segments(&self) -> bool {
        !self.segments.is_empty()
    }
}

/// One time-aligned segment of a transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Segment text.
    pub text: String,
    /// Start offset from the beginning of the audio, in seconds.
    pub start_seconds: f64,
    /// End offset from the beginning of the audio, in seconds.
    pub end_seconds: f64,
    /// Optional confidence score in `[0.0, 1.0]`.
    pub confidence: Option<f32>,
}

impl TranscriptSegment {
    /// Duration of this segment in seconds.
    pub fn duration_seconds(&self) -> f64 {
        (self.end_seconds - self.start_seconds).max(0.0)
    }
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Trait for Speech-to-Text backends.
///
/// Implementations wrap a provider-specific client (Whisper, Deepgram,
/// etc.) and translate a [`TranscriptionRequest`] into provider-specific
/// wire format.
#[async_trait::async_trait]
pub trait SttBackend: Send + Sync {
    /// Short provider identifier used for routing (e.g. `"openai-whisper"`,
    /// `"deepgram"`).
    fn provider_name(&self) -> &str;

    /// Audio formats this backend can accept.
    fn supported_formats(&self) -> Vec<AudioFormat>;

    /// Transcribe the request into text (plus optional metadata).
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> argentor_core::ArgentorResult<TranscriptionResult>;
}

/// Trait for Text-to-Speech backends.
#[async_trait::async_trait]
pub trait TtsBackend: Send + Sync {
    /// Short provider identifier used for routing (e.g. `"openai-tts"`,
    /// `"elevenlabs"`).
    fn provider_name(&self) -> &str;

    /// Provider-specific voice IDs this backend exposes.
    fn available_voices(&self) -> Vec<String>;

    /// Synthesize `text` using `config` and return the audio bytes in
    /// [`VoiceConfig::format`].
    async fn synthesize(
        &self,
        text: &str,
        config: &VoiceConfig,
    ) -> argentor_core::ArgentorResult<Vec<u8>>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- AudioFormat ----

    #[test]
    fn audio_format_mime_wav() {
        assert_eq!(AudioFormat::Wav.mime_type(), "audio/wav");
    }

    #[test]
    fn audio_format_mime_mp3() {
        assert_eq!(AudioFormat::Mp3.mime_type(), "audio/mpeg");
    }

    #[test]
    fn audio_format_mime_ogg() {
        assert_eq!(AudioFormat::Ogg.mime_type(), "audio/ogg");
    }

    #[test]
    fn audio_format_mime_flac() {
        assert_eq!(AudioFormat::Flac.mime_type(), "audio/flac");
    }

    #[test]
    fn audio_format_mime_webm() {
        assert_eq!(AudioFormat::Webm.mime_type(), "audio/webm");
    }

    #[test]
    fn audio_format_mime_m4a() {
        assert_eq!(AudioFormat::M4a.mime_type(), "audio/mp4");
    }

    #[test]
    fn audio_format_extensions() {
        assert_eq!(AudioFormat::Wav.extension(), "wav");
        assert_eq!(AudioFormat::Mp3.extension(), "mp3");
        assert_eq!(AudioFormat::Ogg.extension(), "ogg");
        assert_eq!(AudioFormat::Flac.extension(), "flac");
        assert_eq!(AudioFormat::Webm.extension(), "webm");
        assert_eq!(AudioFormat::M4a.extension(), "m4a");
    }

    #[test]
    fn audio_format_from_path_wav() {
        assert_eq!(AudioFormat::from_path("a.wav"), Some(AudioFormat::Wav));
    }

    #[test]
    fn audio_format_from_path_mp3_uppercase() {
        assert_eq!(AudioFormat::from_path("a.MP3"), Some(AudioFormat::Mp3));
    }

    #[test]
    fn audio_format_from_path_flac() {
        assert_eq!(AudioFormat::from_path("x.flac"), Some(AudioFormat::Flac));
    }

    #[test]
    fn audio_format_from_path_m4a_and_mp4() {
        assert_eq!(AudioFormat::from_path("x.m4a"), Some(AudioFormat::M4a));
        assert_eq!(AudioFormat::from_path("x.mp4"), Some(AudioFormat::M4a));
    }

    #[test]
    fn audio_format_from_path_oga_alias() {
        assert_eq!(AudioFormat::from_path("x.oga"), Some(AudioFormat::Ogg));
    }

    #[test]
    fn audio_format_from_path_unknown_is_none() {
        assert_eq!(AudioFormat::from_path("x.txt"), None);
        assert_eq!(AudioFormat::from_path("noext"), None);
    }

    #[test]
    fn audio_format_serde_roundtrip() {
        for fmt in [
            AudioFormat::Wav,
            AudioFormat::Mp3,
            AudioFormat::Ogg,
            AudioFormat::Flac,
            AudioFormat::Webm,
            AudioFormat::M4a,
        ] {
            let s = serde_json::to_string(&fmt).unwrap();
            let back: AudioFormat = serde_json::from_str(&s).unwrap();
            assert_eq!(fmt, back);
        }
    }

    #[test]
    fn audio_format_equality() {
        assert_eq!(AudioFormat::Mp3, AudioFormat::Mp3);
        assert_ne!(AudioFormat::Mp3, AudioFormat::Wav);
    }

    // ---- AudioInput ----

    #[test]
    fn audio_input_base64_is_base64() {
        let a = AudioInput::Base64 {
            format: AudioFormat::Wav,
            data: "AAAA".into(),
        };
        assert!(a.is_base64());
        assert!(!a.is_file());
        assert!(!a.is_url());
    }

    #[test]
    fn audio_input_file_is_file() {
        let a = AudioInput::FilePath("/tmp/x.wav".into());
        assert!(a.is_file());
        assert!(!a.is_base64());
        assert!(!a.is_url());
    }

    #[test]
    fn audio_input_url_is_url() {
        let a = AudioInput::Url("https://x/a.mp3".into());
        assert!(a.is_url());
        assert!(!a.is_base64());
        assert!(!a.is_file());
    }

    #[test]
    fn audio_input_format_hint_base64() {
        let a = AudioInput::Base64 {
            format: AudioFormat::Flac,
            data: "AA".into(),
        };
        assert_eq!(a.format_hint(), Some(AudioFormat::Flac));
    }

    #[test]
    fn audio_input_format_hint_file() {
        let a = AudioInput::FilePath("/tmp/hello.ogg".into());
        assert_eq!(a.format_hint(), Some(AudioFormat::Ogg));
    }

    #[test]
    fn audio_input_format_hint_url_is_none() {
        let a = AudioInput::Url("https://x/a".into());
        assert_eq!(a.format_hint(), None);
    }

    #[test]
    fn audio_input_serde_base64_roundtrip() {
        let a = AudioInput::Base64 {
            format: AudioFormat::Mp3,
            data: "ZZZ".into(),
        };
        let s = serde_json::to_string(&a).unwrap();
        let back: AudioInput = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn audio_input_serde_file_roundtrip() {
        let a = AudioInput::FilePath("/tmp/a.wav".into());
        let s = serde_json::to_string(&a).unwrap();
        let back: AudioInput = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn audio_input_serde_url_roundtrip() {
        let a = AudioInput::Url("https://x/a.mp3".into());
        let s = serde_json::to_string(&a).unwrap();
        let back: AudioInput = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }

    // ---- VoiceConfig ----

    #[test]
    fn voice_config_default_values() {
        let c = VoiceConfig::default();
        assert_eq!(c.voice_id, "default");
        assert_eq!(c.language, "en");
        assert_eq!(c.speed, 1.0);
        assert_eq!(c.pitch, 0.0);
        assert_eq!(c.format, AudioFormat::Mp3);
    }

    #[test]
    fn voice_config_new_sets_voice_id() {
        let c = VoiceConfig::new("alloy");
        assert_eq!(c.voice_id, "alloy");
        assert_eq!(c.speed, 1.0);
    }

    #[test]
    fn voice_config_with_language() {
        let c = VoiceConfig::new("alloy").with_language("es-AR");
        assert_eq!(c.language, "es-AR");
    }

    #[test]
    fn voice_config_with_speed_clamps_low() {
        let c = VoiceConfig::default().with_speed(0.1);
        assert_eq!(c.speed, 0.5);
    }

    #[test]
    fn voice_config_with_speed_clamps_high() {
        let c = VoiceConfig::default().with_speed(5.0);
        assert_eq!(c.speed, 2.0);
    }

    #[test]
    fn voice_config_with_pitch_clamps() {
        let low = VoiceConfig::default().with_pitch(-4.0);
        let high = VoiceConfig::default().with_pitch(4.0);
        assert_eq!(low.pitch, -1.0);
        assert_eq!(high.pitch, 1.0);
    }

    #[test]
    fn voice_config_with_format() {
        let c = VoiceConfig::default().with_format(AudioFormat::Wav);
        assert_eq!(c.format, AudioFormat::Wav);
    }

    #[test]
    fn voice_config_valid_defaults() {
        assert!(VoiceConfig::default().is_valid());
    }

    #[test]
    fn voice_config_invalid_empty_voice() {
        let mut c = VoiceConfig::default();
        c.voice_id.clear();
        assert!(!c.is_valid());
    }

    #[test]
    fn voice_config_invalid_empty_language() {
        let mut c = VoiceConfig::default();
        c.language.clear();
        assert!(!c.is_valid());
    }

    #[test]
    fn voice_config_serde_roundtrip() {
        let c = VoiceConfig::new("nova")
            .with_language("es")
            .with_speed(1.2)
            .with_pitch(-0.2)
            .with_format(AudioFormat::Wav);
        let s = serde_json::to_string(&c).unwrap();
        let back: VoiceConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    // ---- TranscriptionRequest ----

    #[test]
    fn transcription_request_new_defaults() {
        let r = TranscriptionRequest::new(AudioInput::FilePath("/tmp/a.wav".into()));
        assert!(r.language.is_none());
        assert!(r.prompt.is_none());
        assert!(r.audio.is_file());
    }

    #[test]
    fn transcription_request_builder() {
        let r = TranscriptionRequest::new(AudioInput::Url("https://x".into()))
            .with_language("en")
            .with_prompt("Medical vocabulary.");
        assert_eq!(r.language.as_deref(), Some("en"));
        assert_eq!(r.prompt.as_deref(), Some("Medical vocabulary."));
    }

    #[test]
    fn transcription_request_serde_roundtrip() {
        let r = TranscriptionRequest::new(AudioInput::Base64 {
            format: AudioFormat::Mp3,
            data: "AA".into(),
        })
        .with_language("es");
        let s = serde_json::to_string(&r).unwrap();
        let back: TranscriptionRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    // ---- TranscriptionResult / TranscriptSegment ----

    #[test]
    fn transcription_result_text_only() {
        let r = TranscriptionResult::text_only("hola");
        assert_eq!(r.text, "hola");
        assert!(!r.has_segments());
        assert!(r.language_detected.is_none());
        assert!(r.duration_seconds.is_none());
    }

    #[test]
    fn transcription_result_has_segments() {
        let r = TranscriptionResult {
            text: "x".into(),
            language_detected: None,
            duration_seconds: None,
            segments: vec![TranscriptSegment {
                text: "x".into(),
                start_seconds: 0.0,
                end_seconds: 1.0,
                confidence: Some(0.9),
            }],
        };
        assert!(r.has_segments());
    }

    #[test]
    fn transcript_segment_duration() {
        let s = TranscriptSegment {
            text: "x".into(),
            start_seconds: 1.5,
            end_seconds: 3.0,
            confidence: None,
        };
        assert!((s.duration_seconds() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn transcript_segment_duration_never_negative() {
        let s = TranscriptSegment {
            text: "x".into(),
            start_seconds: 5.0,
            end_seconds: 2.0,
            confidence: None,
        };
        assert_eq!(s.duration_seconds(), 0.0);
    }

    #[test]
    fn transcription_result_serde_roundtrip() {
        let r = TranscriptionResult {
            text: "hello world".into(),
            language_detected: Some("en".into()),
            duration_seconds: Some(2.5),
            segments: vec![
                TranscriptSegment {
                    text: "hello".into(),
                    start_seconds: 0.0,
                    end_seconds: 1.0,
                    confidence: Some(0.95),
                },
                TranscriptSegment {
                    text: "world".into(),
                    start_seconds: 1.1,
                    end_seconds: 2.0,
                    confidence: None,
                },
            ],
        };
        let s = serde_json::to_string(&r).unwrap();
        let back: TranscriptionResult = serde_json::from_str(&s).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn transcript_segment_serde_roundtrip() {
        let s = TranscriptSegment {
            text: "hi".into(),
            start_seconds: 0.0,
            end_seconds: 0.5,
            confidence: Some(0.8),
        };
        let js = serde_json::to_string(&s).unwrap();
        let back: TranscriptSegment = serde_json::from_str(&js).unwrap();
        assert_eq!(s, back);
    }

    // ---- Trait object safety (dyn) ----

    #[test]
    fn stt_backend_is_object_safe() {
        fn _take(_b: &dyn SttBackend) {}
    }

    #[test]
    fn tts_backend_is_object_safe() {
        fn _take(_b: &dyn TtsBackend) {}
    }
}
