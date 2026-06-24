//! Аудио движок — захват с микрофона в реальном времени
//! cpal + ringbuf для zero-copy передачи

use crate::{JarvisState, config::JarvisConfig};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, error, debug, warn};
use ringbuf::HeapRb;

const RING_BUFFER_SIZE: usize = 32000;
const CHUNK_SIZE: usize = 1024;

pub async fn run_audio_capture(
    state: Arc<RwLock<JarvisState>>,
    audio_tx: mpsc::Sender<Vec<f32>>,
) {
    info!("[AUDIO] Инициализация аудио захвата...");

    let host = cpal::default_host();
    let device = match host.default_input_device() {
        Some(d) => {
            let name = d.name().unwrap_or_else(|_| "Unknown".to_string());
            info!("[AUDIO] Микрофон: {}", name);
            d
        }
        None => {
            error!("[AUDIO] Микрофон не найден!");
            return;
        }
    };

    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            error!("[AUDIO] Не удалось получить конфиг микрофона: {}", e);
            return;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let sample_format = config.sample_format();

    info!("[AUDIO] Конфиг: {}Hz, {} каналов, {:?}", sample_rate, channels, sample_format);

    if sample_format != cpal::SampleFormat::F32 {
        warn!("[AUDIO] Микрофон использует {:?}, конвертация в f32", sample_format);
    }

    let ring = HeapRb::<f32>::new(RING_BUFFER_SIZE);
    let (mut producer, mut consumer) = ring.split();

    let stream = match device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            for &sample in data {
                let _ = producer.try_push(sample);
            }
        },
        move |err| {
            error!("[AUDIO] Ошибка потока: {}", err);
        },
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            error!("[AUDIO] Не удалось создать аудио поток: {}", e);
            return;
        }
    };

    if let Err(e) = stream.play() {
        error!("[AUDIO] Не удалось запустить аудио поток: {}", e);
        return;
    }

    info!("[AUDIO] Захват активен");

    let mut buffer = Vec::with_capacity(CHUNK_SIZE);
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(10));

    loop {
        interval.tick().await;

        {
            let s = state.read().await;
            if !s.active {
                info!("[AUDIO] Остановка захвата (active=false)");
                break;
            }
        }

        while let Some(sample) = consumer.try_pop() {
            buffer.push(sample);
        }

        if !buffer.is_empty() {
            let chunk = buffer.clone();
            buffer.clear();

            if audio_tx.send(chunk).await.is_err() {
                info!("[AUDIO] Канал закрыт, остановка");
                break;
            }
        }
    }

    drop(stream);
    info!("[AUDIO] Захват остановлен");
}
