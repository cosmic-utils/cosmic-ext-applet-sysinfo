use std::{
    cell::LazyCell,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use backoff::{ExponentialBackoff, backoff::Backoff};
use sysinfo::{Components, CpuRefreshKind, MemoryRefreshKind, Networks, RefreshKind, System};

use crate::{
    config::SysInfoConfig,
    template::{Requires, Variable},
};

const IP_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

enum IpVersion {
    V4,
    V6,
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

    pub(crate) cpu_usage: Option<f32>,
    pub(crate) ram_usage: Option<u64>,
    pub(crate) download_speed: Option<f64>,
    pub(crate) upload_speed: Option<f64>,
    pub(crate) cpu_temp: Option<f32>,
    pub(crate) gpu_temp: Option<f32>,
    pub(crate) gpu_usage: Option<u64>,
    pub(crate) public_ipv4: Option<String>,
    pub(crate) public_ipv6: Option<String>,
}

impl Data {
    pub(crate) fn new(config: &SysInfoConfig) -> Self {
        let system = System::new_with_specifics(RefreshKind::nothing());
        let networks = Networks::new_with_refreshed_list();
        let components = Components::new_with_refreshed_list();
        let physical_interfaces = Self::detect_physical_interfaces(config);

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
            gpu_temp: None,
            gpu_usage: None,
            public_ipv4: None,
            public_ipv6: None,
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
        let needs_gpu_usage = requires.contains(Variable::GpuUsage);
        let needs_pub_ipv4 = requires.contains(Variable::PublicIpv4);
        let needs_pub_ipv6 = requires.contains(Variable::PublicIpv6);

        if (needs_download || needs_upload)
            && self.last_interface_scan.elapsed() > Duration::from_secs(10)
        {
            self.physical_interfaces = Self::detect_physical_interfaces(config);
            self.last_interface_scan = Instant::now();
        }

        // sysinfo system refresh
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

        // cpu
        self.cpu_usage = needs_cpu.then(|| self.system.global_cpu_usage());

        // ram
        self.ram_usage = needs_ram.then(|| {
            if config.include_swap_in_ram {
                ((self.system.used_memory() + self.system.used_swap()) * 100)
                    / (self.system.total_memory() + self.system.total_swap())
            } else {
                (self.system.used_memory() * 100) / self.system.total_memory()
            }
        });

        // network
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

        // temperatures
        if needs_cpu_temp || needs_gpu_temp {
            self.components.refresh(true);
        }

        self.cpu_temp = if needs_cpu_temp {
            Self::find_cpu_temp(&self.components)
        } else {
            None
        };

        // gpu (lazy nvidia-smi)
        if needs_gpu_temp || needs_gpu_usage {
            let nvidia = LazyCell::new(Self::query_nvidia_smi);

            self.gpu_temp = if needs_gpu_temp {
                Self::find_gpu_temp(&self.components)
                    .or_else(|| nvidia.as_ref().and_then(|(t, _)| *t))
            } else {
                None
            };
            self.gpu_usage = if needs_gpu_usage {
                Self::find_gpu_usage_sysfs().or_else(|| nvidia.as_ref().and_then(|(_, u)| *u))
            } else {
                None
            };
        } else {
            self.gpu_temp = None;
            self.gpu_usage = None;
        }

        // public IPs — exponential backoff on failure, 5-minute cadence on success
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

        if !needs_pub_ipv4 {
            self.public_ipv4 = None;
        }
        if !needs_pub_ipv6 {
            self.public_ipv6 = None;
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
        const LABELS: &[&str] = &[
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
        components
            .iter()
            .find(|c| {
                let l = c.label().to_lowercase();
                LABELS.iter().any(|k| l.contains(k))
            })
            .and_then(|c| c.temperature())
    }

    fn find_gpu_temp(components: &Components) -> Option<f32> {
        const LABELS: &[&str] = &[
            "amdgpu", "radeon", "nouveau", "nvidia", "gpu", "edge", "junction", "mem",
        ];
        components
            .iter()
            .find(|c| {
                let l = c.label().to_lowercase();
                LABELS.iter().any(|k| l.contains(k))
            })
            .and_then(|c| c.temperature())
    }

    fn find_gpu_usage_sysfs() -> Option<u64> {
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

    /// Fetch a public IP address using curl.
    fn fetch_public_ip(version: IpVersion) -> Option<String> {
        let ip_flag = match version {
            IpVersion::V4 => "-4",
            IpVersion::V6 => "-6",
        };
        let output = Command::new("curl")
            .args([ip_flag, "-sf", "--max-time", "5", "https://icanhazip.com"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!ip.is_empty()).then_some(ip)
    }

    fn query_nvidia_smi() -> Option<(Option<f32>, Option<u64>)> {
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=temperature.gpu,utilization.gpu",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some((temp, util)) = stdout.trim().split_once(", ") {
            Some((temp.trim().parse().ok(), util.trim().parse().ok()))
        } else {
            Some((None, None))
        }
    }
}
