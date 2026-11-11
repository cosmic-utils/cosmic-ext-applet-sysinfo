use cosmic::{cosmic_theme::Theme, iced::Color};

/// Color scheme for the applet's threshold indicators.
///
/// `warn` is used for warning states (yellow).
/// `critical` is used for critical/destructive states (red).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AppletColor {
    /// Warning threshold colour (yellow).
    pub(crate) warn: Color,
    /// Critical/threshold colour (red).
    pub(crate) critical: Color,
}

impl AppletColor {
    pub(crate) fn new() -> Self {
        Self::from_theme(cosmic::theme::active().cosmic())
    }

    fn from_theme(theme: &Theme) -> Self {
        Self {
            // Not `theme.warning_color()`: in the default dark theme it is an orange that is
            // hard to tell apart from `destructive_color()` (see #61). The palette's yellow is
            // tuned for each theme, so it stays readable on light panels too (see #67).
            warn: theme.palette.accent_yellow.into(),
            critical: theme.destructive_color().into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2.x contrast ratio between two opaque colours.
    fn contrast_ratio(a: Color, b: Color) -> f32 {
        fn luminance(c: Color) -> f32 {
            let channel = |v: f32| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
        }
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    fn assert_readable(theme: &Theme) {
        let colors = AppletColor::from_theme(theme);
        let background: Color = theme.bg_color().into();

        // WCAG AA asks for at least 4.5:1 for normal text.
        for (name, color) in [("warn", colors.warn), ("critical", colors.critical)] {
            let ratio = contrast_ratio(color, background);
            assert!(
                ratio >= 4.5,
                "{name} colour {color:?} has contrast {ratio:.2} on {background:?} in {}",
                theme.name
            );
        }
        assert_ne!(colors.warn, colors.critical, "in {}", theme.name);
    }

    #[test]
    fn colours_are_readable_in_light_theme() {
        assert_readable(&Theme::light_default());
    }

    #[test]
    fn colours_are_readable_in_dark_theme() {
        assert_readable(&Theme::dark_default());
    }

    #[test]
    fn threshold_picks_colour_by_severity() {
        let colors = AppletColor::from_theme(&Theme::dark_default());
        assert_eq!(colors.threshold(10.0, 50.0, 80.0), None);
        assert_eq!(colors.threshold(50.0, 50.0, 80.0), Some(colors.warn));
        assert_eq!(colors.threshold(80.0, 50.0, 80.0), Some(colors.critical));
    }
}
