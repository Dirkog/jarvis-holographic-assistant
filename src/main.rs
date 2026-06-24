//! JARVIS — Holographic AI Assistant v2.0
//! Оптимизировано для AMD Ryzen 7 5825U с Radeon Vega 8
//! Полная Windows-реализация: MediaPipe Hands, Text Rendering

#![windows_subsystem = "windows"]

mod audio_engine;
mod speech_recognizer;
mod hand_tracker;
mod gesture_parser;
mod holographic_ui;
mod system_monitor;
mod tts_engine;
mod config;
mod voice_activity_detector;
mod windows_api;
mod amd_gpu;

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::AbortHandle;
use tracing::{info, error};

#[derive(Debug, Clone)]
pub struct JarvisState {
    pub active: bool,
    pub listening: bool,
    pub tracking_hands: bool,
    pub current_language: String,
    pub last_command: String,
    pub system_metrics: SystemMetrics,
    pub last_gesture: String,
    pub cursor_position: (f32, f32),
}

#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub gpu_usage: f32,
    pub gpu_temp: f32,
    pub network_up: f32,
    pub network_down: f32,
}

#[derive(Debug, Clone)]
pub enum UICommand {
    UpdateMetrics(SystemMetrics),
    UpdateGesture(String),
    UpdateCommand(String),
    Speak(String),
    Exit,
}

#[tokio::main]
async fn main() {
    let appender = tracing_appender::rolling::hourly("logs", "jarvis");
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(appender)
        .with_ansi(false)
        .init();

    info!("╔════════════════════════════════════════════════════╗");
    info!("║     J.A.R.V.I.S.  v2.0  SYSTEM  INITIALIZING       ║");
    info!("║     AMD Ryzen 7 5825U | Vega 8 | Full Windows     ║");
    info!("║     MediaPipe Hands | Text Rendering | Advanced AI  ║");
    info!("╚════════════════════════════════════════════════════╝");

    let config = match config::load_config() {
        Ok(cfg) => {
            info!("[CONFIG] Загружена конфигурация: {:?}", cfg.default_language);
            cfg
        }
        Err(e) => {
            error!("[CONFIG] Ошибка: {}. Используем defaults.", e);
            config::JarvisConfig::default()
        }
    };

    let state = Arc::new(RwLock::new(JarvisState {
        active: true,
        listening: true,
        tracking_hands: true,
        current_language: config.default_language.clone(),
        last_command: String::new(),
        system_metrics: SystemMetrics::default(),
        last_gesture: String::from("None"),
        cursor_position: (0.5, 0.5),
    }));

    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>(1024);
    let (speech_audio_tx, speech_audio_rx) = mpsc::channel::<Vec<f32>>(64);
    let (gesture_tx, gesture_rx) = mpsc::channel::<gesture_parser::GestureEvent>(64);
    let (tts_tx, tts_rx) = mpsc::channel::<String>(16);
    let (ui_cmd_tx, ui_cmd_rx) = mpsc::channel::<UICommand>(32);

    let mut handles: Vec<AbortHandle> = Vec::new();

    handles.push(tokio::spawn({
        let s = Arc::clone(&state);
        let tx = audio_tx;
        async move { audio_engine::run_audio_capture(s, tx).await; }
    }).abort_handle());

    handles.push(tokio::spawn({
        let s = Arc::clone(&state);
        let cmd_tx = speech_audio_tx.clone();
        let audio_tx = speech_audio_tx;
        async move { voice_activity_detector::run_vad(s, audio_rx, cmd_tx, audio_tx).await; }
    }).abort_handle());

    handles.push(tokio::spawn({
        let s = Arc::clone(&state);
        let tx = tts_tx.clone();
        let ui_tx = ui_cmd_tx.clone();
        async move { speech_recognizer::run_speech_recognition(s, speech_audio_rx, tx, ui_tx).await; }
    }).abort_handle());

    handles.push(tokio::spawn({
        let s = Arc::clone(&state);
        let tx = gesture_tx;
        async move { hand_tracker::run_hand_tracking(s, tx).await; }
    }).abort_handle());

    handles.push(tokio::spawn({
        let s = Arc::clone(&state);
        async move { gesture_parser::run_gesture_parser(s, gesture_rx).await; }
    }).abort_handle());

    handles.push(tokio::spawn({
        let s = Arc::clone(&state);
        let tx = ui_cmd_tx.clone();
        async move { system_monitor::run_system_monitor(s, tx).await; }
    }).abort_handle());

    handles.push(tokio::spawn({
        let s = Arc::clone(&state);
        async move { tts_engine::run_tts_engine(s, tts_rx).await; }
    }).abort_handle());

    info!("[MAIN] Запуск голографического интерфейса v2.0...");
    let ui_result = holographic_ui::run_holographic_ui(state.clone(), ui_cmd_rx);

    if let Err(e) = ui_result {
        error!("[MAIN] UI ошибка: {}", e);
    }

    info!("[MAIN] Инициализация graceful shutdown...");
    {
        let mut s = state.write().await;
        s.active = false;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

    for handle in &handles {
        if !handle.is_finished() {
            handle.abort();
        }
    }

    for handle in handles {
        let _ = handle.await;
    }

    info!("[MAIN] JARVIS v2.0 shutdown complete");
}
