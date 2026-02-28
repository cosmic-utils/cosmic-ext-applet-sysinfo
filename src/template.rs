use std::str::FromStr;

pub(crate) struct Template {
    pub(crate) segments: Vec<Segment>,
    pub(crate) requires: Requires,
}

impl FromStr for Template {
    type Err = std::convert::Infallible;

    fn from_str(template: &str) -> Result<Self, Self::Err> {
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
                        match Variable::from_str(&name) {
                            Ok(var) => segments.push(Segment::Variable(var)),
                            Err(()) => segments.push(Segment::Unknown(name)),
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

        Ok(Self::from_segments(segments))
    }
}

impl Template {
    fn from_segments(segments: Vec<Segment>) -> Self {
        let requires = Requires::from_segments(&segments);
        Self { segments, requires }
    }
}

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
    fn from_segments(segments: &[Segment]) -> Self {
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
