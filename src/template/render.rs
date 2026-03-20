use std::borrow::Cow;

use cosmic::{
    iced::Color,
    iced_widget::{rich_text, span, text},
};

use super::{Segment, Template, Variable};
use crate::{applet, data::Data};

impl Template {
    pub(crate) fn render<'a, Theme: text::Catalog + 'a>(
        &'a self,
        data: &'a Data,
        colors: &applet::ThemeColors,
    ) -> text::Rich<'a, applet::Message, Theme> {
        let spans: Vec<_> = self
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Literal(text) => span(&**text),
                Segment::Variable(var) => {
                    let (text, color) = self.resolve_variable(*var, data, colors);
                    span(text).color_maybe(color)
                }
                Segment::Unknown(name) => span(format!("{{{name}}}")).color(colors.red),
            })
            .collect();

        rich_text(spans)
    }

    fn resolve_variable<'data>(
        &self,
        var: Variable,
        data: &'data Data,
        colors: &applet::ThemeColors,
    ) -> (Cow<'data, str>, Option<Color>) {
        match var {
            Variable::CpuUsage => match data.cpu_usage {
                Some(v) => (
                    format!("{v:.0}%").into(),
                    colors.threshold(v as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
            Variable::RamUsage => match data.ram_usage {
                Some(v) => (
                    format!("{v}%").into(),
                    colors.threshold(v as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
            Variable::CpuTemp => match data.cpu_temp {
                Some(t) => (
                    format!("{t:.0}°C").into(),
                    colors.threshold(t as f64, 60.0, 80.0),
                ),
                None => ("--°C".into(), None),
            },
            Variable::GpuTemp => match data.gpu_temp {
                Some(t) => (
                    format!("{t:.0}°C").into(),
                    colors.threshold(t as f64, 60.0, 85.0),
                ),
                None => ("--°C".into(), None),
            },
            Variable::GpuUsage => match data.gpu_usage {
                Some(u) => (
                    format!("{u}%").into(),
                    colors.threshold(u as f64, 50.0, 80.0),
                ),
                None => ("--%".into(), None),
            },
            Variable::DlSpeed => match data.download_speed {
                Some(s) => (format!("{s:.2}").into(), None),
                None => ("--".into(), None),
            },
            Variable::UlSpeed => match data.upload_speed {
                Some(s) => (format!("{s:.2}").into(), None),
                None => ("--".into(), None),
            },
            Variable::PublicIpv4 => match &data.public_ipv4 {
                Some(ip) => (ip.into(), None),
                None => ("--".into(), None),
            },
            Variable::PublicIpv6 => match &data.public_ipv6 {
                Some(ip) => (ip.into(), None),
                None => ("--".into(), None),
            },
        }
    }
}
