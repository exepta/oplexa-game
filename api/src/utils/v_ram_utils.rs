#![allow(dead_code)]

//! Cross-platform V-RAM detection helpers.
//!
//! This module exposes vendor/OS specific ways to query *actual* V-RAM usage,
//! with graceful fallbacks when a backend is not available.
//!
//! - NVIDIA (Win/Linux): NVML → **per-process** bytes (preferred when available).
//! - Linux (DRM fdinfo): **per-process** VRAM for DRM clients when exposed by driver.
//! - Windows (AMD & NVIDIA): DXGI → **adapter-wide** "CurrentUsage" bytes.
//! - macOS: Metal → **device-wide** current allocated size.
//!
//! Use `detect_vram_best_effort()` to try the backends in a sensible order
//! and get a `VideoRamInfo { byte, source, scope }` back.
//!
//! ### Example
//! ```ignore
//! if let Some(info) = v_ram_detector::detect_vram_best_effort() {
//!     println!("V-RAM: {} bytes ({} / {})", info.bytes, info.source, info.scope);
//! } else {
//!     println!("No V-RAM backend available – consider using an estimate.");
//! }
//! ```

/// Information about a V-RAM reading.
#[derive(Debug, Clone, Copy)]
pub struct VideoRamInfo {
    /// Bytes reported by the backend.
    pub bytes: u64,
    /// Backend name, e.g. "NVML", "DXGI", "Metal".
    pub source: &'static str,
    /// Scope of the reading, e.g. "per-process", "adapter-wide", "device-wide".
    pub scope: &'static str,
}

/// Information about a GPU load reading.
#[derive(Debug, Clone, Copy)]
pub struct GpuLoadInfo {
    /// Utilization in percent (0.0 - 100.0).
    pub percent: f32,
    /// Backend name, e.g. "amdgpu-sysfs".
    pub source: &'static str,
    /// Scope of the reading, e.g. "device-wide".
    pub scope: &'static str,
}

/// Information about a GPU clock reading.
#[derive(Debug, Clone, Copy)]
pub struct GpuClockInfo {
    /// Effective GPU clock in Hz.
    pub hz: u64,
    /// Backend name, e.g. "amdgpu-sysfs".
    pub source: &'static str,
    /// Scope of the reading, e.g. "device-wide".
    pub scope: &'static str,
}

/// Try platform/vendor-specific backends in a sensible order and return the first hit.
///
/// Order of preference:
/// 1. NVML (NVIDIA, per-process)
/// 2. Linux AMD debugfs (per-process)
/// 3. Linux DRM fdinfo (per-process)
/// 4. Linux AMD sysfs (device-wide)
/// 5. DXGI (Windows adapter-wide; works for AMD & NVIDIA)
/// 6. Metal (macOS device-wide)
pub fn detect_v_ram_best_effort() -> Option<VideoRamInfo> {
    // 1) NVIDIA per-process via NVML
    if let Some(bytes) = query_vram_bytes_nvml_this_process() {
        return Some(VideoRamInfo {
            bytes,
            source: "NVML",
            scope: "per-process",
        });
    }

    #[cfg(target_os = "linux")]
    if let Some(bytes) = query_vram_bytes_linux_amdgpu_per_process(std::process::id()) {
        return Some(VideoRamInfo {
            bytes,
            source: "amdgpu-debugfs",
            scope: "per-process",
        });
    }

    #[cfg(target_os = "linux")]
    if let Some(bytes) = query_vram_bytes_linux_drm_fdinfo_this_process() {
        return Some(VideoRamInfo {
            bytes,
            source: "linux-drm-fdinfo",
            scope: "per-process",
        });
    }

    // 2) Linux AMD
    #[cfg(target_os = "linux")]
    if let Some(bytes) = query_vram_bytes_linux_drm_amdgpu() {
        return Some(VideoRamInfo {
            bytes,
            source: "Linux DRM",
            scope: "device-wide",
        });
    }

    // 2) Windows adapter-wide via DXGI (covers AMD & NVIDIA)
    if let Some(bytes) = query_vram_bytes_dxgi_adapter_current_usage() {
        return Some(VideoRamInfo {
            bytes,
            source: "DXGI",
            scope: "adapter-wide",
        });
    }

    // 3) macOS device-wide via Metal
    if let Some(bytes) = query_vram_bytes_metal_device_allocated() {
        return Some(VideoRamInfo {
            bytes,
            source: "Metal",
            scope: "device-wide",
        });
    }

    None
}

/// Try platform/vendor-specific backends and return total physical VRAM bytes.
///
/// Best effort order:
/// 1. Linux AMDGPU sysfs total VRAM
/// 2. Windows DXGI dedicated adapter memory
pub fn detect_v_ram_total_best_effort() -> Option<u64> {
    #[cfg(target_os = "linux")]
    if let Some(bytes) = query_vram_total_bytes_linux_amdgpu() {
        return Some(bytes);
    }

    if let Some(bytes) = query_vram_total_bytes_dxgi_dedicated() {
        return Some(bytes);
    }

    None
}

/// Try platform/vendor-specific backends in a sensible order and return the first hit.
///
/// Current best-effort order:
/// 1. Linux AMDGPU `gpu_busy_percent` (sysfs)
/// 2. Linux AMDGPU `amdgpu_pm_info` (debugfs)
pub fn detect_gpu_load_best_effort() -> Option<GpuLoadInfo> {
    #[cfg(target_os = "linux")]
    if let Some(percent) = query_gpu_load_percent_linux_amdgpu_sysfs() {
        return Some(GpuLoadInfo {
            percent,
            source: "amdgpu-sysfs",
            scope: "device-wide",
        });
    }

    #[cfg(target_os = "linux")]
    if let Some(percent) = query_gpu_load_percent_linux_amdgpu_debugfs() {
        return Some(GpuLoadInfo {
            percent,
            source: "amdgpu-debugfs",
            scope: "device-wide",
        });
    }

    None
}

/// Try platform/vendor-specific GPU clock backends and return the first hit.
///
/// Current best-effort order:
/// 1. Linux AMDGPU sysfs (`gpu_freq`, `pp_dpm_sclk`)
pub fn detect_gpu_clock_best_effort() -> Option<GpuClockInfo> {
    #[cfg(target_os = "linux")]
    if let Some(hz) = query_gpu_clock_hz_linux_amdgpu_sysfs() {
        if hz > 0 {
            return Some(GpuClockInfo {
                hz,
                source: "amdgpu-sysfs",
                scope: "device-wide",
            });
        }
    }

    None
}

/* ======================== NVIDIA: NVML (per-process) ======================== */

/// Query V-RAM bytes for the current process via NVIDIA NVML (if available).
///
/// Requires feature `vram_nvml`. Returns `Some(bytes)` on success.
#[cfg(feature = "vram_nvml")]
pub fn query_vram_bytes_nvml_this_process() -> Option<u64> {
    query_vram_bytes_nvml_for_pid(std::process::id())
}

/// Stub when `vram_nvml` feature is disabled.
#[cfg(not(feature = "vram_nvml"))]
pub fn query_vram_bytes_nvml_this_process() -> Option<u64> {
    None
}

/// Query VRAM bytes for a given PID via NVML (NVIDIA).
///
/// Requires feature `vram_nvml`. Returns `Some(bytes)` on success.
/// Falls back to scanning all devices and returning the **max** per-device value
/// for that PID (typical single-GPU setups).
#[cfg(feature = "vram_nvml")]
pub fn query_vram_bytes_nvml_for_pid(pid: u32) -> Option<u64> {
    use nvml_wrapper::Nvml;

    let nvml = Nvml::init().ok()?;
    let count = nvml.device_count().ok()?;
    let mut best: Option<u64> = None;

    for i in 0..count {
        let device = nvml.device_by_index(i).ok()?;

        // Prefer graphics; fall back to compute.
        let found = device
            .running_graphics_processes()
            .ok()
            .and_then(|list| find_bytes_for_pid(list, pid))
            .or_else(|| {
                device
                    .running_compute_processes()
                    .ok()
                    .and_then(|list| find_bytes_for_pid(list, pid))
            });

        if let Some(bytes) = found {
            best = Some(best.map_or(bytes, |b| b.max(bytes)));
        }
    }

    best
}

/// Finds bytes for pid for the `utils::v_ram_utils` module.
#[cfg(feature = "vram_nvml")]
fn find_bytes_for_pid(
    list: Vec<nvml_wrapper::struct_wrappers::device::ProcessInfo>,
    pid: u32,
) -> Option<u64> {
    use nvml_wrapper::enums::device::UsedGpuMemory;

    for p in list {
        if (p.pid) == pid {
            if let UsedGpuMemory::Used(bytes) = p.used_gpu_memory {
                return Some(bytes);
            }
        }
    }
    None
}

/// Runs the `query_vram_bytes_linux_drm_amdgpu` routine for query vram bytes linux drm amdgpu in the `utils::v_ram_utils` module.
#[cfg(target_os = "linux")]
pub fn query_vram_bytes_linux_drm_amdgpu() -> Option<u64> {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Reads u64 any for the `utils::v_ram_utils` module.
    fn read_u64_any(path: &Path) -> Option<u64> {
        let s = fs::read_to_string(path).ok()?;
        let t = s.trim();
        if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).ok()
        } else {
            t.parse::<u64>().ok()
        }
    }

    /// Checks whether amdgpu in the `utils::v_ram_utils` module.
    fn is_amdgpu(dev_dir: &Path) -> bool {
        if let Ok(link) = fs::read_link(dev_dir.join("driver")) {
            if link.file_name().map(|n| n == "amdgpu").unwrap_or(false) {
                return true;
            }
        }
        // Fallback über Vendor-ID (0x1002)
        read_u64_any(&dev_dir.join("vendor")).map_or(false, |v| v == 0x1002)
    }

    let mut best: Option<u64> = None;
    let drm_path = Path::new("/sys/class/drm");
    let entries = fs::read_dir(drm_path).ok()?;

    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || !name[4..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let dev_dir: PathBuf = e.path().join("device");
        if !is_amdgpu(&dev_dir) {
            continue;
        }

        let used = read_u64_any(&dev_dir.join("mem_info_vram_used"))
            .or_else(|| read_u64_any(&dev_dir.join("mem_info_vis_vram_used")));

        if let Some(bytes) = used {
            best = Some(best.map_or(bytes, |b| b.max(bytes)));
        }
    }

    best
}

/// Query total VRAM bytes on Linux for AMD GPUs via sysfs.
#[cfg(target_os = "linux")]
pub fn query_vram_total_bytes_linux_amdgpu() -> Option<u64> {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn read_u64_any(path: &Path) -> Option<u64> {
        let s = fs::read_to_string(path).ok()?;
        let t = s.trim();
        if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).ok()
        } else {
            t.parse::<u64>().ok()
        }
    }

    fn is_amdgpu(dev_dir: &Path) -> bool {
        if let Ok(link) = fs::read_link(dev_dir.join("driver"))
            && link.file_name().map(|n| n == "amdgpu").unwrap_or(false)
        {
            return true;
        }
        read_u64_any(&dev_dir.join("vendor")) == Some(0x1002)
    }

    let drm_path = Path::new("/sys/class/drm");
    let entries = fs::read_dir(drm_path).ok()?;
    let mut best: Option<u64> = None;

    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || !name[4..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let dev_dir: PathBuf = e.path().join("device");
        if !is_amdgpu(&dev_dir) {
            continue;
        }

        let total = read_u64_any(&dev_dir.join("mem_info_vram_total"))
            .or_else(|| read_u64_any(&dev_dir.join("mem_info_vis_vram_total")));
        if let Some(bytes) = total {
            best = Some(best.map_or(bytes, |curr| curr.max(bytes)));
        }
    }

    best
}

/// Stub when not on Linux.
#[cfg(not(target_os = "linux"))]
pub fn query_vram_total_bytes_linux_amdgpu() -> Option<u64> {
    None
}

/// Runs the `query_vram_bytes_linux_amdgpu_per_process` routine for query vram bytes linux amdgpu per process in the `utils::v_ram_utils` module.
#[cfg(target_os = "linux")]
pub fn query_vram_bytes_linux_amdgpu_per_process(pid: u32) -> Option<u64> {
    use std::fs;
    use std::path::Path;

    let root = Path::new("/sys/kernel/debug/dri");
    let entries = fs::read_dir(root).ok()?;

    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }

        let vm_info = p.join("amdgpu_vm_info");
        if vm_info.exists() {
            if let Some(b) = parse_amdgpu_vm_info_for_pid(&vm_info, pid) {
                return Some(b);
            }
        }

        let gem_info = p.join("amdgpu_gem_info");
        if gem_info.exists() {
            if let Some(b) = parse_amdgpu_gem_info_for_pid(&gem_info, pid) {
                return Some(b);
            }
        }
    }

    None
}

/// Query per-process VRAM from DRM fdinfo for the current process.
///
/// Uses `/proc/<pid>/fdinfo/*` lines such as:
/// - `drm-memory-vram: 123456 KiB`
/// - `drm-resident-vram: 123456 KiB`
#[cfg(target_os = "linux")]
pub fn query_vram_bytes_linux_drm_fdinfo_this_process() -> Option<u64> {
    query_vram_bytes_linux_drm_fdinfo_for_pid(std::process::id())
}

/// Query per-process VRAM from DRM fdinfo for a specific process id.
#[cfg(target_os = "linux")]
pub fn query_vram_bytes_linux_drm_fdinfo_for_pid(pid: u32) -> Option<u64> {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    let root = PathBuf::from(format!("/proc/{pid}/fdinfo"));
    let entries = fs::read_dir(root).ok()?;
    let mut per_client_best: HashMap<String, u64> = HashMap::new();

    for entry in entries.flatten() {
        let content = match fs::read_to_string(entry.path()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if !content.lines().any(|line| {
            line.trim_start()
                .to_ascii_lowercase()
                .starts_with("drm-driver:")
        }) {
            continue;
        }

        let mut client_id: Option<String> = None;
        let mut pdev: Option<String> = None;
        let mut fd_vram: Option<u64> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_ascii_lowercase();

            if lower.starts_with("drm-client-id:") {
                client_id = trimmed
                    .split_once(':')
                    .map(|(_, value)| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                continue;
            }

            if lower.starts_with("drm-pdev:") {
                pdev = trimmed
                    .split_once(':')
                    .map(|(_, value)| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                continue;
            }

            if !(lower.starts_with("drm-memory-vram:") || lower.starts_with("drm-resident-vram:")) {
                continue;
            }

            let Some(raw_value) = trimmed.split_once(':').map(|(_, value)| value.trim()) else {
                continue;
            };
            let Some(bytes) = parse_vram_bytes_token(raw_value) else {
                continue;
            };

            fd_vram = Some(fd_vram.map_or(bytes, |curr| curr.max(bytes)));
        }

        let Some(bytes) = fd_vram else {
            continue;
        };

        let key = format!(
            "{}:{}",
            pdev.unwrap_or_else(|| "unknown-pdev".to_string()),
            client_id.unwrap_or_else(|| "unknown-client".to_string())
        );
        per_client_best
            .entry(key)
            .and_modify(|curr| *curr = (*curr).max(bytes))
            .or_insert(bytes);
    }

    if per_client_best.is_empty() {
        None
    } else {
        Some(per_client_best.values().copied().sum())
    }
}

/// Query AMD GPU load percentage from sysfs.
///
/// Uses `/sys/class/drm/card*/device/gpu_busy_percent` and picks the maximum value.
#[cfg(target_os = "linux")]
pub fn query_gpu_load_percent_linux_amdgpu_sysfs() -> Option<f32> {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn read_u64_any(path: &Path) -> Option<u64> {
        let s = fs::read_to_string(path).ok()?;
        let t = s.trim();
        if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).ok()
        } else {
            t.parse::<u64>().ok()
        }
    }

    fn is_amdgpu(dev_dir: &Path) -> bool {
        if let Ok(link) = fs::read_link(dev_dir.join("driver"))
            && link.file_name().map(|n| n == "amdgpu").unwrap_or(false)
        {
            return true;
        }
        read_u64_any(&dev_dir.join("vendor")) == Some(0x1002)
    }

    let drm_path = Path::new("/sys/class/drm");
    let entries = fs::read_dir(drm_path).ok()?;
    let mut best: Option<f32> = None;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || !name[4..].chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }

        let dev_dir: PathBuf = entry.path().join("device");
        if !is_amdgpu(&dev_dir) {
            continue;
        }

        let busy_file = dev_dir.join("gpu_busy_percent");
        let Some(raw) = read_u64_any(&busy_file) else {
            continue;
        };
        let percent = (raw as f32).clamp(0.0, 100.0);
        best = Some(best.map_or(percent, |curr| curr.max(percent)));
    }

    best
}

/// Query AMD GPU load percentage from debugfs.
///
/// Parses `/sys/kernel/debug/dri/*/amdgpu_pm_info` lines like `GPU load: 23 %`.
#[cfg(target_os = "linux")]
pub fn query_gpu_load_percent_linux_amdgpu_debugfs() -> Option<f32> {
    use std::fs;
    use std::path::Path;

    let root = Path::new("/sys/kernel/debug/dri");
    let entries = fs::read_dir(root).ok()?;
    let mut best: Option<f32> = None;

    for entry in entries.flatten() {
        let pm_info = entry.path().join("amdgpu_pm_info");
        let Ok(content) = fs::read_to_string(pm_info) else {
            continue;
        };

        for line in content.lines() {
            let lower = line.to_ascii_lowercase();
            if !lower.contains("gpu load") {
                continue;
            }
            if let Some(percent) = parse_percent_from_line(line) {
                best = Some(best.map_or(percent, |curr| curr.max(percent)));
            }
        }
    }

    best
}

#[cfg(target_os = "linux")]
fn parse_percent_from_line(line: &str) -> Option<f32> {
    let mut value = String::new();
    for ch in line.chars() {
        if ch.is_ascii_digit() {
            value.push(ch);
        } else if !value.is_empty() {
            break;
        }
    }
    if value.is_empty() {
        return None;
    }
    value.parse::<f32>().ok().map(|v| v.clamp(0.0, 100.0))
}

/// Query AMD GPU core clock from sysfs.
///
/// Tries `gpu_freq` first (raw numeric value), then `pp_dpm_sclk` (active `*` line).
#[cfg(target_os = "linux")]
pub fn query_gpu_clock_hz_linux_amdgpu_sysfs() -> Option<u64> {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn read_u64_any(path: &Path) -> Option<u64> {
        let s = fs::read_to_string(path).ok()?;
        let t = s.trim();
        if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).ok()
        } else {
            t.parse::<u64>().ok()
        }
    }

    fn is_amdgpu(dev_dir: &Path) -> bool {
        if let Ok(link) = fs::read_link(dev_dir.join("driver"))
            && link.file_name().map(|n| n == "amdgpu").unwrap_or(false)
        {
            return true;
        }
        read_u64_any(&dev_dir.join("vendor")) == Some(0x1002)
    }

    fn normalize_raw_gpu_freq_to_hz(raw: u64) -> u64 {
        if raw >= 50_000_000 {
            // Already Hz.
            raw
        } else if raw >= 50_000 {
            // Likely kHz.
            raw.saturating_mul(1_000)
        } else {
            // Likely MHz.
            raw.saturating_mul(1_000_000)
        }
    }

    let drm_path = Path::new("/sys/class/drm");
    let entries = fs::read_dir(drm_path).ok()?;
    let mut best_hz: Option<u64> = None;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || !name[4..].chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }

        let dev_dir: PathBuf = entry.path().join("device");
        if !is_amdgpu(&dev_dir) {
            continue;
        }

        let mut card_hz: Option<u64> = None;

        if let Some(raw) = read_u64_any(&dev_dir.join("gpu_freq")) {
            card_hz = Some(normalize_raw_gpu_freq_to_hz(raw));
        }

        if card_hz.is_none()
            && let Ok(dpm) = fs::read_to_string(dev_dir.join("pp_dpm_sclk"))
        {
            let mut active_hz: Option<u64> = None;
            let mut max_hz: Option<u64> = None;
            for line in dpm.lines() {
                let Some(hz) = parse_frequency_hz_from_line(line) else {
                    continue;
                };
                max_hz = Some(max_hz.map_or(hz, |curr| curr.max(hz)));
                if line.contains('*') {
                    active_hz = Some(hz);
                }
            }
            card_hz = active_hz.or(max_hz);
        }

        if let Some(hz) = card_hz {
            best_hz = Some(best_hz.map_or(hz, |curr| curr.max(hz)));
        }
    }

    best_hz
}

/// Query AMD GPU core clock from debugfs (`amdgpu_pm_info`).
#[cfg(target_os = "linux")]
pub fn query_gpu_clock_hz_linux_amdgpu_debugfs() -> Option<u64> {
    use std::fs;
    use std::path::Path;

    let root = Path::new("/sys/kernel/debug/dri");
    let entries = fs::read_dir(root).ok()?;
    let mut best_hz: Option<u64> = None;

    for entry in entries.flatten() {
        let pm_info = entry.path().join("amdgpu_pm_info");
        let Ok(content) = fs::read_to_string(pm_info) else {
            continue;
        };

        for line in content.lines() {
            let lower = line.to_ascii_lowercase();
            if lower.contains("mclk") || lower.contains("memclk") || lower.contains("memory") {
                continue;
            }
            if !(lower.contains("sclk")
                || lower.contains("gfx")
                || lower.contains("gpu clock")
                || lower.contains("gpuclk"))
            {
                continue;
            }
            if let Some(hz) = parse_frequency_hz_from_line(line) {
                best_hz = Some(best_hz.map_or(hz, |curr| curr.max(hz)));
            }
        }
    }

    best_hz
}

#[cfg(target_os = "linux")]
fn parse_frequency_hz_from_line(line: &str) -> Option<u64> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }

        let start = i;
        let mut seen_dot = false;
        i += 1;
        while i < chars.len() {
            let ch = chars[i];
            if ch.is_ascii_digit() {
                i += 1;
                continue;
            }
            if ch == '.' && !seen_dot {
                seen_dot = true;
                i += 1;
                continue;
            }
            break;
        }

        let value: f64 = chars[start..i].iter().collect::<String>().parse().ok()?;

        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ':' || chars[i] == '=') {
            i += 1;
        }

        let unit_start = i;
        while i < chars.len() && chars[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit = chars[unit_start..i]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();

        let multiplier = if unit.starts_with("ghz") || unit == "g" {
            1_000_000_000.0
        } else if unit.starts_with("mhz") || unit == "m" {
            1_000_000.0
        } else if unit.starts_with("khz") || unit == "k" {
            1_000.0
        } else {
            continue;
        };

        return Some((value * multiplier).round() as u64);
    }

    None
}

/// Parses amdgpu vm info for pid for the `utils::v_ram_utils` module.
#[cfg(target_os = "linux")]
fn parse_amdgpu_vm_info_for_pid(path: &std::path::Path, pid: u32) -> Option<u64> {
    use std::fs;
    let s = fs::read_to_string(path).ok()?;
    for block in s.split("\n\n") {
        if !block.to_ascii_lowercase().contains(&format!("pid {}", pid)) {
            continue;
        }
        if let Some(bytes) = extract_vram_bytes_from_text(block) {
            return Some(bytes);
        }
    }
    None
}

/// Parses amdgpu gem info for pid for the `utils::v_ram_utils` module.
#[cfg(target_os = "linux")]
fn parse_amdgpu_gem_info_for_pid(path: &std::path::Path, pid: u32) -> Option<u64> {
    use std::fs;
    let s = fs::read_to_string(path).ok()?;
    for line in s.lines() {
        if !line.to_ascii_lowercase().contains(&format!("pid {}", pid)) {
            continue;
        }
        if let Some(bytes) = extract_vram_bytes_from_text(line) {
            return Some(bytes);
        }
    }
    None
}

/// Runs the `extract_vram_bytes_from_text` routine for extract vram bytes from text in the `utils::v_ram_utils` module.
#[cfg(target_os = "linux")]
fn extract_vram_bytes_from_text(txt: &str) -> Option<u64> {
    let lower = txt.to_ascii_lowercase();
    let mut it = lower.split_whitespace().peekable();

    while let Some(tok) = it.next() {
        if tok.contains("vram") {
            for _ in 0..3 {
                if let Some(next) = it.peek().cloned() {
                    if let Some(bytes) = parse_number_with_unit(next) {
                        return Some(bytes);
                    }
                    if let Some(bytes) = parse_embedded_number_with_unit(next) {
                        return Some(bytes);
                    }
                    it.next();
                }
            }
        }
    }
    None
}

/// Parses number with unit for the `utils::v_ram_utils` module.
#[cfg(target_os = "linux")]
fn parse_number_with_unit(token: &str) -> Option<u64> {
    // "1234", "1234kb", "1234kib", "1234MB", "1234MiB"
    let t = token.trim_matches(|c: char| c == ':' || c == '=');
    parse_embedded_number_with_unit(t)
}

/// Parses embedded number with unit for the `utils::v_ram_utils` module.
#[cfg(target_os = "linux")]
fn parse_embedded_number_with_unit(t: &str) -> Option<u64> {
    let mut digits = String::new();
    let mut unit = String::new();
    for ch in t.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            unit.push(ch);
        }
    }
    if digits.is_empty() {
        return None;
    }
    let val: u64 = digits.parse().ok()?;
    let u = unit.to_ascii_lowercase();

    let mul: u64 = if u.is_empty() || u == "b" {
        1
    } else if u == "k" || u == "kb" || u == "kib" {
        1024
    } else if u == "m" || u == "mb" || u == "mib" {
        1024 * 1024
    } else if u == "g" || u == "gb" || u == "gib" {
        1024 * 1024 * 1024
    } else {
        1
    };
    Some(val.saturating_mul(mul))
}

#[cfg(target_os = "linux")]
fn parse_vram_bytes_token(value: &str) -> Option<u64> {
    // Normalize "123 KiB" to "123KiB" and parse common unit suffixes.
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    parse_embedded_number_with_unit(compact.trim_matches(|ch: char| ch == ':' || ch == '='))
}

/// Runs the `query_vram_bytes_linux_drm_amdgpu` routine for query vram bytes linux drm amdgpu in the `utils::v_ram_utils` module.
#[cfg(not(target_os = "linux"))]
pub fn query_vram_bytes_linux_drm_amdgpu() -> Option<u64> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn query_gpu_load_percent_linux_amdgpu_sysfs() -> Option<f32> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn query_gpu_load_percent_linux_amdgpu_debugfs() -> Option<f32> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn query_gpu_clock_hz_linux_amdgpu_sysfs() -> Option<u64> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn query_gpu_clock_hz_linux_amdgpu_debugfs() -> Option<u64> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn query_vram_bytes_linux_drm_fdinfo_this_process() -> Option<u64> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn query_vram_bytes_linux_drm_fdinfo_for_pid(_pid: u32) -> Option<u64> {
    None
}

/// Stub when `vram_nvml` feature is disabled.
#[cfg(not(feature = "vram_nvml"))]
pub fn query_vram_bytes_nvml_for_pid(_pid: u32) -> Option<u64> {
    None
}

/* ========================= Windows: DXGI (adapter) ========================= */

/// Query adapter-local VRAM "CurrentUsage" via DXGI (Windows).
///
/// - Works for **AMD & NVIDIA** (and others) on Windows.
/// - Scope is **adapter-wide** (not per-process).
/// - Requires feature `vram_dxgi` and the `windows` crate with DXGI features.
///
/// Returns `Some(bytes)` on success. Chooses the first enumerated adapter.
#[cfg(all(windows, feature = "vram_dxgi"))]
pub fn query_vram_bytes_dxgi_adapter_current_usage() -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
        DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter3, IDXGIFactory4,
    };
    use windows::core::Interface;

    unsafe {
        let factory: IDXGIFactory4 =
            CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0)).ok()?;

        let mut index: u32 = 0;
        loop {
            let adapter = match factory.EnumAdapters1(index) {
                Ok(a) => a,
                Err(_) => break,
            };
            index += 1;

            if let Ok(adapter3) = adapter.cast::<IDXGIAdapter3>() {
                let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                if adapter3
                    .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
                    .is_ok()
                {
                    return Some(info.CurrentUsage as u64);
                }
            }
        }
    }

    None
}

/// Stub when not on Windows or feature disabled.
#[cfg(not(all(windows, feature = "vram_dxgi")))]
pub fn query_vram_bytes_dxgi_adapter_current_usage() -> Option<u64> {
    None
}

/// Query dedicated adapter VRAM total bytes via DXGI (Windows).
#[cfg(all(windows, feature = "vram_dxgi"))]
pub fn query_vram_total_bytes_dxgi_dedicated() -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIFactory4,
    };

    unsafe {
        let factory: IDXGIFactory4 =
            CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0)).ok()?;
        let adapter = factory.EnumAdapters1(0).ok()?;
        let desc = adapter.GetDesc1().ok()?;
        let total = desc.DedicatedVideoMemory as u64;
        (total > 0).then_some(total)
    }
}

/// Stub when not on Windows or feature disabled.
#[cfg(not(all(windows, feature = "vram_dxgi")))]
pub fn query_vram_total_bytes_dxgi_dedicated() -> Option<u64> {
    None
}

/* ============================ macOS: Metal (GPU) =========================== */

/// Query device-wide allocated bytes from Metal (macOS).
///
/// - Scope is **device-wide** (not per-process).
/// - Requires feature `vram_metal`.
#[cfg(all(target_os = "macos", feature = "vram_metal"))]
pub fn query_vram_bytes_metal_device_allocated() -> Option<u64> {
    let device = metal::Device::system_default()?;
    Some(device.current_allocated_size())
}

/// Stub when not on macOS or feature disabled.
#[cfg(not(all(target_os = "macos", feature = "vram_metal")))]
pub fn query_vram_bytes_metal_device_allocated() -> Option<u64> {
    None
}

/* ========================== Utility / Presentation ========================= */

/// Human-readable formatter for bytes (MiB/GiB).
pub fn fmt_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GB", b / GIB)
    } else {
        format!("{:.0} MB", b / MIB)
    }
}

/// Human-readable formatter for frequency (MHz/GHz).
pub fn fmt_hz(hz: u64) -> String {
    const MHZ: f64 = 1_000_000.0;
    const GHZ: f64 = 1_000_000_000.0;
    let h = hz as f64;
    if h >= GHZ {
        format!("{:.2} GHz", h / GHZ)
    } else {
        format!("{:.0} MHz", h / MHZ)
    }
}
