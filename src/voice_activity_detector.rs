//! Voice Activity Detector v2.0 — улучшенный с noise suppression
//! Определяет наличие речи с адаптивным порогом и шумоподавлением

use crate::JarvisState;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, debug, warn};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadState {
    Silence,
    Speech,
}

pub struct VadConfig {
    pub energy_threshold: f32,
    pub zcr_threshold: f32,
    pub min_speech_frames: usize,
    pub max_silence_frames: usize,
    pub frame_size: usize,
    pub max_speech_buffer_size: usize,
    pub noise_floor: f32,        // Адаптивный порог шума
    pub snr_min: f32,            // Минимальное SNR для определения речи
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            energy_threshold: 0.015,
            zcr_threshold: 0.1,
            min_speech_frames: 5,
            max_silence_frames: 20,
            frame_size: 512,
            max_speech_buffer_size: 48000 * 10, // 10 секунд при 48kHz
            noise_floor: 0.005,                   // Начальный порог шума
            snr_min: 3.0,                        // Минимум 3dB SNR
        }
    }
}

pub struct VadEngine {
    config: VadConfig,
    noise_estimate: f32,         // Адаптивная оценка шума
    speech_count: usize,         // Счётчик фреймов речи (для адаптации)
}

impl VadEngine {
    pub fn new() -> Self {
        Self {
            config: VadConfig::default(),
            noise_estimate: 0.005,
            speech_count: 0,
        }
    }

    /// Обновляет адаптивную оценку шума
    fn update_noise_estimate(&mut self, energy: f32, is_speech: bool) {
        if !is_speech {
            // Экспоненциальное сглаживание для шума
            self.noise_estimate = self.noise_estimate * 0.95 + energy * 0.05;
        }
    }

    /// Вычисляет адаптивный порог на основе SNR
    fn adaptive_threshold(&self) -> f32 {
        let base_threshold = self.config.energy_threshold;
        let noise_compensated = base_threshold.max(self.noise_estimate * 2.0);
        noise_compensated
    }

    /// Проверка SNR
    fn check_snr(&self, energy: f32) -> bool {
        if self.noise_estimate < 1e-6 {
            return energy > self.config.energy_threshold;
        }
        let snr = 10.0 * (energy / self.noise_estimate).log10();
        snr > self.config.snr_min
    }
}

pub async fn run_vad(
    state: Arc<RwLock<JarvisState>>,
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    _command_tx: mpsc::Sender<Vec<f32>>,
    speech_audio_tx: mpsc::Sender<Vec<f32>>,
) {
    info!("[VAD] Инициализация улучшенного детектора речи v2.0...");
    info!("[VAD] Адаптивный порог, noise suppression, SNR-based detection");

    let mut engine = VadEngine::new();
    let mut vad_state = VadState::Silence;
    let mut speech_buffer: Vec<f32> = Vec::new();
    let mut silence_counter = 0usize;
    let mut speech_frames = 0usize;
    let mut accumulated: Vec<f32> = Vec::new();
    let mut frame_count = 0usize;

    while let Some(chunk) = audio_rx.recv().await {
        // Проверяем состояние системы
        {
            let s = state.read().await;
            if !s.active {
                info!("[VAD] Остановка (active=false)");
                break;
            }
        }

        accumulated.extend(chunk);
        frame_count += 1;

        // Защита от переполнения
        if accumulated.len() > engine.config.max_speech_buffer_size {
            warn!("[VAD] Accumulated buffer overflow, dropping old data");
            accumulated.clear();
            continue;
        }

        while accumulated.len() >= engine.config.frame_size {
            let frame: Vec<f32> = accumulated.drain(..engine.config.frame_size).collect();

            if frame.is_empty() {
                continue;
            }

            let energy = calculate_energy(&frame);
            let zcr = calculate_zcr(&frame);

            // Адаптивное определение речи
            let threshold = engine.adaptive_threshold();
            let is_speech = energy > threshold && zcr > engine.config.zcr_threshold && engine.check_snr(energy);

            // Обновляем оценку шума
            engine.update_noise_estimate(energy, is_speech);
            if is_speech {
                engine.speech_count += 1;
            }

            // Логирование каждые 100 фреймов
            if frame_count % 100 == 0 {
                debug!("[VAD] energy={:.6} threshold={:.6} noise={:.6} SNR={:.1}dB state={:?}",
                    energy, threshold, engine.noise_estimate,
                    if engine.noise_estimate > 1e-6 { 10.0 * (energy / engine.noise_estimate).log10() } else { 0.0 },
                    vad_state);
            }

            match vad_state {
                VadState::Silence => {
                    if is_speech {
                        vad_state = VadState::Speech;
                        speech_buffer.extend(frame);
                        speech_frames = 1;
                        silence_counter = 0;
                        debug!("[VAD] Речь обнаружена (energy={:.6})", energy);
                    }
                }
                VadState::Speech => {
                    speech_buffer.extend(frame);
                    speech_frames += 1;

                    if is_speech {
                        silence_counter = 0;
                    } else {
                        silence_counter += 1;
                    }

                    // Защита от переполнения speech_buffer
                    if speech_buffer.len() > engine.config.max_speech_buffer_size {
                        warn!("[VAD] Speech buffer overflow, forcing segment flush");
                        let audio_data = std::mem::take(&mut speech_buffer);
                        if let Err(e) = speech_audio_tx.try_send(audio_data) {
                            warn!("[VAD] Не удалось отправить аудио: {:?}", e);
                        }
                        vad_state = VadState::Silence;
                        speech_frames = 0;
                        silence_counter = 0;
                        continue;
                    }

                    if silence_counter >= engine.config.max_silence_frames {
                        if speech_frames >= engine.config.min_speech_frames {
                            let audio_data = std::mem::take(&mut speech_buffer);
                            if let Err(e) = speech_audio_tx.try_send(audio_data) {
                                warn!("[VAD] Канал распознавания переполнен: {:?}", e);
                            } else {
                                info!("[VAD] Отправлен аудио сегмент: {} сэмплов, {} фреймов речи",
                                    audio_data.len(), speech_frames);
                            }
                        } else {
                            speech_buffer.clear();
                        }
                        vad_state = VadState::Silence;
                        speech_frames = 0;
                        silence_counter = 0;
                    }
                }
            }
        }
    }

    info!("[VAD] Детектор остановлен. Обработано {} фреймов, {} речевых.",
        frame_count, engine.speech_count);
}

fn calculate_energy(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    // RMS energy с нормализацией
    let sum_sq = frame.iter().map(|&s| s * s).sum::<f32>();
    (sum_sq / frame.len() as f32).sqrt()
}

fn calculate_zcr(frame: &[f32]) -> f32 {
    if frame.len() < 2 {
        return 0.0;
    }
    let mut crossings = 0usize;
    for i in 1..frame.len() {
        if (frame[i] >= 0.0) != (frame[i - 1] >= 0.0) {
            crossings += 1;
        }
    }
    crossings as f32 / frame.len() as f32
}
