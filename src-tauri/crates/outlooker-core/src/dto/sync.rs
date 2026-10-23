use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::error::AppError;

/// When registered PST paths sync automatically. `off` = only "Sync now". `daily.time` is local "HH:MM";
/// `daily.weekdays` are ISO weekdays (1 = Monday … 7), non-empty and unique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SyncSchedule {
    Off,
    Interval { minutes: u32 },
    Daily { time: String, weekdays: Vec<u8> },
}

pub const INTERVAL_MINUTES: std::ops::RangeInclusive<u32> = 5..=10080;

impl SyncSchedule {
    /// `daily.time` as minutes after local midnight, None when not "HH:MM" in 00:00..=23:59.
    pub fn parse_time(time: &str) -> Option<u16> {
        let (h, m) = time.split_once(':')?;
        if h.len() != 2 || m.len() != 2 {
            return None;
        }
        let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
        (h < 24 && m < 60).then_some(h * 60 + m)
    }

    /// ISO weekdays as a bit mask (bit 0 = Monday), None when empty, out of 1..=7 or repeated.
    pub fn weekday_mask(weekdays: &[u8]) -> Option<u8> {
        let mask = weekdays.iter().try_fold(0u8, |m, &d| {
            let bit = 1u8.checked_shl(u32::from(d).checked_sub(1)?)?;
            ((1..=7).contains(&d) && m & bit == 0).then_some(m | bit)
        })?;
        (mask != 0).then_some(mask)
    }

    pub fn validate(&self) -> Result<(), AppError> {
        let ok = match self {
            SyncSchedule::Off => true,
            SyncSchedule::Interval { minutes } => INTERVAL_MINUTES.contains(minutes),
            SyncSchedule::Daily { time, weekdays } => Self::parse_time(time).is_some() && Self::weekday_mask(weekdays).is_some(),
        };
        if ok { Ok(()) } else { Err(AppError::invalid("schedule", "outOfRange")) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub schedule: SyncSchedule,
    /// RFC 3339 UTC, null before the first automatic run.
    pub last_run_at: Option<String>,
    /// RFC 3339 UTC; null when the schedule is off. A past or present instant means "due now".
    pub next_run_at: Option<String>,
}
