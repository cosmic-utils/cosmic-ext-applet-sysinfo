use std::{fmt::Debug, str::FromStr};

use crate::template::Variable::{CpuTemp, CpuUsage, DlSpeed, GpuTemp, GpuUsage, RamUsage, UlSpeed};

#[derive(Debug)]
pub(crate) struct Template {
    pub(crate) segments: Vec<Segment>,
    pub(crate) requires: Requires,
}

impl FromStr for Template {
    type Err = std::convert::Infallible;

    fn from_str(template: &str) -> Result<Self, Self::Err> {
        let mut segments = Vec::new();
        let mut rest = template;

        while let Some((before_var, var_n_rest)) = rest.split_once('{')
            && let Some((var, rest_)) = var_n_rest.split_once('}')
        {
            if !before_var.is_empty() {
                segments.push(Segment::Literal(before_var.to_string()));
            }
            match Variable::from_str(var) {
                Ok(var) => segments.push(Segment::Variable(var)),
                Err(()) => segments.push(Segment::Unknown(var.to_string())),
            }
            rest = rest_;
        }

        if !rest.is_empty() {
            segments.push(Segment::Literal(rest.to_string()));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Variable {
    CpuUsage = 0,
    RamUsage = 1,
    CpuTemp = 2,
    GpuTemp = 3,
    GpuUsage = 4,
    DlSpeed = 5,
    UlSpeed = 6,
}

impl Variable {
    const fn bit(self) -> u8 {
        1 << (self as u8)
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

#[derive(Debug)]
pub(crate) enum Segment {
    Literal(String),
    Variable(Variable),
    Unknown(String),
}

/// Compact bitset tracking which `Variable`s a template references.
#[derive(Clone, Copy, Default)]
pub(crate) struct Requires(u8);

impl Requires {
    pub(crate) fn contains(self, var: Variable) -> bool {
        self.0 & var.bit() != 0
    }

    fn insert(&mut self, var: Variable) {
        self.0 |= var.bit();
    }

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

impl Debug for Requires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let all = [
            CpuUsage, RamUsage, CpuTemp, GpuTemp, GpuUsage, DlSpeed, UlSpeed,
        ];

        let names: Vec<&str> = all
            .iter()
            .filter(|v| self.contains(**v))
            .map(|v| match v {
                CpuUsage => "cpu_usage",
                RamUsage => "ram_usage",
                CpuTemp => "cpu_temp",
                GpuTemp => "gpu_temp",
                GpuUsage => "gpu_usage",
                DlSpeed => "dl_speed",
                UlSpeed => "ul_speed",
            })
            .collect();

        f.debug_set().entries(names).finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse() {
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
