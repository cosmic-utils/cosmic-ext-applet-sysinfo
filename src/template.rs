#[derive(Clone, Copy)]
pub(crate) enum Variable {
    CpuUsage,
    RamUsage,
    CpuTemp,
    GpuTemp,
    GpuUsage,
    DlSpeed,
    UlSpeed,
}

impl Variable {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "cpu_usage" => Some(Self::CpuUsage),
            "ram_usage" => Some(Self::RamUsage),
            "cpu_temp" => Some(Self::CpuTemp),
            "gpu_temp" => Some(Self::GpuTemp),
            "gpu_usage" => Some(Self::GpuUsage),
            "dl_speed" => Some(Self::DlSpeed),
            "ul_speed" => Some(Self::UlSpeed),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub(crate) enum Segment {
    Literal(String),
    Variable(Variable),
    Unknown(String),
}

pub(crate) struct Requires {
    pub(crate) cpu_temp: bool,
    pub(crate) gpu_temp: bool,
    pub(crate) gpu_usage: bool,
}

impl Requires {
    pub(crate) fn from_segments(segments: &[Segment]) -> Self {
        let mut requires = Self {
            cpu_temp: false,
            gpu_temp: false,
            gpu_usage: false,
        };
        for segment in segments {
            if let Segment::Variable(var) = segment {
                match var {
                    Variable::CpuTemp => requires.cpu_temp = true,
                    Variable::GpuTemp => requires.gpu_temp = true,
                    Variable::GpuUsage => requires.gpu_usage = true,
                    _ => {}
                }
            }
        }
        requires
    }
}

pub(crate) fn parse(template: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if chars.peek() == Some(&'{') {
                    // escaped literal '{'
                    chars.next();
                    literal.push('{');
                } else {
                    // start of a variable: collect until '}'
                    if !literal.is_empty() {
                        segments.push(Segment::Literal(std::mem::take(&mut literal)));
                    }
                    let mut name = String::new();
                    for inner in chars.by_ref() {
                        if inner == '}' {
                            break;
                        }
                        name.push(inner);
                    }
                    match Variable::parse(&name) {
                        Some(var) => segments.push(Segment::Variable(var)),
                        None => segments.push(Segment::Unknown(name)),
                    }
                }
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    // escaped literal '}'
                    chars.next();
                    literal.push('}');
                } else {
                    // stray '}', treat as literal
                    literal.push('}');
                }
            }
            _ => literal.push(ch),
        }
    }

    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }

    segments
}
