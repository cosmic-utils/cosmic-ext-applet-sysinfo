use std::fmt::Debug;

use crate::template::Variable::{
    AmdGpuTemp, AmdGpuUsage, AmdVramUsage, CpuTemp, CpuUsage, DiskRead, DiskUsage, DiskUsed,
    DiskWrite, DlSpeed, GpuTemp, GpuUsage, IntelGpuTemp, IntelGpuUsage, IntelVramUsage,
    NpuFrequency, NpuUsage, NvidiaGpuTemp, NvidiaGpuUsage, NvidiaVramUsage, PublicIpv4, PublicIpv6,
    RamUsage, UlSpeed, VramUsage,
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
    NpuUsage = 9,
    NpuFrequency = 10,
    DiskRead = 11,
    DiskWrite = 12,
    VramUsage = 13,
    DiskUsed = 14,
    DiskUsage = 15,
    AmdGpuTemp = 16,
    AmdGpuUsage = 17,
    AmdVramUsage = 18,
    IntelGpuTemp = 19,
    IntelGpuUsage = 20,
    IntelVramUsage = 21,
    NvidiaGpuTemp = 22,
    NvidiaGpuUsage = 23,
    NvidiaVramUsage = 24,
}

const ALL_VARIABLES: [Variable; 25] = [
    CpuUsage,
    RamUsage,
    CpuTemp,
    GpuTemp,
    GpuUsage,
    DlSpeed,
    UlSpeed,
    PublicIpv4,
    PublicIpv6,
    NpuUsage,
    NpuFrequency,
    DiskRead,
    DiskWrite,
    VramUsage,
    DiskUsed,
    DiskUsage,
    AmdGpuTemp,
    AmdGpuUsage,
    AmdVramUsage,
    IntelGpuTemp,
    IntelGpuUsage,
    IntelVramUsage,
    NvidiaGpuTemp,
    NvidiaGpuUsage,
    NvidiaVramUsage,
];

impl Variable {
    const fn bit(self) -> u32 {
        1 << (self as u32)
    }

    /// Every variable, in the order they are offered in the popup.
    pub(crate) const ALL: [Variable; 25] = ALL_VARIABLES;

    /// The name used in templates, without braces.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            CpuUsage => "cpu_usage",
            RamUsage => "ram_usage",
            CpuTemp => "cpu_temp",
            GpuTemp => "gpu_temp",
            GpuUsage => "gpu_usage",
            DlSpeed => "dl_speed",
            UlSpeed => "ul_speed",
            PublicIpv4 => "pub_ipv4",
            PublicIpv6 => "pub_ipv6",
            NpuUsage => "npu_usage",
            NpuFrequency => "npu_frequency",
            DiskRead => "disk_read",
            DiskWrite => "disk_write",
            VramUsage => "vram_usage",
            DiskUsed => "disk_used",
            DiskUsage => "disk_usage",
            AmdGpuTemp => "amd_gpu_temp",
            AmdGpuUsage => "amd_gpu_usage",
            AmdVramUsage => "amd_vram_usage",
            IntelGpuTemp => "intel_gpu_temp",
            IntelGpuUsage => "intel_gpu_usage",
            IntelVramUsage => "intel_vram_usage",
            NvidiaGpuTemp => "nvidia_gpu_temp",
            NvidiaGpuUsage => "nvidia_gpu_usage",
            NvidiaVramUsage => "nvidia_vram_usage",
        }
    }

    pub(crate) const fn category(self) -> Category {
        match self {
            CpuUsage | CpuTemp => Category::Cpu,
            RamUsage => Category::Memory,
            GpuTemp | GpuUsage | VramUsage | AmdGpuTemp | AmdGpuUsage | AmdVramUsage
            | IntelGpuTemp | IntelGpuUsage | IntelVramUsage | NvidiaGpuTemp | NvidiaGpuUsage
            | NvidiaVramUsage => Category::Gpu,
            DlSpeed | UlSpeed | PublicIpv4 | PublicIpv6 => Category::Network,
            DiskRead | DiskWrite | DiskUsed | DiskUsage => Category::Disk,
            NpuUsage | NpuFrequency => Category::Npu,
        }
    }

    /// The text to insert for this variable between `before` and `after`, the characters
    /// around the cursor. Adds a space on each side that would otherwise touch other text.
    pub(crate) fn insertion(self, before: Option<char>, after: Option<char>) -> String {
        let touches = |c: Option<char>| c.is_some_and(|c| !c.is_whitespace());
        let mut text = String::with_capacity(self.name().len() + 4);
        if touches(before) {
            text.push(' ');
        }
        text.push('{');
        text.push_str(self.name());
        text.push('}');
        if touches(after) {
            text.push(' ');
        }
        text
    }

    /// Whether reading this variable sends a request over the network.
    pub(crate) const fn queries_network(self) -> bool {
        matches!(self, PublicIpv4 | PublicIpv6)
    }
}

/// Groups the variables in the popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    Cpu,
    Memory,
    Gpu,
    Network,
    Disk,
    Npu,
}

impl Category {
    pub(crate) const ALL: [Category; 6] = [
        Category::Cpu,
        Category::Memory,
        Category::Gpu,
        Category::Network,
        Category::Disk,
        Category::Npu,
    ];
}

impl Template {
    /// Names inside braces that are not a known variable, in order of appearance.
    pub(crate) fn unknown_variables(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().filter_map(|segment| match segment {
            Segment::Unknown(name) => Some(name.as_ref()),
            _ => None,
        })
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
pub(crate) struct Requires(u32);

impl Requires {
    pub(crate) fn contains(self, var: Variable) -> bool {
        self.0 & var.bit() != 0
    }

    fn insert(&mut self, var: Variable) {
        self.0 |= var.bit();
    }

    /// Every variable that can be read without a network request.
    pub(crate) fn local() -> Self {
        let mut req = Self::default();
        for var in ALL_VARIABLES {
            if !var.queries_network() {
                req.insert(var);
            }
        }
        req
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Debug for Requires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = ALL_VARIABLES
            .into_iter()
            .filter(|v| self.contains(*v))
            .map(Variable::name)
            .collect();

        f.debug_set().entries(names).finish()
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashSet, str::FromStr};

    use super::*;

    #[test]
    fn every_variable_is_listed_once() {
        let discriminants: HashSet<u8> = ALL_VARIABLES.iter().map(|v| *v as u8).collect();
        assert_eq!(discriminants.len(), ALL_VARIABLES.len());
        // discriminants are bit positions, so they must be 0..N without gaps
        assert!(
            discriminants
                .iter()
                .all(|d| usize::from(*d) < ALL_VARIABLES.len())
        );
    }

    #[test]
    fn names_round_trip_through_parser() {
        let names: HashSet<&str> = ALL_VARIABLES.iter().map(|v| v.name()).collect();
        assert_eq!(names.len(), ALL_VARIABLES.len(), "names must be unique");
        for var in ALL_VARIABLES {
            assert_eq!(Variable::from_str(var.name()), Ok(var));
        }
    }

    #[test]
    fn every_category_has_variables() {
        for category in Category::ALL {
            assert!(
                ALL_VARIABLES.iter().any(|v| v.category() == category),
                "{category:?} is empty"
            );
        }
    }

    #[test]
    fn local_requires_skips_network_queries() {
        let local = Requires::local();
        assert!(!local.contains(PublicIpv4));
        assert!(!local.contains(PublicIpv6));
        assert!(local.contains(CpuUsage));
        assert!(local.contains(NvidiaGpuUsage));

        let mut template = Requires::default();
        template.insert(PublicIpv4);
        assert!(local.union(template).contains(PublicIpv4));
    }

    #[test]
    fn insertion_adds_spaces_only_where_needed() {
        assert_eq!(CpuUsage.insertion(None, None), "{cpu_usage}");
        assert_eq!(CpuUsage.insertion(Some(' '), Some(' ')), "{cpu_usage}");
        assert_eq!(CpuUsage.insertion(Some('U'), None), " {cpu_usage}");
        assert_eq!(CpuUsage.insertion(Some('}'), Some('R')), " {cpu_usage} ");
        assert_eq!(CpuUsage.insertion(None, Some('\n')), "{cpu_usage}");
    }

    #[test]
    fn unknown_variables_are_reported_in_order() {
        let Ok(template) = Template::from_str("{cpu_usge} {ram_usage} {{literal}} {gpu}");
        assert_eq!(
            template.unknown_variables().collect::<Vec<_>>(),
            ["cpu_usge", "gpu"]
        );

        let Ok(template) = Template::from_str("{cpu_usage} {ram_usage}");
        assert_eq!(template.unknown_variables().count(), 0);
    }
}
