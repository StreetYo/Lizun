//! The computer's load for widgets with the `system` permission (`system.stats` in the SDK):
//! processor, graphics, memory, battery, disks and network. The platform measures it; this
//! module keeps one sampler for all widgets and hands out the same sample for a short while, so
//! several widgets polling at once cost one measurement.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// How long a sample is handed out again instead of measuring anew.
const FRESH_FOR: Duration = Duration::from_millis(500);
pub const PERMISSION: &str = "system";

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    pub cpu: Cpu,
    pub memory: Memory,
    /// `None` when the graphics counters are unavailable.
    pub gpu: Option<Gpu>,
    /// `None` on a computer without a battery.
    pub battery: Option<Battery>,
    pub disks: Vec<Disk>,
    pub network: Network,
    pub uptime_sec: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cpu {
    /// 0–100, over all processors since the previous sample.
    pub usage: f64,
    /// 0–100 for each logical processor.
    pub cores: Vec<f64>,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub total_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    /// 0–100, the busiest engine.
    pub usage: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: f64,
    pub charging: bool,
    pub plugged_in: bool,
    pub seconds_left: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    /// Such as `C:`.
    pub mount: String,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub down_bytes_per_sec: u64,
    pub up_bytes_per_sec: u64,
}

#[derive(Default)]
pub struct Sampler(Mutex<Option<(crate::platform::SystemSampler, Option<(Instant, SystemStats)>)>>);

#[tauri::command]
pub async fn widget_system_stats(app: AppHandle, widget_id: String) -> Result<SystemStats, String> {
    let manifest = crate::widgets::find(&app, &widget_id).ok_or("unknown widget")?;
    if !manifest.permissions.iter().any(|p| p == PERMISSION) {
        return Err(format!("system.stats requires the \"{PERMISSION}\" permission in manifest.json"));
    }
    // Opening the counters and reading them take a few milliseconds; not on the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Sampler>();
        let mut slot = state.0.lock().unwrap_or_else(|e| e.into_inner());
        let (sampler, last) = slot.get_or_insert_with(|| (crate::platform::SystemSampler::new(), None));
        if let Some((at, stats)) = last
            && at.elapsed() < FRESH_FOR
        {
            return Ok(stats.clone());
        }
        let stats = sampler.sample()?;
        *last = Some((Instant::now(), stats.clone()));
        Ok(stats)
    })
    .await
    .map_err(|err| err.to_string())?
}
