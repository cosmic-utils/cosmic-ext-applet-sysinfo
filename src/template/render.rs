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
            Variable::GpuTemp => match data.gpu.temp {
                Some(t) => (
                    format!("{t:>2.0}°C").into(),
                    colors.threshold(t as f64, 60.0, 85.0),
                ),
                None => ("--°C".into(), None),
            },
            Variable::GpuUsage => match data.gpu.usage {
                Some(v) => (
                    format!("{v:>2}%").into(),
                    colors.threshold(v as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
            Variable::VramUsage => match data.gpu.vram_usage {
                Some(v) => (
                    format!("{v:>2}%").into(),
                    colors.threshold(v as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
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
            Variable::BatteryPower => battery_power(data.battery_power, colors),
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

/// Battery power with its direction, e.g. `+25.1W` (green) while charging, `-12.3W`
/// (yellow) while discharging and `0.0W` while neither.
fn battery_power<'a>(watts: Option<f64>, colors: &AppletColor) -> (Cow<'a, str>, Option<Color>) {
    match watts {
        // don't show `-0.0W` or `+0.0W` for a battery that is barely in use
        Some(w) if w.abs() < 0.05 => (format!("{:>5.1}W", 0.0).into(), None),
        Some(w) if w > 0.0 => (format!("{w:>+5.1}W").into(), Some(colors.charging)),
        Some(w) => (format!("{w:>+5.1}W").into(), Some(colors.warn)),
        None => ("--W".into(), None),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const COLORS: AppletColor = AppletColor {
        warn: Color::from_rgb(1.0, 1.0, 0.0),
        critical: Color::from_rgb(1.0, 0.0, 0.0),
        charging: Color::from_rgb(0.0, 1.0, 0.0),
    };

    #[test]
    fn battery_power_format_and_colour() {
        let power = |watts| battery_power(watts, &COLORS);
        assert_eq!(power(None), ("--W".into(), None));
        assert_eq!(power(Some(0.0)), ("  0.0W".into(), None));
        assert_eq!(power(Some(-0.01)), ("  0.0W".into(), None));
        assert_eq!(power(Some(0.04)), ("  0.0W".into(), None));
        assert_eq!(power(Some(-12.345)), ("-12.3W".into(), Some(COLORS.warn)));
        assert_eq!(power(Some(-5.06)), (" -5.1W".into(), Some(COLORS.warn)));
        assert_eq!(power(Some(25.0)), ("+25.0W".into(), Some(COLORS.charging)));
        assert_eq!(
            power(Some(100.0)),
            ("+100.0W".into(), Some(COLORS.charging))
        );
    }
}
