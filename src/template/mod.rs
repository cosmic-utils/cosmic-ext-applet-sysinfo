use std::fmt::Debug;

use crate::template::Variable::{CpuTemp, CpuUsage, DlSpeed, GpuTemp, GpuUsage, RamUsage, UlSpeed};

mod parse;

#[derive(Debug)]
pub(crate) struct Template {
    pub(crate) segments: Vec<Segment>,
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
}

impl Variable {
    const fn bit(self) -> u8 {
        1 << (self as u8)
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
