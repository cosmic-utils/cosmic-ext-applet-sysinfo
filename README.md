# Simple system info applet for cosmic

<p align="center">
    <img alt="Applet Screenshot" src="https://github.com/rwxroot/cosmic-ext-applet-sysinfo/blob/main/data/applet_screenshot_1.png">
</p>

<p align="center">
    <img alt="Applet Screenshot" src="https://github.com/rwxroot/cosmic-ext-applet-sysinfo/blob/main/data/applet_screenshot_2.png">
</p>

## Installation

### Manual

The applet can be installed using the following steps:

```sh
sudo apt install libxkbcommon-dev just
git clone https://github.com/rwxroot/cosmic-ext-applet-sysinfo.git
cd cosmic-ext-applet-sysinfo
just build
sudo just install
```

`libxkbcommon-dev` is required by `smithay-client-toolkit`

### Arch Linux

On Arch Linux, the applet can be installed using the PKGBUILD [`cosmic-ext-applet-sysinfo-git`](https://aur.archlinux.org/packages/cosmic-ext-applet-sysinfo-git), available on the [AUR](https://wiki.archlinux.org/index.php/Arch_User_Repository).
