# Simple system info applet for cosmic

<p align="center">
    <img alt="Applet Screenshot" src="https://github.com/cosmic-utils/cosmic-ext-applet-sysinfo/blob/main/data/applet_screenshot_1.png">
</p>

<p align="center">
    <img alt="Applet Screenshot" src="https://github.com/cosmic-utils/cosmic-ext-applet-sysinfo/blob/main/data/applet_screenshot_2.png">
</p>

## Features

- **CPU usage** — percentage of total CPU utilization
- **RAM usage** — percentage of memory used (optionally including swap)
- **Network speed** — download and upload speeds in MB/s
- **CPU temperature** — reads from common thermal sensors via sysinfo
- **GPU temperature** — reads from sysinfo components (AMD/Intel), falls back to `nvidia-smi` for NVIDIA
- **GPU usage** — reads from sysfs (`gpu_busy_percent`), falls back to `nvidia-smi` for NVIDIA
- **Color-coded values** — metrics change color (normal → yellow → red) based on severity using COSMIC theme colors

## Display Template

The applet uses a configurable template string to control what is displayed and in what order. Edit the `template` file in your config directory:

```sh
~/.config/cosmic/io.github.cosmic-utils.cosmic-ext-applet-sysinfo/v1/template
```

### Default template

```
CPU {cpu_usage} RAM {ram_usage} ↓{dl_speed}M/s ↑{ul_speed}M/s
```

### Available variables

| Variable | Description | Example output |
|---|---|---|
| `{cpu_usage}` | CPU usage % (0 decimals) | `45%` |
| `{ram_usage}` | RAM usage % | `67%` |
| `{cpu_temp}` | CPU temperature in °C | `51°C` |
| `{gpu_temp}` | GPU temperature in °C | `48°C` |
| `{gpu_usage}` | GPU usage % | `3%` |
| `{dl_speed}` | Download speed in MB/s (2 decimals) | `1.23` |
| `{ul_speed}` | Upload speed in MB/s (2 decimals) | `0.45` |

When a sensor is not available, it shows `--` (e.g. `--°C`, `--%`).

### Example templates

All metrics with separators:
```
{gpu_temp} {gpu_usage} | {cpu_temp} {cpu_usage} | {ram_usage} | ↓{dl_speed} ↑{ul_speed}
```
→ `48°C 3% | 51°C 45% | 67% | ↓1.23 ↑0.45`

Grouped by category:
```
CPU {cpu_usage} {cpu_temp} | GPU {gpu_usage} {gpu_temp} | RAM {ram_usage}
```
→ `CPU 45% 51°C | GPU 3% 48°C | RAM 67%`

Network focused:
```
↓{dl_speed}M/s ↑{ul_speed}M/s | CPU {cpu_usage}
```
→ `↓1.23M/s ↑0.45M/s | CPU 45%`

Minimal:
```
{cpu_usage} {ram_usage}
```
→ `45% 67%`

Temps only:
```
CPU {cpu_temp} GPU {gpu_temp}
```
→ `CPU 51°C GPU 48°C`

## Color Coding

Values are automatically color-coded using COSMIC theme colors:

| Metric | Normal | Yellow | Red |
|---|---|---|---|
| CPU usage | < 50% | 50–80% | ≥ 80% |
| RAM usage | < 50% | 50–80% | ≥ 80% |
| CPU temp | < 60°C | 60–80°C | ≥ 80°C |
| GPU temp | < 60°C | 60–85°C | ≥ 85°C |
| GPU usage | < 50% | 50–80% | ≥ 80% |

Download and upload speeds are not color-coded (high speed is not a problem).

## GPU Monitoring

GPU temperature and usage are read using sysinfo components and sysfs for AMD/Intel GPUs. For NVIDIA GPUs, the applet falls back to `nvidia-smi` when sysinfo/sysfs data is unavailable.

`nvidia-smi` is an optional dependency — GPU metrics will simply show `--` if it is not installed and sysfs data is unavailable.

## Installation

### Flatpak

Depending on how you've installed COSMIC Desktop, the Sysinfo applet may show up in your app store by default. In COSMIC Store it should be under the "COSMIC Applets" category.

If the applet does not show up in your app store, you'll need to add `cosmic-flatpak` as a source:

```sh
flatpak remote-add --if-not-exists --user cosmic https://apt.pop-os.org/cosmic/cosmic.flatpakrepo
```

Then, proceed to your preferred app store and search for Sysinfo applet.

### Manual

The applet can be installed using the following steps:

```sh
sudo apt install libxkbcommon-dev just
git clone https://github.com/cosmic-utils/cosmic-ext-applet-sysinfo.git
cd cosmic-ext-applet-sysinfo
just build
sudo just install
```

`libxkbcommon-dev` is required by `smithay-client-toolkit`

### Arch Linux

On Arch Linux, the applet can be installed using the PKGBUILD [`cosmic-ext-applet-sysinfo-git`](https://aur.archlinux.org/packages/cosmic-ext-applet-sysinfo-git), available on the [AUR](https://wiki.archlinux.org/index.php/Arch_User_Repository).

---

## Network Interface Detection & Configuration

The applet automatically monitors physical network interfaces (Ethernet and Wi-Fi), ignoring virtual interfaces and loopback.

### Advanced Configuration

The applet also provides a configuration that can be used to specify interfaces to include or exclude.

```sh
cd ~/.config/cosmic/io.github.cosmic-utils.cosmic-ext-applet-sysinfo/v1/
```

Example configuration:

Include interface(s) in the `include_interfaces` file:

```
Some(["enp7s0", "wlp4s0"])
```

Or exclude specific interface(s) in `exclude_interfaces` file:

```
Some(["lo", "br0", "docker0", "virbr0"])
```

or

```
None
```

- `include_interfaces`: Only monitor listed interfaces
- `exclude_interfaces`: Ignore listed interfaces
- Both options can be combined; `include_interfaces` is applied first
- Without configuration: all physical interfaces are monitored
- For hotplugged devices (10s rescan interval), prefer `exclude_interfaces` as interface names may be unpredictable

---

## Uninstall

To uninstall files installed by `just install`, run:

```sh
sudo just uninstall
```
