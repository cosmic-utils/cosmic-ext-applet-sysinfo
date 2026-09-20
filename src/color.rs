use cosmic::iced::Color;

/// Color scheme for the applet's threshold indicators.
///
/// `yellow` is used for warning states.
/// `red` is used for critical/destructive states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AppletColor {
    /// Warning threshold colour (yellow).
    pub(crate) warn: Color,
    /// Critical/threshold colour (red).
    pub(crate) critical: Color,
}

impl AppletColor {
    pub(crate) fn new() -> Self {
        Self {
            warn: Color {
                r: 0.96862745,
                g: 0.8784314,
                b: 0.38431373,
                a: 1.0,
            },
            critical: Color {
                r: 0.99215686,
                g: 0.6313726,
                b: 0.627451,
                a: 1.0,
            },
        }
    }

    /// Return `Some(color)` if `value` crosses a threshold, else `None`.
    ///
    /// - `value >= critical` → red
    /// - `value >= warn`     → yellow
    /// - otherwise           → None (normal)
    pub(crate) fn threshold(&self, value: f64, warn: f64, critical: f64) -> Option<Color> {
        if value >= critical {
            Some(self.critical)
        } else if value >= warn {
            Some(self.warn)
        } else {
            None
        }
    }
}
