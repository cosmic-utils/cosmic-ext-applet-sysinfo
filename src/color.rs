use cosmic::iced::Color;

/// Color scheme for the applet's threshold indicators.
///
/// `yellow` is used for warning states (amber to ensure distinction from red).
/// `red` is used for critical/destructive states.
pub(crate) struct AppletColor {
    pub(crate) yellow: Color,
    pub(crate) red: Color,
}

impl AppletColor {
    pub(crate) fn from_active_theme() -> Self {
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();

        // Amber/yellow that is reliably distinct from red, regardless of theme.
        const AMBER: Color = Color {
            r: 1.0,
            g: 0.82,
            b: 0.0,
            a: 1.0,
        };

        Self {
            yellow: AMBER,
            red: cosmic.destructive_color().into(),
        }
    }

    /// Return `Some(color)` if `value` crosses a threshold, else `None`.
    ///
    /// - `value >= critical` → red
    /// - `value >= warn`     → yellow
    /// - otherwise           → None (normal)
    pub(crate) fn threshold(&self, value: f64, warn: f64, critical: f64) -> Option<Color> {
        if value >= critical {
            Some(self.red)
        } else if value >= warn {
            Some(self.yellow)
        } else {
            None
        }
    }
}
