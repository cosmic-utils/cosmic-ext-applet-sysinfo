//! Battery charge and discharge power, read from `/sys/class/power_supply`.

use std::{fs, path::Path};

pub(crate) const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply";

/// Net power flowing into the system batteries, in watts: positive while charging,
/// negative while discharging and zero while neither (e.g. full on AC).
///
/// Returns `None` when there is no system battery (e.g. on a desktop) or its power
/// can't be read. With several batteries, their power is added up.
pub(crate) fn read_power(power_supply_dir: &Path) -> Option<f64> {
    let mut total = None;

    for entry in fs::read_dir(power_supply_dir).ok()?.flatten() {
        let supply = entry.path();
        if read(&supply, "type").as_deref() != Some("Battery") {
            continue;
        }
        // peripherals such as a wireless mouse report `scope=Device`
        if read(&supply, "scope").as_deref() == Some("Device") {
            continue;
        }
        let Some(micro_watts) = micro_watts(&supply) else {
            continue;
        };
        let sign = match read(&supply, "status").as_deref() {
            Some("Charging") => 1.0,
            Some("Discharging") => -1.0,
            // "Full", "Not charging", "Unknown"
            _ => 0.0,
        };
        *total.get_or_insert(0.0) += sign * micro_watts / 1_000_000.0;
    }

    total
}

/// The battery's power in µW, regardless of its direction.
fn micro_watts(supply: &Path) -> Option<f64> {
    let number = |name| read(supply, name)?.parse::<f64>().ok();

    // Some drivers report the power directly, others only current and voltage. Some
    // report a negative value while discharging, the status is used for the sign instead.
    if let Some(power) = number("power_now") {
        return Some(power.abs());
    }
    let current = number("current_now")?; // µA
    let voltage = number("voltage_now")?; // µV
    Some((current * voltage).abs() / 1_000_000.0)
}

fn read(supply: &Path, name: &str) -> Option<String> {
    let contents = fs::read_to_string(supply.join(name)).ok()?;
    Some(contents.trim().to_owned())
}

#[cfg(test)]
mod test {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    /// A fake `/sys/class/power_supply`, removed on drop.
    struct PowerSupplyDir(PathBuf);

    impl PowerSupplyDir {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "sysinfo-applet-power-supply-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn supply(&self, name: &str, attributes: &[(&str, &str)]) -> &Self {
            let supply = self.0.join(name);
            fs::create_dir_all(&supply).unwrap();
            for (attribute, value) in attributes {
                // sysfs values end with a newline
                fs::write(supply.join(attribute), format!("{value}\n")).unwrap();
            }
            self
        }

        fn power(&self) -> Option<f64> {
            read_power(&self.0)
        }
    }

    impl Drop for PowerSupplyDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_battery() {
        let dir = PowerSupplyDir::new();
        dir.supply("ADP0", &[("type", "Mains"), ("online", "1")]);
        assert_eq!(dir.power(), None);
    }

    #[test]
    fn missing_directory() {
        assert_eq!(read_power(Path::new("/nonexistent/power_supply")), None);
    }

    #[test]
    fn discharging_is_negative() {
        let dir = PowerSupplyDir::new();
        dir.supply(
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("power_now", "12345000"),
            ],
        );
        assert_eq!(dir.power(), Some(-12.345));
    }

    #[test]
    fn charging_is_positive() {
        let dir = PowerSupplyDir::new();
        dir.supply(
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Charging"),
                ("power_now", "25000000"),
            ],
        );
        assert_eq!(dir.power(), Some(25.0));
    }

    #[test]
    fn full_is_zero() {
        // as reported by a real laptop on AC
        let dir = PowerSupplyDir::new();
        dir.supply("ADP0", &[("type", "Mains"), ("online", "1")])
            .supply(
                "BAT0",
                &[
                    ("type", "Battery"),
                    ("status", "Full"),
                    ("power_now", "0"),
                    ("voltage_now", "17423000"),
                ],
            );
        assert_eq!(dir.power(), Some(0.0));
    }

    #[test]
    fn negative_power_now_uses_status_for_sign() {
        let dir = PowerSupplyDir::new();
        dir.supply(
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("power_now", "-8000000"),
            ],
        );
        assert_eq!(dir.power(), Some(-8.0));
    }

    #[test]
    fn falls_back_to_current_times_voltage() {
        let dir = PowerSupplyDir::new();
        // 1.5 A × 12 V = 18 W
        dir.supply(
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("current_now", "1500000"),
                ("voltage_now", "12000000"),
            ],
        );
        assert_eq!(dir.power(), Some(-18.0));
    }

    #[test]
    fn several_batteries_are_added_up() {
        let dir = PowerSupplyDir::new();
        dir.supply(
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("power_now", "10000000"),
            ],
        )
        .supply(
            "BAT1",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("power_now", "5000000"),
            ],
        );
        assert_eq!(dir.power(), Some(-15.0));
    }

    #[test]
    fn peripheral_batteries_are_ignored() {
        let dir = PowerSupplyDir::new();
        dir.supply(
            "hidpp_battery_0",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("status", "Discharging"),
                ("power_now", "100000"),
            ],
        );
        assert_eq!(dir.power(), None);
    }

    #[test]
    fn unreadable_battery_is_skipped() {
        let dir = PowerSupplyDir::new();
        dir.supply("BAT0", &[("type", "Battery"), ("status", "Discharging")]);
        assert_eq!(dir.power(), None);
    }
}
