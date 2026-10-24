//! Pure next-run math for the sync scheduler. The local UTC offset is injected: production passes `local_offset`,
//! tests a fake zone with DST changes, so one code path serves every platform and zone.
use time::{Duration, OffsetDateTime, PlainDateTime, Time, UtcOffset};

use crate::dto::sync::SyncSchedule;

/// The system zone's offset at `t`; UTC when the OS cannot tell.
pub fn local_offset(t: OffsetDateTime) -> UtcOffset {
    UtcOffset::local_offset_at(t).unwrap_or(UtcOffset::UTC)
}

/// The first automatic run strictly after `last`. None when the schedule is off (or invalid).
/// Interval: absolute time, DST-neutral. Daily: the first selected weekday whose local `time` resolves to an instant
/// after `last` (a time inside a DST gap moves forward, an ambiguous time takes the earlier instant).
pub fn next_run(s: &SyncSchedule, last: OffsetDateTime, offset_at: &impl Fn(OffsetDateTime) -> UtcOffset) -> Option<OffsetDateTime> {
    match s {
        SyncSchedule::Off => None,
        SyncSchedule::Interval { minutes } => Some(last + Duration::minutes(i64::from(*minutes))),
        SyncSchedule::Daily { time, weekdays } => {
            let minute = SyncSchedule::parse_time(time)?;
            let mask = SyncSchedule::weekday_mask(weekdays)?;
            let wall = Time::from_hms((minute / 60) as u8, (minute % 60) as u8, 0).ok()?;
            let mut day = last.to_offset(offset_at(last)).date();
            for _ in 0..=8 {
                if mask & (1 << (day.weekday().number_from_monday() - 1)) != 0 {
                    let at = resolve(PlainDateTime::new(day, wall), offset_at);
                    if at > last {
                        return Some(at);
                    }
                }
                day = day.next_day()?;
            }
            None
        }
    }
}

/// When the next automatic run is due. Off → None. Never ran → `now` (overdue). Otherwise the first slot after the
/// last run, which is in the past after downtime: that is the one catch-up run, after which `last` becomes now.
/// A last run in the future (clock moved back) counts as `now`.
pub fn due_at(s: &SyncSchedule, last: Option<OffsetDateTime>, now: OffsetDateTime, offset_at: &impl Fn(OffsetDateTime) -> UtcOffset) -> Option<OffsetDateTime> {
    match (s, last) {
        (SyncSchedule::Off, _) => None,
        (_, None) => Some(now),
        (_, Some(t)) => next_run(s, t.min(now), offset_at),
    }
}

/// Local wall time → instant. Candidates use the offsets one day before and after; a candidate is valid when it
/// maps back to `w`. Both valid (normal day or fold) → the earlier; none valid (gap) → the pre-transition offset.
fn resolve(w: PlainDateTime, offset_at: &impl Fn(OffsetDateTime) -> UtcOffset) -> OffsetDateTime {
    let t1 = w.assume_offset(offset_at((w - Duration::days(1)).assume_utc()));
    let t2 = w.assume_offset(offset_at((w + Duration::days(1)).assume_utc()));
    let valid = |t: OffsetDateTime| {
        let local = t.to_offset(offset_at(t));
        local.date() == w.date() && local.time() == w.time()
    };
    match (valid(t1), valid(t2)) {
        (true, true) => t1.min(t2),
        (false, true) => t2,
        _ => t1,
    }
}

#[cfg(test)]
mod tests {
    use time::{Date, Month};

    use super::*;

    /// America/New_York in 2026: EDT (−4) from 2026-03-08 07:00 UTC to 2026-11-01 06:00 UTC, else EST (−5).
    fn new_york(t: OffsetDateTime) -> UtcOffset {
        let utc = |m, d, h| Date::from_calendar_date(2026, m, d).unwrap().with_hms(h, 0, 0).unwrap().assume_utc();
        let dst = t >= utc(Month::March, 8, 7) && t < utc(Month::November, 1, 6);
        UtcOffset::from_hms(if dst { -4 } else { -5 }, 0, 0).unwrap()
    }

    /// Wall time in New York with an explicit offset (−4 or −5).
    fn ny(m: Month, d: u8, h: u8, min: u8, off: i8) -> OffsetDateTime {
        Date::from_calendar_date(2026, m, d).unwrap().with_hms(h, min, 0).unwrap().assume_offset(UtcOffset::from_hms(off, 0, 0).unwrap())
    }

    fn daily(time: &str, weekdays: &[u8]) -> SyncSchedule {
        SyncSchedule::Daily { time: time.into(), weekdays: weekdays.to_vec() }
    }

    const ALL_DAYS: [u8; 7] = [1, 2, 3, 4, 5, 6, 7];

    #[test]
    fn schedule_interval_adds_minutes_across_dst() {
        let s = SyncSchedule::Interval { minutes: 60 };
        // 01:30 EST on the spring-forward night: one absolute hour later is 03:30 EDT.
        assert_eq!(next_run(&s, ny(Month::March, 8, 1, 30, -5), &new_york), Some(ny(Month::March, 8, 3, 30, -4)));
    }

    #[test]
    fn schedule_daily_later_today_then_tomorrow() {
        let s = daily("09:00", &ALL_DAYS);
        // Thursday 2026-06-11 08:00 → same day 09:00; 09:00 itself → next day (strictly after).
        assert_eq!(next_run(&s, ny(Month::June, 11, 8, 0, -4), &new_york), Some(ny(Month::June, 11, 9, 0, -4)));
        assert_eq!(next_run(&s, ny(Month::June, 11, 9, 0, -4), &new_york), Some(ny(Month::June, 12, 9, 0, -4)));
    }

    #[test]
    fn schedule_daily_weekday_wraps_into_next_week() {
        // Friday 2026-06-12 10:00 with Monday only → Monday 2026-06-15.
        assert_eq!(next_run(&daily("09:00", &[1]), ny(Month::June, 12, 10, 0, -4), &new_york), Some(ny(Month::June, 15, 9, 0, -4)));
        // Monday 2026-06-15 10:00 with Monday only → a full week ahead.
        assert_eq!(next_run(&daily("09:00", &[1]), ny(Month::June, 15, 10, 0, -4), &new_york), Some(ny(Month::June, 22, 9, 0, -4)));
        // Local date, not UTC date: Sunday 21:00 EDT is Monday 01:00 UTC; Sunday-only at 22:00 is still tonight.
        assert_eq!(next_run(&daily("22:00", &[7]), ny(Month::June, 14, 21, 0, -4), &new_york), Some(ny(Month::June, 14, 22, 0, -4)));
    }

    #[test]
    fn schedule_daily_dst_gap_moves_forward() {
        // 2026-03-08 02:30 does not exist in New York → 03:30 EDT (07:30 UTC).
        let got = next_run(&daily("02:30", &ALL_DAYS), ny(Month::March, 7, 12, 0, -5), &new_york).unwrap();
        assert_eq!(got, ny(Month::March, 8, 3, 30, -4));
        assert_eq!(got.unix_timestamp(), ny(Month::March, 8, 7, 30, 0).unix_timestamp());
    }

    #[test]
    fn schedule_daily_dst_fold_takes_earlier_instant() {
        // 2026-11-01 01:30 happens twice → the EDT one (05:30 UTC), not EST (06:30 UTC).
        let got = next_run(&daily("01:30", &ALL_DAYS), ny(Month::October, 31, 12, 0, -4), &new_york).unwrap();
        assert_eq!(got.unix_timestamp(), ny(Month::November, 1, 5, 30, 0).unix_timestamp());
        // After the first 01:30 ran, the second 01:30 is not a new slot: next is the following day.
        assert_eq!(next_run(&daily("01:30", &ALL_DAYS), got, &new_york), Some(ny(Month::November, 2, 1, 30, -5)));
    }

    #[test]
    fn schedule_off_never_runs() {
        let now = ny(Month::June, 11, 8, 0, -4);
        assert_eq!(next_run(&SyncSchedule::Off, now, &new_york), None);
        assert_eq!(due_at(&SyncSchedule::Off, None, now, &new_york), None, "no catch-up when off");
        assert_eq!(due_at(&SyncSchedule::Off, Some(now - Duration::days(30)), now, &new_york), None);
    }

    #[test]
    fn schedule_catch_up_runs_once_after_downtime() {
        let s = daily("09:00", &ALL_DAYS);
        let now = ny(Month::June, 20, 12, 0, -4);
        // Ten days closed: ten slots missed, the loop runs once (then last = now) and the next slot is tomorrow.
        let mut last = Some(now - Duration::days(10));
        let mut runs = 0;
        while due_at(&s, last, now, &new_york).is_some_and(|d| d <= now) {
            runs += 1;
            last = Some(now);
            assert!(runs < 5, "catch-up loops");
        }
        assert_eq!(runs, 1);
        assert_eq!(due_at(&s, last, now, &new_york), Some(ny(Month::June, 21, 9, 0, -4)));
        // Never ran → due now; clock moved back (last in the future) → measured from now.
        assert_eq!(due_at(&s, None, now, &new_york), Some(now));
        assert_eq!(due_at(&s, Some(now + Duration::days(3)), now, &new_york), Some(ny(Month::June, 21, 9, 0, -4)));
    }

    #[test]
    fn schedule_validate_rejects_bad_input() {
        let ok = [SyncSchedule::Off, SyncSchedule::Interval { minutes: 5 }, SyncSchedule::Interval { minutes: 10080 }, daily("23:59", &[7, 1])];
        for s in ok {
            assert!(s.validate().is_ok(), "{s:?}");
        }
        let bad = [
            SyncSchedule::Interval { minutes: 4 },
            SyncSchedule::Interval { minutes: 10081 },
            daily("24:00", &[1]),
            daily("9:00", &[1]),
            daily("09:60", &[1]),
            daily("09:00", &[]),
            daily("09:00", &[0]),
            daily("09:00", &[8]),
            daily("09:00", &[2, 2]),
        ];
        for s in bad {
            assert!(s.validate().is_err(), "{s:?}");
        }
    }
}
