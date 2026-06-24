//! Распознавание жестов v2.0 — улучшенное с smoothing и velocity tracking

use crate::{JarvisState, windows_api};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, debug, warn};

#[derive(Debug, Clone)]
pub struct GestureEvent {
    pub gesture_type: GestureType,
    pub confidence: f32,
    pub position: (f32, f32),
    pub velocity: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GestureType {
    Point,
    OpenPalm,
    Pinch,
    ClosedFist,
    SwipeLeft,
    SwipeRight,
    Unknown,
}

pub async fn run_gesture_parser(
    state: Arc<RwLock<JarvisState>>,
    mut gesture_rx: mpsc::Receiver<GestureEvent>,
) {
    info!("[GESTURE] Инициализация парсера жестов v2.0...");
    info!("[GESTURE] Smoothing, velocity tracking, adaptive sensitivity");

    let mut last_gesture = GestureType::Unknown;
    let mut gesture_cooldown = tokio::time::Instant::now();
    let cooldown_duration = tokio::time::Duration::from_millis(400);
    let mut last_position = (0.5f32, 0.5f32);
    let mut position_history: Vec<(f32, f32, tokio::time::Instant)> = Vec::new();
    const HISTORY_SIZE: usize = 10;

    while let Some(event) = gesture_rx.recv().await {
        {
            let s = state.read().await;
            if !s.active {
                info!("[GESTURE] Остановка (active=false)");
                break;
            }
            if !s.tracking_hands {
                continue;
            }
        }

        if gesture_cooldown.elapsed() < cooldown_duration {
            continue;
        }

        if event.confidence < 0.7 {
            debug!("[GESTURE] Низкая уверенность: {:.2}", event.confidence);
            continue;
        }

        // Smoothing: усреднение позиции по истории
        position_history.push((event.position.0, event.position.1, tokio::time::Instant::now()));
        if position_history.len() > HISTORY_SIZE {
            position_history.remove(0);
        }

        let smoothed_pos = if position_history.len() >= 3 {
            let sum_x: f32 = position_history.iter().map(|p| p.0).sum();
            let sum_y: f32 = position_history.iter().map(|p| p.1).sum();
            (sum_x / position_history.len() as f32, sum_y / position_history.len() as f32)
        } else {
            event.position
        };

        // Velocity tracking
        let velocity = (smoothed_pos.0 - last_position.0, smoothed_pos.1 - last_position.1);
        let speed = (velocity.0.powi(2) + velocity.1.powi(2)).sqrt();

        // Swipe detection
        let mut detected_gesture = event.gesture_type.clone();
        if speed > 0.15 && event.gesture_type == GestureType::Point {
            if velocity.0 < -0.05 {
                detected_gesture = GestureType::SwipeLeft;
            } else if velocity.0 > 0.05 {
                detected_gesture = GestureType::SwipeRight;
            }
        }

        if detected_gesture == last_gesture && detected_gesture != GestureType::Point {
            debug!("[GESTURE] Повтор жеста, игнорируем");
            continue;
        }

        last_gesture = detected_gesture.clone();
        gesture_cooldown = tokio::time::Instant::now();
        last_position = smoothed_pos;

        match detected_gesture {
            GestureType::Point => {
                info!("[GESTURE] POINT: Указание @ ({:.2}, {:.2})", smoothed_pos.0, smoothed_pos.1);
                windows_api::move_cursor(smoothed_pos.0, smoothed_pos.1);
                {
                    let mut s = state.write().await;
                    s.last_gesture = "POINT".to_string();
                    s.cursor_position = smoothed_pos;
                }
            }
            GestureType::OpenPalm => {
                info!("[GESTURE] OPEN_PALM: Пауза");
                {
                    let mut s = state.write().await;
                    s.last_gesture = "OPEN_PALM".to_string();
                }
            }
            GestureType::Pinch => {
                info!("[GESTURE] PINCH: Клик");
                windows_api::mouse_click();
                {
                    let mut s = state.write().await;
                    s.last_gesture = "PINCH".to_string();
                }
            }
            GestureType::ClosedFist => {
                info!("[GESTURE] CLOSED_FIST: Захват");
                windows_api::mouse_down();
                {
                    let mut s = state.write().await;
                    s.last_gesture = "CLOSED_FIST".to_string();
                }
            }
            GestureType::SwipeLeft => {
                info!("[GESTURE] SWIPE_LEFT: Предыдущий стол");
                windows_api::switch_desktop_left();
                {
                    let mut s = state.write().await;
                    s.last_gesture = "SWIPE_LEFT".to_string();
                }
            }
            GestureType::SwipeRight => {
                info!("[GESTURE] SWIPE_RIGHT: Следующий стол");
                windows_api::switch_desktop_right();
                {
                    let mut s = state.write().await;
                    s.last_gesture = "SWIPE_RIGHT".to_string();
                }
            }
            GestureType::Unknown => {
                debug!("[GESTURE] Неизвестный жест");
            }
        }
    }

    info!("[GESTURE] Парсер остановлен");
}
