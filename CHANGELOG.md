# Changelog

## 0.1.0 - 2026-03-03

Not really the first release of cosmic-ext-applet-sysinfo.

### Added

- Configurable display template system with `{cpu_usage}`, `{ram_usage}`,
  `{swap_usage}`, `{cpu_temp}`, `{gpu_temp}`, `{gpu_usage}`, `{net_up}`,
  `{net_down}`, and other variables for customizing panel output
- Colour-coded severity indicators that inherit the theme's text colour at low
  utilization
- Escape literal braces with `{{` / `}}`; unknown variables render in red
