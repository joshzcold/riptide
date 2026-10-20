//! Checks that run every few days in the background, such as for plugin and
//! extension updates. A file holds when each last ran.

use std::path::Path;

const DAY: u64 = 86_400;

/// Whether the check whose last run `file` holds is due, `days` apart (0:
/// never), and if so, records now as its last run.
pub fn due(file: &Path, days: i64) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let last = std::fs::read_to_string(file)
        .ok()
        .and_then(|t| t.trim().parse::<u64>().ok());
    let (due, record) = decide(last, now, days);
    if record {
        let _ = std::fs::write(file, now.to_string());
    }
    due
}

/// Whether to run now, and whether to record now as the last run. The first
/// time only starts the clock, so what was just installed counts as current.
fn decide(last: Option<u64>, now: u64, days: i64) -> (bool, bool) {
    let Ok(days) = u64::try_from(days) else {
        return (false, false);
    };
    match last {
        _ if days == 0 => (false, false),
        None => (false, true),
        Some(last) => {
            let due = now.saturating_sub(last) >= days * DAY;
            (due, due)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_run_starts_the_clock_and_later_runs_wait_the_interval() {
        let now = 100 * DAY;
        assert_eq!(decide(None, now, 1), (false, true));
        assert_eq!(decide(Some(now - DAY + 1), now, 1), (false, false));
        assert_eq!(decide(Some(now - DAY), now, 1), (true, true));
        assert_eq!(decide(Some(now - 6 * DAY), now, 7), (false, false));
        assert_eq!(decide(Some(0), now, 7), (true, true));
    }

    #[test]
    fn zero_days_never_checks() {
        assert_eq!(decide(None, 100 * DAY, 0), (false, false));
        assert_eq!(decide(Some(0), 100 * DAY, 0), (false, false));
        assert_eq!(decide(Some(0), 100 * DAY, -1), (false, false));
    }
}
