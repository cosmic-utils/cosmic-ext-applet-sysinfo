# Simple system info applet for cosmic

<p align="center">
    <img alt="Applet Screenshot" src="https://github.com/cosmic-utils/cosmic-ext-applet-sysinfo/blob/main/data/applet_screenshot_1.png">
</p>

<p align="center">
    <img alt="Applet Screenshot" src="https://github.com/cosmic-utils/cosmic-ext-applet-sysinfo/blob/main/data/applet_screenshot_2.png">
</p>

## Installation

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
