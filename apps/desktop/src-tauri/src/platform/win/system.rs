//! The computer's load for the `system.stats` widget call: processor and graphics usage from
//! Performance Data Helper counters (the numbers Task Manager shows), memory, battery, fixed
//! disks, and network rates from the hardware adapters' byte counters.

use std::collections::HashMap;
use std::time::Instant;

use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
    PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::SystemInformation::{GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::core::{PCWSTR, w};

use crate::system_stats::{Battery, Cpu, Disk, Gpu, Memory, Network, SystemStats};

/// Keeps percentages above 100 as they are; "% Processor Utility" goes past 100 on turbo boost.
const PDH_FMT_NOCAP100: u32 = 0x8000;
const DRIVE_FIXED: u32 = 3;
/// `BatteryFlag` value of a computer without a battery, and of an unknown state.
const NO_BATTERY: u8 = 128;
const UNKNOWN_BATTERY: u8 = 255;
const CHARGING: u8 = 8;

/// Counters and byte totals kept between samples, since rates and usages are differences.
pub struct SystemSampler {
    query: Option<Query>,
    cpu_name: String,
    network: Option<(Instant, u64, u64)>,
}

struct Query {
    handle: PDH_HQUERY,
    total: PDH_HCOUNTER,
    cores: PDH_HCOUNTER,
    gpu: Option<PDH_HCOUNTER>,
}

// SAFETY: PDH handles may be used from any thread; the sampler is kept behind a mutex.
unsafe impl Send for Query {}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: the handle came from PdhOpenQueryW and is closed once.
        unsafe { PdhCloseQuery(self.handle) };
    }
}

impl Query {
    fn open() -> Option<Self> {
        let mut handle = PDH_HQUERY::default();
        // SAFETY: a null data source means the live counters; `handle` is written on success.
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut handle) } != ERROR_SUCCESS.0 {
            return None;
        }
        let add = |path: PCWSTR| {
            let mut counter = PDH_HCOUNTER::default();
            // SAFETY: `handle` is an open query and `path` a static wide string.
            (unsafe { PdhAddEnglishCounterW(handle, path, 0, &mut counter) } == ERROR_SUCCESS.0).then_some(counter)
        };
        let total = add(w!("\\Processor Information(_Total)\\% Processor Utility"))
            .or_else(|| add(w!("\\Processor(_Total)\\% Processor Time")));
        let cores = add(w!("\\Processor Information(*)\\% Processor Utility"))
            .or_else(|| add(w!("\\Processor(*)\\% Processor Time")));
        let gpu = add(w!("\\GPU Engine(*)\\Utilization Percentage"));
        let query = Self { handle, total: total?, cores: cores?, gpu };
        // Rates need two collections; the first one only sets the starting point.
        // SAFETY: the query is open.
        unsafe { PdhCollectQueryData(handle) };
        Some(query)
    }

    fn value(&self, counter: PDH_HCOUNTER) -> Option<f64> {
        let mut value = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: `counter` belongs to this query and `value` is writable.
        let status = unsafe { PdhGetFormattedCounterValue(counter, format(), None, &mut value) };
        (status == ERROR_SUCCESS.0 && valid(value.CStatus)).then(|| unsafe { value.Anonymous.doubleValue })
    }

    /// Every instance of a wildcard counter with its value.
    fn values(&self, counter: PDH_HCOUNTER) -> Vec<(String, f64)> {
        let (mut size, mut count) = (0u32, 0u32);
        // SAFETY: without a buffer the call reports the size it needs.
        let status = unsafe { PdhGetFormattedCounterArrayW(counter, format(), &mut size, &mut count, None) };
        if status != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }
        // u64 elements keep the buffer aligned for the items at its start.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        // SAFETY: the buffer holds `size` bytes, as asked for above.
        let status = unsafe { PdhGetFormattedCounterArrayW(counter, format(), &mut size, &mut count, Some(items)) };
        if status != ERROR_SUCCESS.0 {
            return Vec::new();
        }
        // SAFETY: the call wrote `count` items, whose names point into the same buffer.
        let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
        items
            .iter()
            .filter(|item| valid(item.FmtValue.CStatus))
            .map(|item| {
                let name = unsafe { item.szName.to_string() }.unwrap_or_default();
                (name, unsafe { item.FmtValue.Anonymous.doubleValue })
            })
            .collect()
    }
}

fn format() -> PDH_FMT {
    PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100)
}

fn valid(status: u32) -> bool {
    status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
}

impl SystemSampler {
    pub fn new() -> Self {
        Self { query: Query::open(), cpu_name: cpu_name(), network: None }
    }

    pub fn sample(&mut self) -> Result<SystemStats, String> {
        if let Some(query) = &self.query {
            // SAFETY: the query is open.
            unsafe { PdhCollectQueryData(query.handle) };
        }
        let query = self.query.as_ref();
        let usage = query.and_then(|q| q.value(q.total)).unwrap_or(0.0);
        let cores = query.map(|q| core_usages(q.values(q.cores))).unwrap_or_default();
        let gpu = query.and_then(|q| q.gpu.map(|counter| gpu_usage(q.values(counter))));
        Ok(SystemStats {
            cpu: Cpu { usage: percent(usage), cores, name: self.cpu_name.clone() },
            memory: memory(),
            gpu: gpu.flatten().map(|usage| Gpu { usage: percent(usage) }),
            battery: battery(),
            disks: disks(),
            network: self.network_rates(),
            // SAFETY: no arguments.
            uptime_sec: unsafe { GetTickCount64() } / 1000,
        })
    }

    fn network_rates(&mut self) -> Network {
        let now = Instant::now();
        let Some((down, up)) = network_totals() else { return Network::default() };
        let rates = match self.network.replace((now, down, up)) {
            Some((then, last_down, last_up)) => {
                let seconds = now.duration_since(then).as_secs_f64().max(0.001);
                Network {
                    down_bytes_per_sec: (down.saturating_sub(last_down) as f64 / seconds) as u64,
                    up_bytes_per_sec: (up.saturating_sub(last_up) as f64 / seconds) as u64,
                }
            }
            None => Network::default(),
        };
        rates
    }
}

fn percent(value: f64) -> f64 {
    (value.clamp(0.0, 100.0) * 10.0).round() / 10.0
}

/// Per logical processor, in order. Instances are named `group,index`, plus totals per group
/// (`0,_Total`) and overall (`_Total`); the older Processor object names them by index alone.
fn core_usages(values: Vec<(String, f64)>) -> Vec<f64> {
    let mut cores: Vec<((u32, u32), f64)> = values
        .into_iter()
        .filter(|(name, _)| !name.contains("_Total"))
        .filter_map(|(name, value)| {
            let (group, index) = name.split_once(',').unwrap_or(("0", &name));
            Some(((group.parse().ok()?, index.parse().ok()?), percent(value)))
        })
        .collect();
    cores.sort_by_key(|(key, _)| *key);
    cores.into_iter().map(|(_, value)| value).collect()
}

/// Like Task Manager: the busiest engine, each engine summed over the processes using it.
/// Instances look like `pid_1234_luid_0x0_0x1_phys_0_eng_0_engtype_3D`.
fn gpu_usage(values: Vec<(String, f64)>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut engines: HashMap<String, f64> = HashMap::new();
    for (name, value) in values {
        let engine = name.find("_luid_").map_or(name.as_str(), |start| &name[start..]).to_string();
        *engines.entry(engine).or_default() += value;
    }
    engines.into_values().reduce(f64::max)
}

fn memory() -> Memory {
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `status` is a valid structure with its length set.
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return Memory::default();
    }
    Memory { total_bytes: status.ullTotalPhys, used_bytes: status.ullTotalPhys.saturating_sub(status.ullAvailPhys) }
}

fn battery() -> Option<Battery> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` is writable.
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
    if status.BatteryFlag & NO_BATTERY != 0 || status.BatteryFlag == UNKNOWN_BATTERY || status.BatteryLifePercent > 100 {
        return None;
    }
    Some(Battery {
        percent: f64::from(status.BatteryLifePercent),
        charging: status.BatteryFlag & CHARGING != 0,
        plugged_in: status.ACLineStatus == 1,
        seconds_left: (status.BatteryLifeTime != u32::MAX).then_some(u64::from(status.BatteryLifeTime)),
    })
}

/// Fixed drives with a letter, such as `C:`.
fn disks() -> Vec<Disk> {
    // SAFETY: no arguments.
    let letters = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|bit| letters & (1 << bit) != 0)
        .filter_map(|bit| {
            let mount = format!("{}:", char::from(b'A' + bit));
            let root: Vec<u16> = format!("{mount}\\").encode_utf16().chain([0]).collect();
            let root = PCWSTR(root.as_ptr());
            // SAFETY: `root` is a null-terminated path that outlives the calls.
            if unsafe { GetDriveTypeW(root) } != DRIVE_FIXED {
                return None;
            }
            let (mut total, mut free) = (0u64, 0u64);
            unsafe { GetDiskFreeSpaceExW(root, None, Some(&mut total), Some(&mut free)) }.ok()?;
            let mut label = [0u16; MAX_PATH as usize + 1];
            let label = match unsafe { GetVolumeInformationW(root, Some(&mut label), None, None, None, None) } {
                Ok(()) => String::from_utf16_lossy(&label[..label.iter().position(|&c| c == 0).unwrap_or(0)]),
                Err(_) => String::new(),
            };
            Some(Disk { mount, label, total_bytes: total, free_bytes: free })
        })
        .collect()
}

/// Bytes received and sent so far by the hardware adapters that are up. Virtual adapters, such
/// as a VPN's, carry the same traffic again and are left out.
fn network_totals() -> Option<(u64, u64)> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    // SAFETY: the call allocates the table, which is freed below.
    if unsafe { GetIfTable2(&mut table) } != ERROR_SUCCESS || table.is_null() {
        return None;
    }
    // SAFETY: the table holds `NumEntries` rows starting at `Table`.
    let rows = unsafe { std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize) };
    let (mut down, mut up) = (0u64, 0u64);
    for row in rows {
        let hardware = row.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
        if hardware && row.OperStatus == IfOperStatusUp {
            down += row.InOctets;
            up += row.OutOctets;
        }
    }
    // SAFETY: the table came from GetIfTable2.
    unsafe { FreeMibTable(table.cast()) };
    Some((down, up))
}

fn cpu_name() -> String {
    let mut buffer = [0u16; 256];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer holds `size` bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0"),
            w!("ProcessorNameString"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..buffer.iter().position(|&c| c == 0).unwrap_or(0)]).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_cores_and_skips_totals() {
        let values = [("0,10", 5.0), ("_Total", 50.0), ("0,2", 20.0), ("0,_Total", 40.0), ("1,0", 300.0)]
            .map(|(name, value)| (name.to_string(), value))
            .to_vec();
        assert_eq!(core_usages(values), [20.0, 5.0, 100.0]);
    }

    #[test]
    fn takes_the_busiest_engine_summed_over_processes() {
        let values = [
            ("pid_1_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 30.0),
            ("pid_2_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 25.0),
            ("pid_2_luid_0x0_0x1_phys_0_eng_3_engtype_VideoDecode", 40.0),
        ]
        .map(|(name, value)| (name.to_string(), value))
        .to_vec();
        assert_eq!(gpu_usage(values), Some(55.0));
        assert_eq!(gpu_usage(Vec::new()), None);
    }

    #[test]
    fn samples_this_computer() {
        let mut sampler = SystemSampler::new();
        let stats = sampler.sample().unwrap();
        assert!(stats.memory.total_bytes > 0 && stats.memory.used_bytes <= stats.memory.total_bytes);
        assert!(stats.disks.iter().any(|disk| disk.mount == "C:"));
        assert!(!stats.cpu.cores.is_empty());
    }
}
