use cosmic::iced::Color;

/// Color scheme for the applet's threshold indicators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AppletColor {
    /// Warning threshold colour (yellow).
    pub(crate) warn: Color,
    /// Critical threshold colour (red).
    pub(crate) critical: Color,
}

impl AppletColor {
    pub(crate) fn new() -> Self {
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();

        Self {
            warn: cosmic.palette.accent_yellow.into(),
            critical: cosmic.palette.accent_red.into(),
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
