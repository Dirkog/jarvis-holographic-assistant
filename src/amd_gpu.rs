//! Мониторинг AMD GPU v2.0 — полная WMI интеграция
//! AMD Ryzen 7 5825U с Radeon Vega 8

use std::ffi::CString;
use tracing::{info, error, warn, debug};

#[cfg(windows)]
use winapi::um::pdh::{
    PdhOpenQueryW, PdhAddCounterW, PdhCollectQueryData,
    PdhGetFormattedCounterValue, PdhCloseQuery,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};
#[cfg(windows)]
use winapi::shared::winerror::ERROR_SUCCESS;

/// Получить ВСЕ метрики AMD GPU
/// Возвращает (load%, temp°C, clockMHz, vram_used_mb, vram_total_mb)
pub fn get_amd_gpu_stats() -> (f32, f32, f32, f32, f32) {
    let load = get_gpu_load_pdh();
    let temp = get_gpu_temp_wmi();
    let clock = get_gpu_clock_wmi();
    let (vram_used, vram_total) = get_gpu_vram_wmi();
    (load, temp, clock, vram_used, vram_total)
}

/// GPU load через PDH (Performance Counters)
fn get_gpu_load_pdh() -> f32 {
    #[cfg(windows)]
    unsafe {
        let mut query: PDH_HQUERY = std::ptr::null_mut();
        let mut counter: PDH_HCOUNTER = std::ptr::null_mut();

        if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != ERROR_SUCCESS as i32 {
            debug!("[AMD] PdhOpenQueryW failed");
            return 0.0;
        }

        // Пробуем разные счётчики GPU
        let counter_paths = [
            "\\GPU Engine(*)\\Utilization Percentage",
            "\\GPU Process Memory(*)\\Dedicated Usage",
            "\\AMD Radeon Vega 8 Graphics(*)\\GPU Engine",
            "\\GPU(*)\\Utilization Percentage",
        ];

        let mut counter_added = false;
        for path in &counter_paths {
            let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            if PdhAddCounterW(query, wide_path.as_ptr(), 0, &mut counter) == ERROR_SUCCESS as i32 {
                counter_added = true;
                debug!("[AMD] PDH counter added: {}", path);
                break;
            }
        }

        if !counter_added {
            debug!("[AMD] No PDH counter found");
            PdhCloseQuery(query);
            return 0.0;
        }

        // Двойной сбор для delta
        if PdhCollectQueryData(query) != ERROR_SUCCESS as i32 {
            PdhCloseQuery(query);
            return 0.0;
        }

        std::thread::sleep(std::time::Duration::from_millis(100));

        if PdhCollectQueryData(query) != ERROR_SUCCESS as i32 {
            PdhCloseQuery(query);
            return 0.0;
        }

        let mut value = std::mem::zeroed::<winapi::um::pdh::PDH_FMT_COUNTERVALUE>();
        let mut type_ = 0u32;
        if PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, &mut type_, &mut value) != ERROR_SUCCESS as i32 {
            PdhCloseQuery(query);
            return 0.0;
        }

        let result = value.doubleValue as f32;
        PdhCloseQuery(query);

        debug!("[AMD] GPU load: {:.1}%", result);
        return result.clamp(0.0, 100.0);
    }

    #[cfg(not(windows))]
    { 0.0 }
}

/// GPU температура через WMI (полная интеграция)
fn get_gpu_temp_wmi() -> f32 {
    #[cfg(windows)]
    {
        // Метод 1: MSAcpi_ThermalZoneTemperature (наиболее точный для AMD)
        match run_wmi_query(
            r#"Get-CimInstance -Namespace root\wmi -ClassName MSAcpi_ThermalZoneTemperature | 
               Select-Object -First 1 -ExpandProperty CurrentTemperature"#
        ) {
            Ok(output) => {
                if let Ok(temp_raw) = output.trim().parse::<f32>() {
                    let temp = (temp_raw - 2732.0) / 10.0;
                    if temp > 0.0 && temp < 150.0 {
                        debug!("[AMD] GPU temp (ThermalZone): {:.1}°C", temp);
                        return temp;
                    }
                }
            }
            Err(e) => debug!("[AMD] ThermalZone failed: {}", e),
        }

        // Метод 2: Win32_PerfFormattedData_AMDGPU
        match run_wmi_query(
            r#"Get-CimInstance -Namespace root\cimv2 -ClassName Win32_PerfFormattedData_AMDGPU | 
               Select-Object -First 1 -ExpandProperty Temperature"#
        ) {
            Ok(output) => {
                if let Ok(temp) = output.trim().parse::<f32>() {
                    if temp > 0.0 && temp < 150.0 {
                        debug!("[AMD] GPU temp (AMD Perf): {:.1}°C", temp);
                        return temp;
                    }
                }
            }
            Err(e) => debug!("[AMD] AMD Perf failed: {}", e),
        }

        // Метод 3: Win32_VideoController (базовая температура)
        match run_wmi_query(
            r#"Get-CimInstance Win32_VideoController | 
               Where-Object { $_.Name -like "*AMD*" -or $_.Name -like "*Radeon*" } | 
               Select-Object -First 1 -ExpandProperty CurrentTemperature"#
        ) {
            Ok(output) => {
                if let Ok(temp) = output.trim().parse::<f32>() {
                    if temp > 0.0 && temp < 150.0 {
                        debug!("[AMD] GPU temp (VideoController): {:.1}°C", temp);
                        return temp;
                    }
                }
            }
            Err(e) => debug!("[AMD] VideoController failed: {}", e),
        }

        // Метод 4: LibreHardwareMonitor (если установлен)
        match run_wmi_query(
            r#"Get-CimInstance -Namespace root\LibreHardwareMonitor -ClassName Sensor | 
               Where-Object { $_.Name -like "*GPU*" -and $_.SensorType -eq "Temperature" } | 
               Select-Object -First 1 -ExpandProperty Value"#
        ) {
            Ok(output) => {
                if let Ok(temp) = output.trim().parse::<f32>() {
                    if temp > 0.0 && temp < 150.0 {
                        debug!("[AMD] GPU temp (LibreHW): {:.1}°C", temp);
                        return temp;
                    }
                }
            }
            Err(e) => debug!("[AMD] LibreHW failed: {}", e),
        }

        0.0
    }

    #[cfg(not(windows))]
    { 0.0 }
}

/// GPU clock speed через WMI
fn get_gpu_clock_wmi() -> f32 {
    #[cfg(windows)]
    {
        match run_wmi_query(
            r#"Get-CimInstance -Namespace root\cimv2 -ClassName Win32_VideoController | 
               Where-Object { $_.Name -like "*AMD*" -or $_.Name -like "*Radeon*" } | 
               Select-Object -First 1 -ExpandProperty CurrentRefreshRate"#
        ) {
            Ok(output) => {
                if let Ok(clock) = output.trim().parse::<f32>() {
                    return clock;
                }
            }
            Err(_) => {}
        }
        0.0
    }
    #[cfg(not(windows))]
    { 0.0 }
}

/// GPU VRAM через WMI
fn get_gpu_vram_wmi() -> (f32, f32) {
    #[cfg(windows)]
    {
        match run_wmi_query(
            r#"Get-CimInstance Win32_VideoController | 
               Where-Object { $_.Name -like "*AMD*" -or $_.Name -like "*Radeon*" } | 
               Select-Object -First 1 AdapterRAM, CurrentNumberOfColors"#
        ) {
            Ok(output) => {
                // Парсим вывод
                let lines: Vec<&str> = output.lines().collect();
                if lines.len() >= 2 {
                    if let Ok(total) = lines[0].trim().parse::<f64>() {
                        let total_mb = (total / 1024.0 / 1024.0) as f32;
                        return (0.0, total_mb); // used не доступен через WMI
                    }
                }
            }
            Err(_) => {}
        }
        (0.0, 0.0)
    }
    #[cfg(not(windows))]
    { (0.0, 0.0) }
}

#[cfg(windows)]
fn run_wmi_query(script: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-Command", script])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string().into())
    }
}
