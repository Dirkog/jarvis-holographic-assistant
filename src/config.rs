//! Конфигурация JARVIS

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JarvisConfig {
    pub default_language: String,
    pub supported_languages: Vec<String>,
    pub audio: AudioConfig,
    pub camera: CameraConfig,
    pub ui: UiConfig,
    pub gestures: GestureConfig,
    pub models: ModelPaths,
    pub voice: VoiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: usize,
    pub vad_threshold: f32,
    pub silence_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    pub device_index: usize,
    pub resolution: (u32, u32),
    pub fps: u32,
    pub auto_exposure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub window_width: u32,
    pub window_height: u32,
    pub transparency: f32,
    pub theme: String,
    pub fps_target: u32,
    pub enable_glow: bool,
    pub hologram_intensity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GestureConfig {
    pub enabled: bool,
    pub sensitivity: f32,
    pub smoothing: f32,
    pub min_confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPaths {
    pub vosk_russian: PathBuf,
    pub vosk_english: PathBuf,
    pub mediapipe_hands: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub speed: f32,
    pub pitch: f32,
    pub voice_id: String,
}

impl Default for JarvisConfig {
    fn default() -> Self {
        Self {
            default_language: "ru".to_string(),
            supported_languages: vec!["ru".to_string(), "en".to_string()],
            audio: AudioConfig {
                sample_rate: 16000,
                channels: 1,
                buffer_size: 1024,
                vad_threshold: 0.02,
                silence_timeout_ms: 1500,
            },
            camera: CameraConfig {
                device_index: 0,
                resolution: (1280, 720),
                fps: 30,
                auto_exposure: true,
            },
            ui: UiConfig {
                window_width: 1920,
                window_height: 1080,
                transparency: 0.85,
                theme: "jarvis_blue".to_string(),
                fps_target: 60,
                enable_glow: true,
                hologram_intensity: 0.7,
            },
            gestures: GestureConfig {
                enabled: true,
                sensitivity: 0.8,
                smoothing: 0.5,
                min_confidence: 0.7,
            },
            models: ModelPaths {
                vosk_russian: PathBuf::from("models/vosk-model-ru-0.42"),
                vosk_english: PathBuf::from("models/vosk-model-en-us-0.22"),
                mediapipe_hands: PathBuf::from("models/mediapipe_hand_landmark.onnx"),
            },
            voice: VoiceConfig {
                enabled: true,
                speed: 1.0,
                pitch: 1.0,
                voice_id: "ru-RU-SvetlanaNeural".to_string(),
            },
        }
    }
}

pub fn load_config() -> Result<JarvisConfig, Box<dyn std::error::Error>> {
    let config_path = PathBuf::from("config/jarvis.toml");

    if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        let config: JarvisConfig = toml::from_str(&content)?;

        if config.ui.fps_target == 0 {
            return Err("fps_target не может быть 0".into());
        }
        if config.audio.sample_rate == 0 {
            return Err("sample_rate не может быть 0".into());
        }

        Ok(config)
    } else {
        let config = JarvisConfig::default();
        let toml_string = toml::to_string_pretty(&config)?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&config_path, toml_string)?;
        Ok(config)
    }
}
