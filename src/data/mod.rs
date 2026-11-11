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
    pub(crate) used: Option<u64>,
    pub(crate) usage: Option<u64>,
}

impl Disk {
    fn new() -> Self {
        Self {
            read: None,
            write: None,
            used: None,
            usage: None,
            disks: sysinfo::Disks::new_with_refreshed_list()
                .expect("failed to initialise disk list"),
        }
    }

    fn refresh_disks(&mut self) {
        self.disks.refresh(true);
        let mut seen = HashSet::new();
        let (mut read, mut write) = (0u64, 0u64);
        let (mut total, mut available) = (0, 0);

        for disk in &self.disks {
            if !seen.insert(disk.name()) {
                continue;
            }

            read += disk.usage().read_bytes;
            write += disk.usage().written_bytes;
            total += disk.total_space();
            available += disk.available_space();
        }

        let used = total - available;

        self.read = Some(read as f64 / 1_000_000.0);
        self.write = Some(write as f64 / 1_000_000.0);
        self.used = Some(used / 1_000_000_000);
        self.usage = Some(100 * used / total);
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
    vram_usage: Option<u64>,
}

impl NvidiaSmi {
    /// What to report for a runtime-suspended GPU: it is idle and holds
    /// nothing in VRAM, but its temperature is unknown without waking it.
    fn suspended() -> Self {
        Self {
            temp: None,
            usage: Some(0),
            vram_usage: Some(0),
        }
    }
}

const PCI_VENDOR_AMD: u16 = 0x1002;
const PCI_VENDOR_INTEL: u16 = 0x8086;
const PCI_VENDOR_NVIDIA: u16 = 0x10de;

/// A GPU as listed in `/sys/class/drm` (`card0`, `card1`, …).
struct DrmCard {
    vendor: u16,
    /// The `device` directory of the card, holding its sysfs attributes
    device: PathBuf,
}

impl DrmCard {
    fn scan() -> Vec<Self> {
        Self::scan_in(Path::new("/sys/class/drm"))
    }

    /// List the cards in `drm_dir`, skipping connectors (`card1-eDP-1`) and
    /// render nodes, which share the device of their card.
    fn scan_in(drm_dir: &Path) -> Vec<Self> {
        let Ok(entries) = fs::read_dir(drm_dir) else {
            return Vec::new();
        };

        let mut cards: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|name| name.strip_prefix("card"))
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
            .filter_map(|e| {
                let device = e.path().join("device");
                let vendor = fs::read_to_string(device.join("vendor")).ok()?;
                let vendor =
                    u16::from_str_radix(vendor.trim().trim_start_matches("0x"), 16).ok()?;
                Some(Self { vendor, device })
            })
            .collect();
        cards.sort_by(|a, b| a.device.cmp(&b.device));
        cards
    }

    fn read_u64(&self, attribute: &str) -> Option<u64> {
        fs::read_to_string(self.device.join(attribute))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// `gpu_busy_percent`, exposed by amdgpu
    fn busy_percent(&self) -> Option<u64> {
        self.read_u64("gpu_busy_percent")
    }

    /// VRAM usage percentage from the amdgpu `mem_info_vram_*` attributes
    fn vram_usage(&self) -> Option<u64> {
        Vram {
            used: self.read_u64("mem_info_vram_used"),
            total: self.read_u64("mem_info_vram_total"),
        }
        .usage()
    }

    /// Hottest sensor of the card's own hwmon device, in °C
    fn hottest_temp(&self) -> Option<f32> {
        fs::read_dir(self.device.join("hwmon"))
            .ok()?
            .flatten()
            .flat_map(|hwmon| fs::read_dir(hwmon.path()).into_iter().flatten().flatten())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("temp") && n.ends_with("_input"))
            })
            .filter_map(|e| {
                fs::read_to_string(e.path())
                    .ok()?
                    .trim()
                    .parse::<f32>()
                    .ok()
            })
            .map(|millidegrees| millidegrees / 1000.0)
            .max_by(f32::total_cmp)
    }

    /// Whether runtime power management has put the GPU to sleep. Reading
    /// this does not wake the GPU, unlike querying it through its driver.
    fn is_runtime_suspended(&self) -> bool {
        fs::read_to_string(self.device.join("power/runtime_status"))
            .is_ok_and(|status| status.trim() == "suspended")
    }
}

/// Metrics of the GPUs of a single vendor. With several GPUs of the same
/// vendor, the busiest / hottest one is reported.
#[derive(Default)]
pub(crate) struct GpuStats {
    pub(crate) temp: Option<f32>,
    pub(crate) usage: Option<u64>,
    pub(crate) vram_usage: Option<u64>,
}

pub(crate) struct Gpu {
    // Sampling state for `refresh_xe_usage`
    last_sample_at: Option<Instant>,
    last_idle_ms: HashMap<PathBuf, u64>,

    pub(crate) temp: Option<f32>,
    pub(crate) usage: Option<u64>,
    pub(crate) vram_usage: Option<u64>,

    pub(crate) amd: GpuStats,
    pub(crate) intel: GpuStats,
    pub(crate) nvidia: GpuStats,
}

impl Gpu {
    fn new() -> Self {
        Self {
            last_sample_at: None,
            last_idle_ms: HashMap::new(),
            temp: None,
            usage: None,
            vram_usage: None,
            amd: GpuStats::default(),
            intel: GpuStats::default(),
            nvidia: GpuStats::default(),
        }
    }

    fn refresh(&mut self, components: &Components, requires: Requires) {
        let needs = |var| requires.contains(var);
        let needs_temp = needs(Variable::GpuTemp);
        let needs_usage = needs(Variable::GpuUsage);
        let needs_vram = needs(Variable::VramUsage);
        let needs_amd = [
            Variable::AmdGpuTemp,
            Variable::AmdGpuUsage,
            Variable::AmdVramUsage,
        ]
        .into_iter()
        .any(needs);
        let needs_intel = [
            Variable::IntelGpuTemp,
            Variable::IntelGpuUsage,
            Variable::IntelVramUsage,
        ]
        .into_iter()
        .any(needs);
        let needs_nvidia = [
            Variable::NvidiaGpuTemp,
            Variable::NvidiaGpuUsage,
            Variable::NvidiaVramUsage,
        ]
        .into_iter()
        .any(needs);

        if !(needs_temp || needs_usage || needs_vram || needs_amd || needs_intel || needs_nvidia) {
            self.temp = None;
            self.usage = None;
            self.vram_usage = None;
            self.amd = GpuStats::default();
            self.intel = GpuStats::default();
            self.nvidia = GpuStats::default();
            return;
        }

        let cards = DrmCard::scan();
        let of_vendor = |vendor| cards.iter().filter(move |c| c.vendor == vendor);

        // Lazy nvidia-smi: spawned at most once
        let nvidia = LazyCell::new(|| Self::query_nvidia(&cards));

        // Sampled once per refresh, as it is computed from the time elapsed
        // since the previous sample
        let xe_usage = if needs_usage || needs(Variable::IntelGpuUsage) {
            self.refresh_xe_usage()
        } else {
            None
        };

        self.temp = if needs_temp {
            Self::find_temp(components).or_else(|| nvidia.as_ref().and_then(|n| n.temp))
        } else {
            None
        };
        self.usage = if needs_usage {
            cards
                .iter()
                .find_map(DrmCard::busy_percent)
                .or(xe_usage)
                .or_else(|| nvidia.as_ref().and_then(|n| n.usage))
        } else {
            None
        };
        self.vram_usage = if needs_vram {
            cards
                .iter()
                .find_map(DrmCard::vram_usage)
                .or_else(Self::query_xe_vram)
                .or_else(|| nvidia.as_ref().and_then(|n| n.vram_usage))
        } else {
            None
        };

        self.amd = GpuStats {
            temp: needs(Variable::AmdGpuTemp)
                .then(|| {
                    of_vendor(PCI_VENDOR_AMD)
                        .filter_map(DrmCard::hottest_temp)
                        .max_by(f32::total_cmp)
                })
                .flatten(),
            usage: needs(Variable::AmdGpuUsage)
                .then(|| {
                    of_vendor(PCI_VENDOR_AMD)
                        .filter_map(DrmCard::busy_percent)
                        .max()
                })
                .flatten(),
            vram_usage: needs(Variable::AmdVramUsage)
                .then(|| {
                    of_vendor(PCI_VENDOR_AMD)
                        .filter_map(DrmCard::vram_usage)
                        .max()
                })
                .flatten(),
        };

        self.intel = GpuStats {
            temp: needs(Variable::IntelGpuTemp)
                .then(|| {
                    of_vendor(PCI_VENDOR_INTEL)
                        .filter_map(DrmCard::hottest_temp)
                        .max_by(f32::total_cmp)
                })
                .flatten(),
            // the idle residency counters are only exposed by Intel's xe driver
            usage: needs(Variable::IntelGpuUsage).then_some(xe_usage).flatten(),
            vram_usage: needs(Variable::IntelVramUsage)
                .then(Self::query_xe_vram)
                .flatten(),
        };

        self.nvidia = if needs_nvidia {
            let n = nvidia.as_ref();
            GpuStats {
                temp: needs(Variable::NvidiaGpuTemp)
                    .then(|| n.and_then(|n| n.temp))
                    .flatten(),
                usage: needs(Variable::NvidiaGpuUsage)
                    .then(|| n.and_then(|n| n.usage))
                    .flatten(),
                vram_usage: needs(Variable::NvidiaVramUsage)
                    .then(|| n.and_then(|n| n.vram_usage))
                    .flatten(),
            }
        } else {
            GpuStats::default()
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
                .filter(|c| c.label().to_lowercase().contains(l))
                .filter_map(|c| c.temperature())
                .max_by(f32::total_cmp)
        })
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

    /// Query NVIDIA GPUs through `nvidia-smi`, unless they are all
    /// runtime-suspended: `nvidia-smi` would wake them up, and a laptop's
    /// discrete GPU would then never go back to sleep while the applet runs.
    fn query_nvidia(cards: &[DrmCard]) -> Option<NvidiaSmi> {
        if Self::all_nvidia_suspended(cards) {
            return Some(NvidiaSmi::suspended());
        }
        Self::query_nvidia_smi()
    }

    /// Whether there is at least one NVIDIA GPU and every one of them is
    /// runtime-suspended.
    fn all_nvidia_suspended<'a>(cards: impl IntoIterator<Item = &'a DrmCard>) -> bool {
        let mut nvidia = cards
            .into_iter()
            .filter(|c| c.vendor == PCI_VENDOR_NVIDIA)
            .peekable();
        nvidia.peek().is_some() && nvidia.all(DrmCard::is_runtime_suspended)
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
        Self::parse_nvidia_smi(&String::from_utf8_lossy(&output.stdout))
    }

    /// Parse the output of the `nvidia-smi` query, which has one line per GPU.
    /// Only the first GPU is reported.
    fn parse_nvidia_smi(stdout: &str) -> Option<NvidiaSmi> {
        let mut fields = stdout.lines().next()?.split(',').map(str::trim);
        let mut next = || fields.next().and_then(|s| s.parse().ok());

        Some(NvidiaSmi {
            temp: next().map(|t: u64| t as f32),
            usage: next(),
            vram_usage: Vram {
                used: next(),
                total: next(),
            }
            .usage(),
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
        let system = System::new_with_specifics(RefreshKind::nothing())
            .expect("failed to initialise sysinfo");
        let networks = Networks::new_with_refreshed_list().expect("failed to list networks");
        let components = Components::new_with_refreshed_list().expect("failed to list components");
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
        let needs_disk_used = requires.contains(Variable::DiskUsed);
        let needs_disk_usage = requires.contains(Variable::DiskUsage);

        if (needs_download || needs_upload)
            && self.last_interface_scan.elapsed() > Duration::from_secs(10)
        {
            self.physical_interfaces = Self::detect_physical_interfaces(config);
            self.last_interface_scan = Instant::now();
        }

        // Crate sysinfo system refresh
        let mut refresh = RefreshKind::nothing();

        if needs_cpu {
            refresh = refresh.with_cpu(CpuRefreshKind::nothing().with_usage());
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
                if self.physical_interfaces.iter().any(|p| p.as_str() == name) {
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
        if needs_disk_read || needs_disk_write || needs_disk_used || needs_disk_usage {
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
                .filter(|c| c.label().to_lowercase().contains(l))
                .filter_map(|c| c.temperature())
                .max_by(f32::total_cmp)
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

            // Do match on the component, even though `k10temp` is only a _part_ of its name
            assert_eq!(Data::find_cpu_temp(&components), Some(1.0));
        }

        #[test]
        fn hottest_sensor_wins() {
            let components = Components::from(vec![
                Component {
                    label: "coretemp Core 0",
                    temperature: 1.0,
                },
                Component {
                    label: "coretemp Core 1",
                    temperature: 3.0,
                },
                Component {
                    label: "coretemp Package id 0",
                    temperature: 2.0,
                },
            ]);

            // Report the hotspot (max of the device's sensors), not whichever
            // sensor is enumerated first
            assert_eq!(Data::find_cpu_temp(&components), Some(3.0));
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

            // Do match on the component, even though `amdgpu` is only a _part_ of its name
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

            // Choose `junction` over `mem` despite `mem` coming earlier
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

            // Prefer the xe die temperature `pkg` over the memory one `vram`
            assert_eq!(Gpu::find_temp(&components), Some(2.0));
        }
    }

    /// A throwaway directory mimicking `/sys/class/drm`
    struct FakeDrm(PathBuf);

    impl FakeDrm {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("sysinfo-applet-test-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// Write `contents` to `card/device/attribute`, creating directories
        fn write(&self, card: &str, attribute: &str, contents: &str) -> &Self {
            let path = self.0.join(card).join("device").join(attribute);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
            self
        }
    }

    impl Drop for FakeDrm {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    mod drm_card {
        use super::*;

        #[test]
        fn scan_skips_connectors_and_render_nodes() {
            let drm = FakeDrm::new("scan");
            drm.write("card0", "vendor", "0x10de\n")
                .write("card0-eDP-1", "vendor", "0x10de\n")
                .write("card1", "vendor", "0x1002\n")
                .write("renderD128", "vendor", "0x10de\n");
            // a card whose device has no vendor attribute is ignored
            fs::create_dir_all(drm.0.join("card2/device")).unwrap();

            let vendors: Vec<_> = DrmCard::scan_in(&drm.0).iter().map(|c| c.vendor).collect();
            assert_eq!(vendors, [PCI_VENDOR_NVIDIA, PCI_VENDOR_AMD]);
        }

        #[test]
        fn amdgpu_attributes() {
            let drm = FakeDrm::new("amdgpu");
            drm.write("card1", "vendor", "0x1002\n")
                .write("card1", "gpu_busy_percent", "37\n")
                .write("card1", "mem_info_vram_used", "256\n")
                .write("card1", "mem_info_vram_total", "1024\n")
                .write("card1", "hwmon/hwmon4/temp1_input", "62000\n")
                .write("card1", "hwmon/hwmon4/temp2_input", "71500\n")
                .write("card1", "hwmon/hwmon4/temp1_label", "edge\n");

            let cards = DrmCard::scan_in(&drm.0);
            let [card] = cards.as_slice() else {
                panic!("expected exactly one card");
            };
            assert_eq!(card.busy_percent(), Some(37));
            assert_eq!(card.vram_usage(), Some(25));
            // the hotspot is reported, not the first sensor
            assert_eq!(card.hottest_temp(), Some(71.5));
            assert!(!card.is_runtime_suspended());
        }

        #[test]
        fn runtime_suspended() {
            let drm = FakeDrm::new("suspended");
            drm.write("card0", "vendor", "0x10de\n").write(
                "card0",
                "power/runtime_status",
                "suspended\n",
            );

            let cards = DrmCard::scan_in(&drm.0);
            assert!(cards[0].is_runtime_suspended());

            // a suspended GPU is reported idle instead of being woken up
            let stats = Gpu::query_nvidia(&cards).unwrap();
            assert_eq!(stats.usage, Some(0));
            assert_eq!(stats.vram_usage, Some(0));
            assert_eq!(stats.temp, None);
        }

        #[test]
        fn nvidia_suspension() {
            let drm = FakeDrm::new("nvidia-suspension");
            drm.write("card0", "vendor", "0x1002\n")
                .write("card0", "power/runtime_status", "suspended\n")
                .write("card1", "vendor", "0x10de\n")
                .write("card1", "power/runtime_status", "active\n")
                .write("card2", "vendor", "0x10de\n")
                .write("card2", "power/runtime_status", "suspended\n");
            let cards = DrmCard::scan_in(&drm.0);
            let [amd, nv_active, nv_suspended] = cards.as_slice() else {
                panic!("expected three cards");
            };
            let suspended = |cards: &[&DrmCard]| Gpu::all_nvidia_suspended(cards.iter().copied());

            // no NVIDIA GPU at all: nothing to skip
            assert!(!suspended(&[]));
            assert!(!suspended(&[amd]));
            // a suspended AMD GPU does not matter
            assert!(!suspended(&[amd, nv_active]));
            // a single awake NVIDIA GPU means querying nvidia-smi
            assert!(!suspended(&[nv_active, nv_suspended]));
            assert!(suspended(&[amd, nv_suspended]));
        }

        #[test]
        fn missing_attributes() {
            let drm = FakeDrm::new("missing");
            drm.write("card0", "vendor", "0x8086\n")
                .write("card0", "mem_info_vram_used", "0\n")
                .write("card0", "mem_info_vram_total", "0\n");

            let cards = DrmCard::scan_in(&drm.0);
            assert_eq!(cards[0].vendor, PCI_VENDOR_INTEL);
            assert_eq!(cards[0].busy_percent(), None);
            // a zero total is not a division by zero
            assert_eq!(cards[0].vram_usage(), None);
            assert_eq!(cards[0].hottest_temp(), None);
            // without runtime PM attributes the GPU is considered awake
            assert!(!cards[0].is_runtime_suspended());
        }

        #[test]
        fn scan_missing_directory() {
            let drm = FakeDrm::new("missing-dir");
            assert!(DrmCard::scan_in(&drm.0.join("nope")).is_empty());
        }
    }

    mod parse_nvidia_smi {
        use super::*;

        #[test]
        fn single_gpu() {
            let stats = Gpu::parse_nvidia_smi("58, 41, 1024, 4096\n").unwrap();
            assert_eq!(stats.temp, Some(58.0));
            assert_eq!(stats.usage, Some(41));
            assert_eq!(stats.vram_usage, Some(25));
        }

        #[test]
        fn first_of_several_gpus() {
            let stats = Gpu::parse_nvidia_smi("58, 41, 1024, 4096\n30, 0, 0, 8192\n").unwrap();
            assert_eq!(stats.usage, Some(41));
            assert_eq!(stats.vram_usage, Some(25));
        }

        #[test]
        fn unavailable_fields() {
            let stats = Gpu::parse_nvidia_smi("[N/A], 12, [N/A], [N/A]\n").unwrap();
            assert_eq!(stats.temp, None);
            assert_eq!(stats.usage, Some(12));
            assert_eq!(stats.vram_usage, None);
        }
    }
}
