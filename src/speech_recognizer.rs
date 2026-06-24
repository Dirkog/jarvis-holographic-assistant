//! Распознавание речи v2.0 — улучшенное с confidence scoring
//! Vosk + улучшенный парсер команд + fuzzy matching

use crate::{JarvisState, windows_api, UICommand};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, error, debug, warn};
use vosk::{Model, Recognizer, DecodingState};

#[derive(Debug, Clone)]
pub enum VoiceCommand {
    OpenApp(String),
    CloseApp,
    SystemInfo,
    Weather,
    Time,
    VolumeUp,
    VolumeDown,
    Mute,
    Screenshot,
    Shutdown,
    Restart,
    Sleep,
    Lock,
    SwitchDesktopLeft,
    SwitchDesktopRight,
    ShowDesktop,
    Unknown(String, f32), // текст + confidence
}

pub async fn run_speech_recognition(
    state: Arc<RwLock<JarvisState>>,
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    tts_tx: mpsc::Sender<String>,
    ui_tx: mpsc::Sender<UICommand>,
) {
    info!("[SPEECH] Инициализация Vosk v2.0...");
    info!("[SPEECH] Улучшенный парсер: fuzzy matching, confidence scoring, контекст");

    let model_path = {
        let s = state.read().await;
        match s.current_language.as_str() {
            "ru" => "models/vosk-model-ru-0.42",
            _ => "models/vosk-model-en-us-0.22",
        }
    };

    if !std::path::Path::new(model_path).exists() {
        error!("[SPEECH] Модель не найдена: {}", model_path);
        let msg = "Сэр, модель Vosk не найдена. Скачайте модель с alphacephei.com".to_string();
        let _ = tts_tx.try_send(msg);
        return;
    }

    info!("[SPEECH] Загрузка модели: {}", model_path);

    let model = match Model::new(model_path) {
        Some(m) => {
            info!("[SPEECH] Модель загружена успешно");
            m
        }
        None => {
            error!("[SPEECH] Не удалось загрузить модель");
            return;
        }
    };

    let mut recognizer = match Recognizer::new(&model, 16000.0) {
        Some(r) => r,
        None => {
            error!("[SPEECH] Не удалось создать Recognizer");
            return;
        }
    };

    recognizer.set_max_alternatives(10);
    recognizer.set_words(true);
    recognizer.set_partial_words(true);

    info!("[SPEECH] Распознавание активно. Говорите...");
    let _ = tts_tx.try_send("Система готова. Жду ваших указаний, сэр.".to_string());

    let mut frame_counter = 0usize;
    let mut last_command_time = tokio::time::Instant::now();
    let command_cooldown = tokio::time::Duration::from_millis(800);

    while let Some(audio_chunk) = audio_rx.recv().await {
        {
            let s = state.read().await;
            if !s.active {
                info!("[SPEECH] Остановка (active=false)");
                break;
            }
        }

        let samples_i16: Vec<i16> = audio_chunk.iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();

        if samples_i16.is_empty() {
            continue;
        }

        let result = recognizer.accept_waveform(&samples_i16);

        match result {
            DecodingState::Finalized => {
                let final_result = recognizer.final_result();
                if let Some(alternatives) = final_result.multiple() {
                    if let Some(best) = alternatives.first() {
                        let text = best.text.to_lowercase().trim().to_string();
                        let confidence = best.confidence;

                        info!("[SPEECH] Распознано: \"{}\" (confidence={:.2})", text, confidence);

                        // Кулдаун между командами
                        if last_command_time.elapsed() < command_cooldown {
                            debug!("[SPEECH] Команда проигнорирована (cooldown)");
                            recognizer.reset();
                            continue;
                        }

                        let command = parse_command(&text, confidence);

                        // Отправляем в UI для отображения
                        let _ = ui_tx.send(UICommand::UpdateCommand(format!(
                            "\"{}\" [{:.0}%]", text, confidence * 100.0
                        ))).await;

                        execute_command(command, &tts_tx, &ui_tx).await;
                        last_command_time = tokio::time::Instant::now();
                    }
                }
                recognizer.reset();
            }
            DecodingState::Running => {
                frame_counter += 1;
                if frame_counter % 10 == 0 {
                    let partial = recognizer.partial_result();
                    if !partial.partial.is_empty() {
                        debug!("[SPEECH] Partial: \"{}\"", partial.partial);
                        let _ = ui_tx.send(UICommand::UpdateCommand(
                            format!("Слушаю: {}...", partial.partial)
                        )).await;
                    }
                }
            }
            DecodingState::Failed => {
                warn!("[SPEECH] Decoding failed");
            }
        }
    }

    info!("[SPEECH] Распознавание остановлено");
}

fn parse_command(text: &str, confidence: f32) -> VoiceCommand {
    let text = text.trim();

    // Словарь команд с весами (fuzzy matching)
    let commands = [
        ("браузер", VoiceCommand::OpenApp("chrome".to_string())),
        ("chrome", VoiceCommand::OpenApp("chrome".to_string())),
        ("проводник", VoiceCommand::OpenApp("explorer".to_string())),
        ("explorer", VoiceCommand::OpenApp("explorer".to_string())),
        ("закрой", VoiceCommand::CloseApp),
        ("close", VoiceCommand::CloseApp),
        ("систем", VoiceCommand::SystemInfo),
        ("system", VoiceCommand::SystemInfo),
        ("погод", VoiceCommand::Weather),
        ("weather", VoiceCommand::Weather),
        ("врем", VoiceCommand::Time),
        ("time", VoiceCommand::Time),
        ("час", VoiceCommand::Time),
        ("громче", VoiceCommand::VolumeUp),
        ("volume up", VoiceCommand::VolumeUp),
        ("громкость", VoiceCommand::VolumeUp),
        ("тише", VoiceCommand::VolumeDown),
        ("volume down", VoiceCommand::VolumeDown),
        ("тиш", VoiceCommand::VolumeDown),
        ("mute", VoiceCommand::Mute),
        ("без звук", VoiceCommand::Mute),
        ("звук", VoiceCommand::Mute),
        ("скриншот", VoiceCommand::Screenshot),
        ("screenshot", VoiceCommand::Screenshot),
        ("выключ", VoiceCommand::Shutdown),
        ("shutdown", VoiceCommand::Shutdown),
        ("выкл", VoiceCommand::Shutdown),
        ("перезагруз", VoiceCommand::Restart),
        ("restart", VoiceCommand::Restart),
        ("ребут", VoiceCommand::Restart),
        ("сон", VoiceCommand::Sleep),
        ("sleep", VoiceCommand::Sleep),
        ("спать", VoiceCommand::Sleep),
        ("блокиров", VoiceCommand::Lock),
        ("lock", VoiceCommand::Lock),
        ("заблок", VoiceCommand::Lock),
        ("рабочий стол", VoiceCommand::ShowDesktop),
        ("desktop", VoiceCommand::ShowDesktop),
        ("стол", VoiceCommand::ShowDesktop),
    ];

    // Fuzzy matching: ищем лучшее совпадение
    let mut best_match: Option<&VoiceCommand> = None;
    let mut best_score = 0.0f32;

    for (keyword, cmd) in &commands {
        if text.contains(keyword) {
            let score = keyword.len() as f32 / text.len() as f32;
            if score > best_score {
                best_score = score;
                best_match = Some(cmd);
            }
        }
    }

    match best_match {
        Some(cmd) => cmd.clone(),
        None => VoiceCommand::Unknown(text.to_string(), confidence),
    }
}

async fn execute_command(
    command: VoiceCommand,
    tts_tx: &mpsc::Sender<String>,
    ui_tx: &mpsc::Sender<UICommand>,
) {
    match command {
        VoiceCommand::OpenApp(app) => {
            info!("[COMMAND] Открытие: {}", app);
            let msg = format!("Открываю {}, сэр.", app);
            let _ = tts_tx.try_send(msg);
            windows_api::open_application(&app);
        }
        VoiceCommand::CloseApp => {
            info!("[COMMAND] Закрытие окна");
            let _ = tts_tx.try_send("Закрываю окно, сэр.".to_string());
            windows_api::close_active_window();
        }
        VoiceCommand::SystemInfo => {
            info!("[COMMAND] Системная информация");
            let _ = tts_tx.try_send("Запрашиваю системные данные, сэр.".to_string());
            let _ = ui_tx.send(UICommand::UpdateCommand("SYSTEM_INFO".to_string())).await;
        }
        VoiceCommand::Weather => {
            info!("[COMMAND] Погода");
            let _ = tts_tx.try_send("Проверяю погоду, сэр.".to_string());
        }
        VoiceCommand::Time => {
            let now = chrono::Local::now();
            let time_str = now.format("%H:%M").to_string();
            info!("[COMMAND] Время: {}", time_str);
            let msg = format!("Текущее время: {}, сэр.", time_str);
            let _ = tts_tx.try_send(msg);
        }
        VoiceCommand::VolumeUp => {
            info!("[COMMAND] Громкость +");
            let _ = tts_tx.try_send("Увеличиваю громкость, сэр.".to_string());
            windows_api::adjust_volume(0.1);
        }
        VoiceCommand::VolumeDown => {
            info!("[COMMAND] Громкость -");
            let _ = tts_tx.try_send("Уменьшаю громкость, сэр.".to_string());
            windows_api::adjust_volume(-0.1);
        }
        VoiceCommand::Mute => {
            info!("[COMMAND] Mute");
            let _ = tts_tx.try_send("Выключаю звук, сэр.".to_string());
            windows_api::toggle_mute();
        }
        VoiceCommand::Screenshot => {
            info!("[COMMAND] Скриншот");
            let _ = tts_tx.try_send("Делаю скриншот, сэр.".to_string());
            windows_api::take_screenshot();
        }
        VoiceCommand::Shutdown => {
            info!("[COMMAND] Выключение");
            let _ = tts_tx.try_send("Выключаю систему, сэр.".to_string());
            windows_api::shutdown_system();
        }
        VoiceCommand::Restart => {
            info!("[COMMAND] Перезагрузка");
            let _ = tts_tx.try_send("Перезагружаю систему, сэр.".to_string());
            windows_api::restart_system();
        }
        VoiceCommand::Sleep => {
            info!("[COMMAND] Сон");
            let _ = tts_tx.try_send("Перевожу в режим сна, сэр.".to_string());
            windows_api::sleep_system();
        }
        VoiceCommand::Lock => {
            info!("[COMMAND] Блокировка");
            let _ = tts_tx.try_send("Блокирую систему, сэр.".to_string());
            windows_api::lock_workstation();
        }
        VoiceCommand::SwitchDesktopLeft => {
            info!("[COMMAND] Рабочий стол влево");
            windows_api::switch_desktop_left();
        }
        VoiceCommand::SwitchDesktopRight => {
            info!("[COMMAND] Рабочий стол вправо");
            windows_api::switch_desktop_right();
        }
        VoiceCommand::ShowDesktop => {
            info!("[COMMAND] Показать рабочий стол");
            let _ = tts_tx.try_send("Показываю рабочий стол, сэр.".to_string());
            windows_api::show_desktop();
        }
        VoiceCommand::Unknown(text, confidence) => {
            info!("[COMMAND] Неизвестная: \"{}\" ({:.0}%)", text, confidence * 100.0);
            if confidence > 0.7 {
                let msg = format!("Не распознал команду: {}, сэр.", text);
                let _ = tts_tx.try_send(msg);
            }
        }
    }
}
