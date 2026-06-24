# J.A.R.V.I.S. — Holographic AI Assistant

Голографический ИИ-ассистент в стиле Железного человека. Управление голосом и жестами рук.

## Возможности

- 🎙️ **Голосовое управление** (Vosk, офлайн, русский/английский)
- ✋ **Жесты рук** (MediaPipe Hands, через веб-камеру)
- 🖥️ **Голографический UI** (wgpu, прозрачные окна, шейдеры)
- 📊 **Мониторинг системы** (CPU, RAM, GPU AMD, сеть)
- 🔊 **Голосовые ответы** (Windows SAPI / Edge TTS)
- ⚡ **Максимальная скорость** (Rust, GPU-ускорение, zero-copy)

## Архитектура

```
jarvis/
├── src/
│   ├── main.rs              # Точка входа, оркестрация
│   ├── audio_engine.rs      # Захват аудио (cpal)
│   ├── voice_activity_detector.rs  # VAD (энергия + ZCR)
│   ├── speech_recognizer.rs # Распознавание речи (Vosk)
│   ├── hand_tracker.rs      # Отслеживание рук (MediaPipe ONNX)
│   ├── gesture_parser.rs    # Распознавание жестов
│   ├── holographic_ui.rs    # GPU-рендеринг (wgpu)
│   ├── system_monitor.rs    # Мониторинг ресурсов
│   ├── tts_engine.rs        # Синтез речи
│   ├── windows_api.rs       # Windows API обёртки
│   ├── amd_gpu.rs           # AMD GPU мониторинг
│   └── config.rs            # Конфигурация
├── assets/shaders/          # WGSL шейдеры
├── config/                  # Конфигурационные файлы
├── models/                  # ML модели (Vosk, MediaPipe)
└── Cargo.toml
```

## Сборка

### Требования

- Windows 10/11
- Rust 1.75+
- Visual Studio Build Tools (C++ workload)
- Git

### Установка

```powershell
# 1. Клонирование
git clone https://github.com/Dirkog/jarvis-holographic-assistant.git
cd jarvis-holographic-assistant

# 2. Скачивание моделей
# Vosk Russian: https://alphacephei.com/vosk/models/vosk-model-ru-0.42.zip
# Распаковать в models/vosk-model-ru-0.42

# 3. Скачивание Vosk DLL
# https://github.com/alphacep/vosk-api/releases
# vosk-win64-*.zip → извлечь vosk.dll → положить в libs/ или рядом с .exe

# 4. Сборка
cargo build --release

# 5. Запуск
.\target\release\jarvis.exe
```

## Горячие клавиши (тестирование)

| Клавиша | Жест | Действие |
|---------|------|----------|
| F1 | POINT | Указание |
| F2 | OPEN_PALM | Пауза |
| F3 | PINCH | Клик |
| F4 | CLOSED_FIST | Захват |
| F5 | SWIPE_LEFT | Предыдущий стол |
| F6 | SWIPE_RIGHT | Следующий стол |
| F7 | — | Метрики системы |
| ESC | — | Выход |

## Голосовые команды (русский)

- "Открой браузер" — Chrome
- "Открой проводник" — Explorer
- "Закрой окно" — Alt+F4
- "Покажи рабочий стол" — Win+D
- "Системная информация" — CPU/RAM/GPU
- "Который час" — Текущее время
- "Сделай скриншот" — Win+Shift+S
- "Выключи компьютер" — Shutdown
- "Перезагрузи" — Restart
- "Заблокируй" — Win+L

## Логи

Логи пишутся в папку `logs/` (ротация каждый час). Критично, т.к. приложение запускается без консоли (`#![windows_subsystem = "windows"]`).

## Лицензия

MIT
