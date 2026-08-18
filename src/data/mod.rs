#[cfg(test)]
mod sysinfo_mock;
#[cfg(not(test))]
use sysinfo::Components;
#[cfg(test)]
use sysinfo_mock::{Component, Components};

use std::{
    cell::LazyCell,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use backoff::{ExponentialBackoff, backoff::Backoff};
use rustix::ioctl;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Networks, RefreshKind, System};

use crate::{
    config::SysInfoConfig,
    template::{Requires, Variable},
};

const IP_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

enum IpVersion {
    V4,
    V6,
}

pub(crate) struct Disk {
    disks: sysinfo::Disks,

    pub(crate) read: Option<f64>,
    pub(crate) write: Option<f64>,
}

impl Disk {
    fn new() -> Self {
        Self {
            read: None,
            write: None,
            disks: sysinfo::Disks::new_with_refreshed_list(),
        }
    }

    fn refresh_disks(&mut self) {
        self.disks.refresh(true);
        let mut seen = HashSet::new();
        let (mut read, mut write) = (0u64, 0u64);

        for disk in self.disks.list() {
            if !seen.insert(disk.name()) {
                continue;
            }

            read += disk.usage().read_bytes;
            write += disk.usage().written_bytes;
        }

        self.read = Some(read as f64 / 1_000_000.0);
        self.write = Some(write as f64 / 1_000_000.0);
    }
}

pub(crate) struct Npu {
    last_busy_time: Option<u64>,
    last_busy_time_diff: Option<u64>,
    max_busy_time_diff: Option<u64>,
    max_busy_time: Option<u64>,

    pub(crate) usage: Option<u64>,
    pub(crate) frequency: Option<u64>,
}

impl Npu {
    fn new() -> Self {
        Self {
            last_busy_time: None,
            last_busy_time_diff: None,
            max_busy_time_diff: None,
            max_busy_time: None,
            usage: None,
            frequency: None,
        }
    }

    fn refresh_usage(&mut self, current_read_us: Option<u64>) {
        if let Some(current_busy_time) = current_read_us {
            match self.last_busy_time {
                Some(last_busy_time) => {
                    let current_diff = current_busy_time - last_busy_time;

                    if let Some(last_diff) = self.last_busy_time_diff {
                        match self.max_busy_time_diff {
                            Some(max_diff) => {
                                if current_diff > max_diff && current_diff > 0 {
                                    self.max_busy_time_diff = Some(current_diff)
                                }
                                let usage_percentage = (last_diff * 100) / max_diff;
                                self.usage = Some(usage_percentage)
                            }
                            None => {
                                if current_diff > 0 {
                                    self.max_busy_time_diff = Some(current_diff)
                                } else {
                                    self.usage = Some(0)
                                }
                            }
                        }
                    }

                    self.last_busy_time_diff = Some(current_diff);
                }
                None => {
                    self.max_busy_time = Some(current_busy_time);
                    self.usage = Some(0)
                }
            }

            self.last_busy_time = Some(current_busy_time);
        }
    }

    fn refresh_frequency(&mut self, current_read: Option<u64>) {
        if let Some(current_npu_frequency) = current_read {
            self.frequency = Some(current_npu_frequency)
        }
    }
}

#[derive(Default)]
struct Vram {
    used: Option<u64>,
    total: Option<u64>,
}

impl Vram {
    fn usage(&self) -> Option<u64> {
        let used = self.used?;
        let total = self.total?;
        (total > 0).then(|| used * 100 / total)
    }
}

#[derive(Default)]
struct NvidiaSmi {
    temp: Option<f32>,
    usage: Option<u64>,
    vram: Vram,
}

pub(crate) struct Gpu {
    // sampling state for `refresh_xe_usage`
    last_sample_at: Option<Instant>,
    last_idle_ms: HashMap<PathBuf, u64>,

    pub(crate) temp: Option<f32>,
    pub(crate) usage: Option<u64>,
    pub(crate) vram_usage: Option<u64>,
}

impl Gpu {
    fn new() -> Self {
        Self {
            last_sample_at: None,
            last_idle_ms: HashMap::new(),
            temp: None,
            usage: None,
            vram_usage: None,
        }
    }

    fn refresh(&mut self, components: &Components, requires: Requires) {
        let needs_temp = requires.contains(Variable::GpuTemp);
        let needs_usage = requires.contains(Variable::GpuUsage);
        let needs_vram = requires.contains(Variable::VramUsage);

        if !needs_temp && !needs_usage && !needs_vram {
            self.temp = None;
            self.usage = None;
            self.vram_usage = None;
            return;
        }

        // lazy nvidia-smi: spawned at most once
        let nvidia = LazyCell::new(Self::query_nvidia_smi);

        self.temp = if needs_temp {
            Self::find_temp(components).or_else(|| nvidia.as_ref().and_then(|n| n.temp))
        } else {
            None
        };
        self.usage = if needs_usage {
            Self::find_usage_sysfs()
                .or_else(|| self.refresh_xe_usage())
                .or_else(|| nvidia.as_ref().and_then(|n| n.usage))
        } else {
            None
        };
        self.vram_usage = if needs_vram {
            Self::find_vram_usage_sysfs()
                .or_else(Self::query_xe_vram)
                .or_else(|| nvidia.as_ref().and_then(|n| n.vram.usage()))
        } else {
            None
        };
    }

    fn find_temp(components: &Components) -> Option<f32> {
        const LABELS: [&str; 10] = [
            "amdgpu", "radeon", "nouveau", "nvidia", "gpu", "edge", "junction", "pkg", "vram",
            "mem",
        ];
        LABELS.into_iter().find_map(|l| {
            components
                .iter()
                .find(|c| c.label().to_lowercase().contains(l))
                .and_then(|c| c.temperature())
        })
    }

    fn find_usage_sysfs() -> Option<u64> {
        let entries = fs::read_dir("/sys/class/drm").ok()?;
        for entry in entries.flatten() {
            if let Ok(contents) = fs::read_to_string(entry.path().join("device/gpu_busy_percent"))
                && let Ok(value) = contents.trim().parse()
            {
                return Some(value);
            }
        }
        None
    }

    /// VRAM usage percentage from the amdgpu sysfs interface.
    fn find_vram_usage_sysfs() -> Option<u64> {
        let entries = fs::read_dir("/sys/class/drm").ok()?;
        for entry in entries.flatten() {
            let device = entry.path().join("device");
            let read = |name: &str| {
                fs::read_to_string(device.join(name))
                    .ok()
                    .and_then(|c| c.trim().parse::<u64>().ok())
            };
            if let Some(used) = read("mem_info_vram_used")
                && let Some(total) = read("mem_info_vram_total")
                && total > 0
            {
                return Some(used * 100 / total);
            }
        }
        None
    }

    /// VRAM usage percentage for the Intel `xe` driver, from the
    /// `DRM_XE_DEVICE_QUERY_MEM_REGIONS` ioctl on the first render node
    /// reporting a VRAM region.
    fn query_xe_vram() -> Option<u64> {
        let entries = fs::read_dir("/dev/dri").ok()?;
        entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("renderD"))
            .find_map(|e| Self::query_xe_vram_device(&e.path()))
    }

    /// Ask one render node for its memory regions and sum the VRAM ones.
    ///
    /// The xe driver has no sysfs equivalent of amdgpu's `mem_info_vram_*`;
    /// its only unprivileged interface for this is the device query ioctl,
    /// and no crate currently wraps the xe uAPI (the `drm`/`drm-ffi` crates
    /// cover core DRM only). All constants and layouts below mirror the
    /// kernel's `include/uapi/drm/xe_drm.h`.
    fn query_xe_vram_device(path: &Path) -> Option<u64> {
        // mirror of `struct drm_xe_device_query` (the ioctl argument)
        #[repr(C)]
        #[derive(Default)]
        struct DrmXeDeviceQuery {
            extensions: u64,
            query: u32,
            size: u32,
            data: u64,
            reserved: [u64; 2],
        }

        // DRM_IOWR(DRM_COMMAND_BASE + DRM_XE_DEVICE_QUERY, struct drm_xe_device_query)
        const DRM_IOCTL_XE_DEVICE_QUERY: ioctl::Opcode =
            ioctl::opcode::read_write::<DrmXeDeviceQuery>(b'd', 0x40);
        // `enum drm_xe_device_query_type`: which query the ioctl runs
        const DRM_XE_DEVICE_QUERY_MEM_REGIONS: u32 = 1;
        // `enum drm_xe_mem_region_class`: dedicated VRAM (SYSMEM is 0)
        const DRM_XE_MEM_REGION_CLASS_VRAM: u16 = 1;
        // reply layout: a `struct drm_xe_query_mem_regions` header
        // { u32 num_mem_regions; u32 pad; }, then `num_mem_regions`
        // consecutive 88-byte `struct drm_xe_mem_region` entries
        const REGIONS_OFFSET: usize = 8;
        const REGION_SIZE: usize = 88;

        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .ok()?;

        // The reply is variable-length (one entry per memory region), and the
        // kernel rejects any `size` other than 0 or the exact required value.
        // The query becomes a two-call pattern: with `size = 0` the kernel only
        // fills in the required size, then a second call fetches the data.
        // On render nodes not driven by xe the first call fails.

        let mut query = DrmXeDeviceQuery {
            query: DRM_XE_DEVICE_QUERY_MEM_REGIONS,
            ..Default::default()
        };

        // SAFETY: the opcode is built from `DrmXeDeviceQuery` above, so it
        // matches the type the kernel reads and writes through the pointer
        unsafe {
            ioctl::ioctl(
                &file,
                ioctl::Updater::<DRM_IOCTL_XE_DEVICE_QUERY, _>::new(&mut query),
            )
        }
        .ok()?;

        let mut buf = vec![0u8; query.size as usize];
        query.data = buf.as_mut_ptr() as u64;

        // SAFETY: as above; `buf` stays alive and writable for the whole
        // call and is as large as `query.size` promises
        unsafe {
            ioctl::ioctl(
                &file,
                ioctl::Updater::<DRM_IOCTL_XE_DEVICE_QUERY, _>::new(&mut query),
            )
        }
        .ok()?;

        let field_u64 = |base: usize, offset: usize| {
            Some(u64::from_ne_bytes(
                buf.get(base + offset..base + offset + 8)?.try_into().ok()?,
            ))
        };

        let num_regions = u64::from(u32::from_ne_bytes(buf.get(0..4)?.try_into().ok()?));
        let (mut used, mut total) = (0u64, 0u64);
        for i in 0..num_regions as usize {
            let base = REGIONS_OFFSET + i * REGION_SIZE;
            // field offsets within `struct drm_xe_mem_region`:
            // mem_class at 0, total_size at 8, used at 16
            let class = u16::from_ne_bytes(buf.get(base..base + 2)?.try_into().ok()?);
            if class == DRM_XE_MEM_REGION_CLASS_VRAM {
                total += field_u64(base, 8)?; // total_size
                used += field_u64(base, 16)?; // used
            }
        }

        (total > 0).then(|| used * 100 / total)
    }

    /// Usage of the busiest GT for the Intel `xe` driver, derived from C6
    /// idle residency: busy ≈ 100 - idle_time_delta / wall_time_delta.
    fn refresh_xe_usage(&mut self) -> Option<u64> {
        let now = Instant::now();
        let samples = Self::read_idle_residency_ms();

        let usage = self.last_sample_at.and_then(|last| {
            let wall_ms = u64::try_from(now.duration_since(last).as_millis()).ok()?;
            if wall_ms == 0 {
                return None;
            }
            samples
                .iter()
                .filter_map(|(path, idle_ms)| {
                    let previous = self.last_idle_ms.get(path)?;
                    let idle_delta = idle_ms.saturating_sub(*previous);
                    Some(100 - (idle_delta * 100 / wall_ms).min(100))
                })
                .max()
        });

        self.last_sample_at = Some(now);
        self.last_idle_ms = samples;

        usage
    }

    /// Collect `idle_residency_ms` for every GT of every `xe` card
    /// (`/sys/class/drm/card*/device/tile*/gt*/gtidle`).
    fn read_idle_residency_ms() -> HashMap<PathBuf, u64> {
        let mut samples = HashMap::new();
        let Ok(cards) = fs::read_dir("/sys/class/drm") else {
            return samples;
        };

        let subdirs = |path: PathBuf, prefix: &'static str| {
            fs::read_dir(path)
                .into_iter()
                .flatten()
                .flatten()
                .filter(move |e| e.file_name().to_string_lossy().starts_with(prefix))
        };

        for card in cards.flatten() {
            for tile in subdirs(card.path().join("device"), "tile") {
                for gt in subdirs(tile.path(), "gt") {
                    let path = gt.path().join("gtidle/idle_residency_ms");
                    if let Ok(contents) = fs::read_to_string(&path)
                        && let Ok(value) = contents.trim().parse()
                    {
                        samples.insert(path, value);
                    }
                }
            }
        }

        samples
    }

    fn query_nvidia_smi() -> Option<NvidiaSmi> {
        let is_flatpak = Path::new("/.flatpak-info").exists();

        let mut command = if is_flatpak {
            let mut c = Command::new("flatpak-spawn");
            c.args(["--host", "nvidia-smi"]);
            c
        } else {
            Command::new("nvidia-smi")
        };

        let output = command
            .args([
                "--query-gpu=temperature.gpu,utilization.gpu,memory.used,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut fields = stdout.trim().split(", ");

        Some(NvidiaSmi {
            temp: fields.next().and_then(|s| s.trim().parse().ok()),
            usage: fields.next().and_then(|s| s.trim().parse().ok()),
            vram: Vram {
                used: fields.next().and_then(|s| s.trim().parse().ok()),
                total: fields.next().and_then(|s| s.trim().parse().ok()),
            },
        })
    }
}

/// The data coming from various sources (mostly the `sysinfo` crate)
///
/// Manages each source, and stores the values extracted from them
pub(crate) struct Data {
    system: System,
    networks: Networks,
    components: Components,
    physical_interfaces: Vec<String>,
    last_interface_scan: Instant,
    next_ip_fetch: Instant,
    ip_backoff: ExponentialBackoff,

    pub(crate) npu: Npu,
    pub(crate) gpu: Gpu,
    pub(crate) cpu_usage: Option<f32>,
    pub(crate) ram_usage: Option<u64>,
    pub(crate) download_speed: Option<f64>,
    pub(crate) upload_speed: Option<f64>,
    pub(crate) cpu_temp: Option<f32>,
    pub(crate) public_ipv4: Option<String>,
    pub(crate) public_ipv6: Option<String>,
    pub(crate) disks: Disk,
}

impl Data {
    pub(crate) fn new(config: &SysInfoConfig) -> Self {
        let system = System::new_with_specifics(RefreshKind::nothing());
        let networks = Networks::new_with_refreshed_list();
        let components = Components::new_with_refreshed_list();
        let physical_interfaces = Self::detect_physical_interfaces(config);
        let npu_data = Npu::new();
        let disks_data = Disk::new();

        let ip_backoff = ExponentialBackoff {
            max_interval: IP_REFRESH_INTERVAL,
            multiplier: 2.0,
            max_elapsed_time: None,
            ..ExponentialBackoff::default()
        };

        Self {
            system,
            networks,
            components,
            physical_interfaces,
            last_interface_scan: Instant::now(),
            next_ip_fetch: Instant::now(), // triggers an immediate fetch on the first tick
            ip_backoff,
            cpu_usage: None,
            ram_usage: None,
            download_speed: None,
            upload_speed: None,
            cpu_temp: None,
            npu: npu_data,
            gpu: Gpu::new(),
            public_ipv4: None,
            public_ipv6: None,
            disks: disks_data,
        }
    }

    /// Refresh only the subsystems the current template actually uses.
    pub(crate) fn refresh(&mut self, requires: Requires, config: &SysInfoConfig) {
        let needs_cpu = requires.contains(Variable::CpuUsage);
        let needs_cpu_temp = requires.contains(Variable::CpuTemp);
        let needs_ram = requires.contains(Variable::RamUsage);
        let needs_download = requires.contains(Variable::DlSpeed);
        let needs_upload = requires.contains(Variable::UlSpeed);
        let needs_gpu_temp = requires.contains(Variable::GpuTemp);
        let needs_pub_ipv4 = requires.contains(Variable::PublicIpv4);
        let needs_pub_ipv6 = requires.contains(Variable::PublicIpv6);
        let needs_npu_usage = requires.contains(Variable::NpuUsage);
        let needs_npu_frequency = requires.contains(Variable::NpuFrequency);
        let needs_disk_read = requires.contains(Variable::DiskRead);
        let needs_disk_write = requires.contains(Variable::DiskWrite);

        if (needs_download || needs_upload)
            && self.last_interface_scan.elapsed() > Duration::from_secs(10)
        {
            self.physical_interfaces = Self::detect_physical_interfaces(config);
            self.last_interface_scan = Instant::now();
        }

        // Crate sysinfo system refresh
        let mut refresh = RefreshKind::nothing();
        if needs_cpu {
            refresh = refresh.with_cpu(CpuRefreshKind::nothing().with_cpu_usage());
        }
        if needs_ram {
            let mem = if config.include_swap_in_ram {
                MemoryRefreshKind::nothing().with_ram().with_swap()
            } else {
                MemoryRefreshKind::nothing().with_ram()
            };
            refresh = refresh.with_memory(mem);
        }

        self.system.refresh_specifics(refresh);

        // CPU
        self.cpu_usage = needs_cpu.then(|| self.system.global_cpu_usage());

        // RAM
        self.ram_usage = needs_ram.then(|| {
            if config.include_swap_in_ram {
                ((self.system.used_memory() + self.system.used_swap()) * 100)
                    / (self.system.total_memory() + self.system.total_swap())
            } else {
                (self.system.used_memory() * 100) / self.system.total_memory()
            }
        });

        // Network
        if needs_download || needs_upload {
            self.networks.refresh(true);
            let (mut up, mut down) = (0u64, 0u64);
            for (name, iface) in self.networks.iter() {
                if self.physical_interfaces.contains(name) {
                    up += iface.transmitted();
                    down += iface.received();
                }
            }
            self.download_speed = needs_download.then(|| down as f64 / 1_000_000.0);
            self.upload_speed = needs_upload.then(|| up as f64 / 1_000_000.0);
        } else {
            self.download_speed = None;
            self.upload_speed = None;
        }

        // Temperatures
        if needs_cpu_temp || needs_gpu_temp {
            self.components.refresh(true);
        }

        self.cpu_temp = if needs_cpu_temp {
            Self::find_cpu_temp(&self.components)
        } else {
            None
        };

        // GPU
        self.gpu.refresh(&self.components, requires);

        // Public IPs — exponential backoff on failure, 5-minute cadence on success.
        // Only refresh if:
        // - the template requires it, and
        // - either:
        //   - we have no value currently (e.g. due to a missing internet connection on the previous try)
        //   - it's time to refresh the value
        if needs_pub_ipv4 || needs_pub_ipv6 {
            let have_ipv4 = self.public_ipv4.is_some();
            let have_ipv6 = self.public_ipv6.is_some();
            let need_refresh = Instant::now() >= self.next_ip_fetch;
            let mut any_failed = false;
            let mut any_fetched = false;

            if needs_pub_ipv4 && (!have_ipv4 || need_refresh) {
                tracing::debug!("trying to refresh public IPv4");
                any_fetched = true;
                self.public_ipv4 = Self::fetch_public_ip(IpVersion::V4);
                if self.public_ipv4.is_none() {
                    tracing::warn!("failed to fetch IPv4");
                    any_failed = true;
                }
            }
            if needs_pub_ipv6 && (!have_ipv6 || need_refresh) {
                tracing::debug!("trying to refresh public IPv6");
                any_fetched = true;
                self.public_ipv6 = Self::fetch_public_ip(IpVersion::V6);
                if self.public_ipv6.is_none() {
                    tracing::warn!("failed to fetch IPv6");
                    any_failed = true;
                }
            }

            if any_fetched {
                if any_failed {
                    let delay = self
                        .ip_backoff
                        .next_backoff()
                        .unwrap_or(IP_REFRESH_INTERVAL);
                    tracing::trace!("IP fetch failed, retrying in {delay:?}");
                    self.next_ip_fetch = Instant::now() + delay;
                } else {
                    self.ip_backoff.reset();
                    tracing::trace!("IP fetch succeeded, next refresh in {IP_REFRESH_INTERVAL:?}");
                    self.next_ip_fetch = Instant::now() + IP_REFRESH_INTERVAL;
                }
            }
        }

        if !needs_pub_ipv4 {
            self.public_ipv4 = None;
        }
        if !needs_pub_ipv6 {
            self.public_ipv6 = None;
        }

        // Disk
        if needs_disk_read || needs_disk_write {
            self.disks.refresh_disks();
        }

        // NPU
        if needs_npu_usage {
            self.npu.refresh_usage(Self::find_npu_busy_time_us_sysfs());
        }

        if needs_npu_frequency {
            self.npu
                .refresh_frequency(Self::find_npu_frequency_mhz_sysfs());
        }
    }

    fn detect_physical_interfaces(config: &SysInfoConfig) -> Vec<String> {
        let mut interfaces = Vec::new();
        if let Ok(entries) = fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                let name = entry.file_name().into_string().unwrap_or_default();
                if Path::new(&format!("/sys/class/net/{name}/device")).exists() {
                    interfaces.push(name);
                }
            }
        }
        if let Some(inc) = &config.include_interfaces {
            interfaces.retain(|i| inc.contains(i));
        }
        if let Some(exc) = &config.exclude_interfaces {
            interfaces.retain(|i| !exc.contains(i));
        }
        interfaces
    }

    fn find_cpu_temp(components: &Components) -> Option<f32> {
        const LABELS: [&str; 10] = [
            "coretemp",
            "k10temp",
            "zenpower",
            "cpu_thermal",
            "soc_thermal",
            "cpu",
            "package",
            "tctl",
            "tdie",
            "core",
        ];
        LABELS.into_iter().find_map(|l| {
            components
                .iter()
                .find(|c| c.label().to_lowercase().contains(l))
                .and_then(|c| c.temperature())
        })
    }

    fn find_npu_busy_time_us_sysfs() -> Option<u64> {
        let entries = fs::read_dir("/sys/class/accel").ok()?;
        for entry in entries.flatten() {
            if let Ok(contents) = fs::read_to_string(entry.path().join("device/npu_busy_time_us"))
                && let Ok(value) = contents.trim().parse()
            {
                return Some(value);
            }
        }
        None
    }

    fn find_npu_frequency_mhz_sysfs() -> Option<u64> {
        let entries = fs::read_dir("/sys/class/accel").ok()?;
        for entry in entries.flatten() {
            if let Ok(contents) =
                fs::read_to_string(entry.path().join("device/npu_current_frequency_mhz"))
                && let Ok(value) = contents.trim().parse()
            {
                return Some(value);
            }
        }
        None
    }

    /// Fetch a public IP address from icanhazip.com.
    ///
    fn fetch_public_ip(version: IpVersion) -> Option<String> {
        // `attohttpc` cannot force a specific IP version on the resolver, so we
        // rely on the version-specific subdomains exposed by icanhazip.com.
        let url = match version {
            IpVersion::V4 => "https://ipv4.icanhazip.com",
            IpVersion::V6 => "https://ipv6.icanhazip.com",
        };

        let response = attohttpc::get(url)
            .timeout(Duration::from_secs(5))
            .send()
            .ok()?
            .error_for_status()
            .ok()?;
        let ip = response.text().ok()?.trim().to_string();

        (!ip.is_empty()).then_some(ip)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    mod find_cpu_temp {
        use super::*;

        #[test]
        fn inexact_match() {
            let components = Components::from(vec![Component {
                label: "k10temp Tctl",
                temperature: 1.0,
            }]);

            // do match on the component, even though `k10temp` is only a _part_
            // of its name
            assert_eq!(Data::find_cpu_temp(&components), Some(1.0));
        }

        #[test]
        fn priority() {
            let components = Components::from(vec![
                Component {
                    label: "k10temp Tctl",
                    temperature: 1.0,
                },
                Component {
                    label: "coretemp foo",
                    temperature: 2.0,
                },
            ]);

            // choose `coretemp` over `k10temp` despite `k10temp` coming earlier
            // in `components`, because `coretemp` comes earlier in `LABELS`
            assert_eq!(Data::find_cpu_temp(&components), Some(2.0));
        }
    }

    mod find_gpu_temp {
        use super::*;

        #[test]
        fn inexact_match() {
            let components = Components::from(vec![Component {
                label: "amdgpu foo",
                temperature: 1.0,
            }]);

            // do match on the component, even though `amdgpu` is only a _part_
            // of its name
            assert_eq!(Gpu::find_temp(&components), Some(1.0));
        }

        #[test]
        fn priority() {
            let components = Components::from(vec![
                Component {
                    label: "mem bar",
                    temperature: 1.0,
                },
                Component {
                    label: "junction foo",
                    temperature: 2.0,
                },
            ]);

            // choose `junction` over `mem` despite `mem` coming earlier
            // in `components`, because `junction` comes earlier in `LABELS`
            assert_eq!(Gpu::find_temp(&components), Some(2.0));
        }

        #[test]
        fn intel_xe() {
            let components = Components::from(vec![
                Component {
                    label: "xe vram",
                    temperature: 1.0,
                },
                Component {
                    label: "xe pkg",
                    temperature: 2.0,
                },
            ]);

            // prefer the xe die temperature `pkg` over the memory one `vram`
            assert_eq!(Gpu::find_temp(&components), Some(2.0));
        }
    }
}
