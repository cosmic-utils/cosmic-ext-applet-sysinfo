use cosmic::iced::Color;

/// Color scheme for the applet's threshold indicators.
///
/// `yellow` is used for warning states.
/// `red` is used for critical/destructive states.
/// `green` is used for a charging battery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AppletColor {
    /// Warning threshold colour (yellow).
    pub(crate) warn: Color,
    /// Critical/threshold colour (red).
    pub(crate) critical: Color,
    /// Charging battery colour (green).
    pub(crate) charging: Color,
}

impl AppletColor {
    pub(crate) fn new() -> Self {
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();

        Self {
            warn: Color {
                r: 0.96862745,
                g: 0.8784314,
                b: 0.38431373,
                a: 1.0,
            },
            critical: cosmic.destructive_color().into(),
            // the palette's green is tuned for both light and dark themes
            charging: cosmic.palette.accent_green.into(),
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
    use cosmic::cosmic_theme::Theme;

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

    #[test]
    fn charging_colour_is_readable_in_light_and_dark_themes() {
        for theme in [Theme::light_default(), Theme::dark_default()] {
            let charging: Color = theme.palette.accent_green.into();
            let background: Color = theme.bg_color().into();
            // WCAG AA asks for at least 4.5:1 for normal text
            let ratio = contrast_ratio(charging, background);
            assert!(ratio >= 4.5, "contrast {ratio:.2} in {}", theme.name);
        }
    }
}
