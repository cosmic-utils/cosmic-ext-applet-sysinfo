use std::fmt::Debug;

use crate::template::Variable::{
    CpuTemp, CpuUsage, DlSpeed, GpuTemp, GpuUsage, PublicIpv4, PublicIpv6, RamUsage, UlSpeed,
};

mod parse;
mod render;

#[derive(Debug)]
pub(crate) struct Template {
    segments: Vec<Segment>,
    pub(crate) requires: Requires,
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
    PublicIpv4 = 7,
    PublicIpv6 = 8,
}

const ALL_VARIABLES: [Variable; 9] = [
    CpuUsage, RamUsage, CpuTemp, GpuTemp, GpuUsage, DlSpeed, UlSpeed, PublicIpv4, PublicIpv6,
];

impl Variable {
    const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

#[derive(Debug)]
pub(crate) enum Segment {
    Literal(Box<str>),
    Variable(Variable),
    Unknown(Box<str>),
}

/// Compact bitset tracking which `Variable`s a template references.
#[derive(Clone, Copy, Default)]
pub(crate) struct Requires(u16);

impl Requires {
    pub(crate) fn contains(self, var: Variable) -> bool {
        self.0 & var.bit() != 0
    }

    fn insert(&mut self, var: Variable) {
        self.0 |= var.bit();
    }
}

impl Debug for Requires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = ALL_VARIABLES
            .into_iter()
            .filter(|v| self.contains(*v))
            .map(|v| match v {
                CpuUsage => "cpu_usage",
                RamUsage => "ram_usage",
                CpuTemp => "cpu_temp",
                GpuTemp => "gpu_temp",
                GpuUsage => "gpu_usage",
                DlSpeed => "dl_speed",
                UlSpeed => "ul_speed",
                PublicIpv4 => "pub_ipv4",
                PublicIpv6 => "pub_ipv6",
            })
            .collect();

        f.debug_set().entries(names).finish()
    }
}
