use std::str::FromStr;

use super::{Requires, Segment, Template, Variable};

impl FromStr for Template {
    type Err = std::convert::Infallible;

    fn from_str(template: &str) -> Result<Self, Self::Err> {
        let mut segments = Vec::new();
        let mut rest = template;

        while let Some((before_var, var_n_rest)) = rest.split_once('{')
            && let Some((var, rest_)) = var_n_rest.split_once('}')
        {
            if !before_var.is_empty() {
                segments.push(Segment::Literal(before_var.into()));
            }
            match Variable::from_str(var) {
                Ok(var) => segments.push(Segment::Variable(var)),
                Err(()) => segments.push(Segment::Unknown(var.into())),
            }
            rest = rest_;
        }

        if !rest.is_empty() {
            segments.push(Segment::Literal(rest.into()));
        }

        Ok(Self::from_segments(segments))
    }
}

impl Template {
    fn from_segments(segments: Vec<Segment>) -> Self {
        let requires = Requires::from_segments(&segments);
        Self { segments, requires }
    }
}

impl FromStr for Variable {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cpu_usage" => Ok(Self::CpuUsage),
            "ram_usage" => Ok(Self::RamUsage),
            "cpu_temp" => Ok(Self::CpuTemp),
            "gpu_temp" => Ok(Self::GpuTemp),
            "gpu_usage" => Ok(Self::GpuUsage),
            "dl_speed" => Ok(Self::DlSpeed),
            "ul_speed" => Ok(Self::UlSpeed),
            _ => Err(()),
        }
    }
}

impl Requires {
    fn from_segments(segments: &[Segment]) -> Self {
        let mut req = Self::default();
        for seg in segments {
            if let Segment::Variable(var) = seg {
                req.insert(*var);
            }
        }
        req
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_template() {
        let parse = |template| match Template::from_str(template) {
            Ok(res) => res,
        };

        insta::assert_debug_snapshot!(
            "all metrics with separators",
            parse(
                "{gpu_temp} {gpu_usage} | {cpu_temp} {cpu_usage} | {ram_usage} | ↓{dl_speed} ↑{ul_speed}",
            ),
        );
        insta::assert_debug_snapshot!(
            "grouped by category",
            parse("CPU {cpu_usage} {cpu_temp} | GPU {gpu_usage} {gpu_temp} | RAM {ram_usage}",)
        );
        insta::assert_debug_snapshot!(
            "network focused",
            parse("↓{dl_speed}M/s ↑{ul_speed}M/s | CPU {cpu_usage}")
        );
        insta::assert_debug_snapshot!("minimal", parse("{cpu_usage} {ram_usage}"));
        insta::assert_debug_snapshot!("temps only", parse("CPU {cpu_temp} GPU {gpu_temp}"));
    }
}
