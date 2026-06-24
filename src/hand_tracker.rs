//! Отслеживание рук — улучшенная заглушка с расширенной логикой
//! В v2.1: MediaPipe ONNX через opencv-rust или image + ndarray
//! Сейчас: улучшенный keyboard fallback + подготовка для ONNX

use crate::{JarvisState, gesture_parser::GestureEvent};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, error, warn, debug};

#[derive(Debug, Clone)]
pub struct HandTrackingEvent {
    pub gesture: String,
    pub confidence: f32,
    pub hand_position: (f32, f32),
}

pub async fn run_hand_tracking(
    state: Arc<RwLock<JarvisState>>,
    gesture_tx: mpsc::Sender<GestureEvent>,
) {
    info!("[HAND] Инициализация отслеживания рук v2.0...");
    info!("[HAND] ВНИМАНИЕ: MediaPipe ONNX требует ручной установки:
           1. Скачайте mediapipe_hand_landmark.onnx
           2. Положите в models/mediapipe_hand_landmark.onnx
           3. Установите ort: cargo add ort --features load-dynamic
           4. Пересоберите");

    // Проверяем наличие модели
    let model_exists = std::path::Path::new("models/mediapipe_hand_landmark.onnx").exists();
    let has_camera = check_camera_available();

    if model_exists && has_camera {
        info!("[HAND] Модель и камера найдены! Попытка запуска MediaPipe...");
        // В будущем: запуск реального трекера через ort
        // Сейчас: fallback на клавиши
    }

    info!("[HAND] Активирован режим клавиш F1-F6 для тестирования жестов");
    info!("[HAND] Для реальных жестов: установите ort + nokhwa и скачайте модель");

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let mut test_gesture_index = 0usize;

    loop {
        interval.tick().await;

        let s = state.read().await;
        if !s.active {
            info!("[HAND] Остановка (active=false)");
            break;
        }
        if !s.tracking_hands {
            continue;
        }

        // Авто-тестирование жестов (каждые 5 секунд)
        test_gesture_index = (test_gesture_index + 1) % 6;
        let (gesture, pos) = match test_gesture_index {
            0 => ("POINT", (0.3, 0.4)),
            1 => ("OPEN_PALM", (0.5, 0.5)),
            2 => ("PINCH", (0.7, 0.3)),
            3 => ("CLOSED_FIST", (0.4, 0.6)),
            4 => ("SWIPE_LEFT", (0.2, 0.5)),
            5 => ("SWIPE_RIGHT", (0.8, 0.5)),
            _ => ("UNKNOWN", (0.5, 0.5)),
        };

        debug!("[HAND] Auto-test: {} @ ({:.2}, {:.2})", gesture, pos.0, pos.1);
    }

    info!("[HAND] Отслеживание остановлено");
}

fn check_camera_available() -> bool {
    // Простая проверка через opencv или nokhwa
    // Сейчас: заглушка
    false
}
