use std::borrow::Cow;

use cosmic::iced::{
    Color,
    widget::{rich_text, span, text},
};

use crate::{
    applet,
    color::AppletColor,
    data::Data,
    template::{Segment, Template, Variable},
};

impl Template {
    pub(crate) fn render<'a, Theme: text::Catalog + 'a>(
        &'a self,
        data: &'a Data,
        colors: &AppletColor,
        use_mono_font: bool,
    ) -> text::Rich<'a, (), applet::Message, Theme> {
        let spans: Vec<_> = self
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Literal(text) => span(text.as_ref()),
                Segment::Variable(var) => {
                    let (text, color) = self.resolve_variable(*var, data, colors);
                    span(text).color_maybe(color)
                }
                Segment::Unknown(name) => span(format!("{{{name}}}")).color(colors.critical),
            })
            .map(|segment| {
                if use_mono_font {
                    segment.font(cosmic::font::mono())
                } else {
                    segment
                }
            })
            .collect();

        rich_text(spans)
    }

    fn resolve_variable<'data>(
        &self,
        var: Variable,
        data: &'data Data,
        colors: &AppletColor,
    ) -> (Cow<'data, str>, Option<Color>) {
        match var {
            Variable::CpuUsage => match data.cpu_usage {
                Some(v) => (
                    format!("{v:>2.0}%").into(),
                    colors.threshold(v as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
            Variable::RamUsage => match data.ram_usage {
                Some(v) => (
                    format!("{v:>2}%").into(),
                    colors.threshold(v as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
            Variable::CpuTemp => match data.cpu_temp {
                Some(t) => (
                    format!("{t:>2.0}°C").into(),
                    colors.threshold(t as f64, 60.0, 80.0),
                ),
                None => ("--°C".into(), None),
            },
            Variable::GpuTemp => gpu_temp(data.gpu.temp, colors),
            Variable::GpuUsage => gpu_percent(data.gpu.usage, colors),
            Variable::VramUsage => gpu_percent(data.gpu.vram_usage, colors),
            Variable::AmdGpuTemp => gpu_temp(data.gpu.amd.temp, colors),
            Variable::AmdGpuUsage => gpu_percent(data.gpu.amd.usage, colors),
            Variable::AmdVramUsage => gpu_percent(data.gpu.amd.vram_usage, colors),
            Variable::IntelGpuTemp => gpu_temp(data.gpu.intel.temp, colors),
            Variable::IntelGpuUsage => gpu_percent(data.gpu.intel.usage, colors),
            Variable::IntelVramUsage => gpu_percent(data.gpu.intel.vram_usage, colors),
            Variable::NvidiaGpuTemp => gpu_temp(data.gpu.nvidia.temp, colors),
            Variable::NvidiaGpuUsage => gpu_percent(data.gpu.nvidia.usage, colors),
            Variable::NvidiaVramUsage => gpu_percent(data.gpu.nvidia.vram_usage, colors),
            Variable::DlSpeed => match data.download_speed {
                Some(s) => (format!("{s:4.1}").into(), None),
                None => (" -.-".into(), None),
            },
            Variable::UlSpeed => match data.upload_speed {
                Some(s) => (format!("{s:4.1}").into(), None),
                None => (" -.-".into(), None),
            },
            Variable::PublicIpv4 => match &data.public_ipv4 {
                Some(ip) => (ip.into(), None),
                None => ("--".into(), None),
            },
            Variable::PublicIpv6 => match &data.public_ipv6 {
                Some(ip) => (ip.into(), None),
                None => ("--".into(), None),
            },
            Variable::NpuUsage => match data.npu.usage {
                Some(v) => (
                    format!("{v:>2}%").into(),
                    colors.threshold(v as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
            Variable::NpuFrequency => match data.npu.frequency {
                Some(f) => (format!("{f:>2}MHz").into(), None),
                None => ("--".into(), None),
            },
            Variable::DiskRead => match data.disks.read {
                Some(s) => (format!("{s:4.1}").into(), None),
                None => (" -.-".into(), None),
            },
            Variable::DiskWrite => match data.disks.write {
                Some(s) => (format!("{s:4.1}").into(), None),
                None => (" -.-".into(), None),
            },
            Variable::DiskUsed => match data.disks.used {
                Some(s) => (format!("{s:4.1}GB").into(), None),
                None => ("-.-".into(), None),
            },
            Variable::DiskUsage => match data.disks.usage {
                Some(s) => (
                    format!("{s:>2}%").into(),
                    colors.threshold(s as f64, 75.0, 90.0),
                ),
                None => ("--%".into(), None),
            },
        }
    }
}

fn gpu_temp<'a>(temp: Option<f32>, colors: &AppletColor) -> (Cow<'a, str>, Option<Color>) {
    match temp {
        Some(t) => (
            format!("{t:>2.0}°C").into(),
            colors.threshold(t as f64, 60.0, 85.0),
        ),
        None => ("--°C".into(), None),
    }
}

fn gpu_percent<'a>(value: Option<u64>, colors: &AppletColor) -> (Cow<'a, str>, Option<Color>) {
    match value {
        Some(v) => (
            format!("{v:>2}%").into(),
            colors.threshold(v as f64, 50.0, 80.0),
        ),
        None => ("--%".into(), None),
    }
}
