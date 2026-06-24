//! Мониторинг системных ресурсов v2.0
//! CPU, RAM, GPU AMD (load, temp, clock, vram), сеть

use crate::{JarvisState, SystemMetrics, UICommand};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, error, debug, warn};
use sysinfo::{System, RefreshKind, CpuRefreshKind, MemoryRefreshKind};

pub async fn run_system_monitor(
    state: Arc<RwLock<JarvisState>>,
    ui_tx: mpsc::Sender<UICommand>,
) {
    info!("[SYS] Инициализация мониторинга системы v2.0...");

    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::new().with_cpu_usage())
            .with_memory(MemoryRefreshKind::everything())
    );

    sys.refresh_cpu();
    sys.refresh_memory();

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
    let mut tick_count = 0u64;

    loop {
        interval.tick().await;

        {
            let s = state.read().await;
            if !s.active {
                info!("[SYS] Остановка мониторинга (active=false)");
                break;
            }
        }

        sys.refresh_cpu();
        sys.refresh_memory();

        let cpu_usage = sys.global_cpu_info().cpu_usage();
        let total_memory = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let used_memory = sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

        // GPU метрики через spawn_blocking
        let (gpu_usage, gpu_temp, gpu_clock, vram_used, vram_total) = 
            tokio::task::spawn_blocking(|| {
                crate::amd_gpu::get_amd_gpu_stats()
            }).await.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0));

        // Сетевые метрики (заглушка — требует реализации через WinAPI)
        let network_up = 0.0;
        let network_down = 0.0;

        let metrics = SystemMetrics {
            cpu_usage,
            ram_used_gb: used_memory as f32,
            ram_total_gb: total_memory as f32,
            gpu_usage,
            gpu_temp,
            network_up,
            network_down,
        };

        {
            let mut s = state.write().await;
            s.system_metrics = metrics.clone();
        }

        tick_count += 1;
        if tick_count % 5 == 0 {
            if let Err(e) = ui_tx.send(UICommand::UpdateMetrics(metrics)).await {
                warn!("[SYS] Не удалось отправить метрики в UI: {}", e);
            }
        }

        debug!(
            "[SYS] CPU: {:.1}% | RAM: {:.1}/{:.1}GB | GPU: {:.0}% @ {:.0}°C | Clock: {:.0}MHz | VRAM: {:.0}/{:.0}MB",
            cpu_usage, used_memory, total_memory, gpu_usage, gpu_temp, gpu_clock, vram_used, vram_total
        );
    }

    info!("[SYS] Мониторинг остановлен");
}
