//! Синтез речи — Windows SAPI через COM (sapi-lite)
//! Быстрее PowerShell, нет задержки на создание процесса

use crate::JarvisState;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, error, debug, warn};

#[cfg(all(windows, feature = "sapi-tts"))]
use sapi_lite::tts::{Synthesizer, SyncSynthesizer};

pub async fn run_tts_engine(
    state: Arc<RwLock<JarvisState>>,
    mut tts_rx: mpsc::Receiver<String>,
) {
    info!("[TTS] Инициализация синтеза речи...");

    #[cfg(all(windows, feature = "sapi-tts"))]
    {
        // Инициализация SAPI
        if let Err(e) = sapi_lite::initialize() {
            error!("[TTS] Не удалось инициализировать SAPI: {}. Переключаемся на PowerShell fallback.", e);
            run_powershell_tts(state, tts_rx).await;
            return;
        }

        // Создаём синтезатор
        let synthesizer = match SyncSynthesizer::new() {
            Ok(s) => {
                info!("[TTS] SAPI синтезатор создан");
                s
            }
            Err(e) => {
                error!("[TTS] Не удалось создать синтезатор: {}. Переключаемся на PowerShell.", e);
                sapi_lite::finalize();
                run_powershell_tts(state, tts_rx).await;
                return;
            }
        };

        // Выбираем голос (русский женский)
        if let Ok(voices) = synthesizer.get_voices() {
            let russian_voice = voices.iter().find(|v| {
                v.get_language().map(|l| l.contains("ru")).unwrap_or(false)
            });

            if let Some(voice) = russian_voice {
                if let Err(e) = synthesizer.set_voice(voice) {
                    warn!("[TTS] Не удалось установить русский голос: {}", e);
                } else {
                    info!("[TTS] Установлен русский голос: {:?}", voice.get_name());
                }
            }
        }

        // Основной цикл
        while let Some(text) = tts_rx.recv().await {
            {
                let s = state.read().await;
                if !s.active {
                    info!("[TTS] Остановка (active=false)");
                    break;
                }
            }

            info!("[TTS] Ответ: \"{}\"", text);

            if let Err(e) = synthesizer.speak(&text) {
                warn!("[TTS] Ошибка синтеза: {}", e);
            } else {
                debug!("[TTS] Голосовой ответ отправлен");
            }
        }

        sapi_lite::finalize();
    }

    #[cfg(not(all(windows, feature = "sapi-tts")))]
    {
        info!("[TTS] SAPI недоступен, используем PowerShell fallback");
        run_powershell_tts(state, tts_rx).await;
    }

    info!("[TTS] Синтез речи остановлен");
}

async fn run_powershell_tts(
    state: Arc<RwLock<JarvisState>>,
    mut tts_rx: mpsc::Receiver<String>,
) {
    info!("[TTS] PowerShell fallback активирован");

    while let Some(text) = tts_rx.recv().await {
        {
            let s = state.read().await;
            if !s.active {
                break;
            }
        }

        let ps_script = format!(
            r#"Add-Type -AssemblyName System.Speech; 
            $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer; 
            $synth.SelectVoiceByHints([System.Speech.Synthesis.VoiceGender]::Female, [System.Speech.Synthesis.VoiceAge]::Adult); 
            $synth.Speak('{}');"#,
            text.replace("'", "''")
        );

        match tokio::process::Command::new("powershell")
            .args(["-WindowStyle", "Hidden", "-Command", &ps_script])
            .output()
            .await
        {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!("[TTS] PowerShell ошибка: {}", stderr);
                }
            }
            Err(e) => {
                error!("[TTS] Не удалось запустить PowerShell: {}", e);
            }
        }
    }
}
