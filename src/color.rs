use cosmic::iced::Color;

/// Threshold colors from the active COSMIC theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AppletColor {
    /// Warning threshold color.
    pub(crate) warn: Color,
    /// Critical/destructive threshold color.
    pub(crate) critical: Color,
}

impl AppletColor {
    pub(crate) fn new() -> Self {
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();

        Self {
            warn: cosmic.palette.accent_yellow.into(),
            critical: cosmic.destructive_color().into(),
        }
    }

    /// Return `Some(color)` if `value` crosses a threshold, else `None`.
    ///
    /// - `value >= critical` → critical color
    /// - `value >= warn`     → warning color
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
