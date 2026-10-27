use crate::config::TimeWindow;

pub(super) fn weekly(day: &str, start: &str, end: &str) -> TimeWindow {
    TimeWindow {
        weekdays: Some(vec![day.into()]),
        start: Some(start.into()),
        end: Some(end.into()),
        ..Default::default()
    }
}
