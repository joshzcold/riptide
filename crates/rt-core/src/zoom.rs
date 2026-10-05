//! Page zoom in percent, stepping through qutebrowser's default levels.

pub const LEVELS: &[f64] = &[
    25.0, 33.0, 50.0, 67.0, 75.0, 90.0, 100.0, 110.0, 125.0, 150.0, 175.0, 200.0, 250.0, 300.0,
    400.0, 500.0,
];

/// `steps` levels up (or down if negative) from `current`; a value between
/// levels counts as the nearest one in the direction of travel.
/// Like [`step`], through `levels` (sorted, from `zoom.levels`).
pub fn step_in(levels: &[f64], current: f64, steps: i64) -> f64 {
    if levels.is_empty() {
        return current;
    }
    let current = current.clamp(levels[0], levels[levels.len() - 1]);
    let index = if steps >= 0 {
        levels
            .iter()
            .rposition(|&l| l <= current + 0.5)
            .unwrap_or(0)
    } else {
        levels
            .iter()
            .position(|&l| l >= current - 0.5)
            .unwrap_or(levels.len() - 1)
    } as i64;
    levels[(index + steps).clamp(0, levels.len() as i64 - 1) as usize]
}

pub fn step(current: f64, steps: i64) -> f64 {
    step_in(LEVELS, current, steps)
}

/// `"110%"` or `"110"` as 110.0.
pub fn parse_percent(text: &str) -> Option<f64> {
    let value: f64 = text.trim().trim_end_matches('%').trim().parse().ok()?;
    (value > 0.0 && value.is_finite()).then_some(value)
}

/// `zoom.levels` as sorted percentages.
pub fn levels_from(list: &[String]) -> Vec<f64> {
    let mut levels: Vec<f64> = list.iter().filter_map(|l| parse_percent(l)).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    levels
}

/// Chromium's zoom level for a percentage: 100% is 0, each step is 20%.
pub fn to_level(percent: f64) -> f64 {
    (percent / 100.0).ln() / 1.2_f64.ln()
}

pub fn to_percent(level: f64) -> f64 {
    100.0 * 1.2_f64.powf(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_through_levels() {
        assert_eq!(step(100.0, 1), 110.0);
        assert_eq!(step(100.0, -1), 90.0);
        assert_eq!(step(100.0, 3), 150.0);
        assert_eq!(step(500.0, 1), 500.0);
        assert_eq!(step(25.0, -2), 25.0);
        assert_eq!(
            step(105.0, 1),
            110.0,
            "between levels, up goes to the next one"
        );
        assert_eq!(step(105.0, -1), 100.0);
        assert!((to_percent(to_level(150.0)) - 150.0).abs() < 1e-9);
        assert_eq!(to_level(100.0), 0.0);
    }

    #[test]
    fn custom_levels() {
        let levels = levels_from(&["150%".into(), "100".into(), "50%".into(), "nope".into()]);
        assert_eq!(levels, [50.0, 100.0, 150.0]);
        assert_eq!(step_in(&levels, 100.0, 1), 150.0);
        assert_eq!(step_in(&levels, 100.0, -1), 50.0);
        assert_eq!(step_in(&levels, 150.0, 1), 150.0);
        assert_eq!(parse_percent("110%"), Some(110.0));
        assert_eq!(parse_percent("-5%"), None);
    }
}
