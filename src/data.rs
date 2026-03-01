use std::{
    cell::LazyCell,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use sysinfo::{Components, CpuRefreshKind, MemoryRefreshKind, Networks, RefreshKind, System};

use crate::{
    config::SysInfoConfig,
    template::{Requires, Variable},
};

pub(crate) struct Data {
    system: System,
    networks: Networks,
    components: Components,
    physical_interfaces: Vec<String>,
    last_interface_scan: Instant,

    pub(crate) cpu_usage: Option<f32>,
    pub(crate) ram_usage: Option<u64>,
    pub(crate) download_speed: Option<f64>,
    pub(crate) upload_speed: Option<f64>,
    pub(crate) cpu_temp: Option<f32>,
    pub(crate) gpu_temp: Option<f32>,
    pub(crate) gpu_usage: Option<u64>,
}

impl Data {
    pub(crate) fn new(config: &SysInfoConfig) -> Self {
        let system = System::new_with_specifics(RefreshKind::nothing());
        let networks = Networks::new_with_refreshed_list();
        let components = Components::new_with_refreshed_list();
        let physical_interfaces = Self::detect_physical_interfaces(config);

        Self {
            system,
            networks,
            components,
            physical_interfaces,
            last_interface_scan: Instant::now(),
            cpu_usage: None,
            ram_usage: None,
            download_speed: None,
            upload_speed: None,
            cpu_temp: None,
            gpu_temp: None,
            gpu_usage: None,
        }
    }

    /// Refresh only the subsystems the current template actually uses.
    pub(crate) fn refresh(&mut self, requires: Requires, config: &SysInfoConfig) {
        let needs_cpu = requires.contains(Variable::CpuUsage);
        let needs_ram = requires.contains(Variable::RamUsage);
        let needs_download = requires.contains(Variable::DlSpeed);
        let needs_upload = requires.contains(Variable::UlSpeed);
        let needs_cpu_temp = requires.contains(Variable::CpuTemp);
        let needs_gpu_temp = requires.contains(Variable::GpuTemp);
        let needs_gpu_usage = requires.contains(Variable::GpuUsage);

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
