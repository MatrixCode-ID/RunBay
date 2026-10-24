//! Conversions from runner and data types to the Slint models.

use chrono::{DateTime, FixedOffset, Local, TimeZone, Utc};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ModelRc, VecModel};

use crate::ansi::AnsiParser;
use crate::data::RunRecord;
use crate::runner::{OutputLine, RunInfo};
use crate::{OutputRow, RunRow, Span};

/// The Win32 window handle of a Slint window, once it is shown.
pub fn hwnd(window: &slint::Window) -> Option<isize> {
    let handle = window.window_handle();
    match handle.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// The uppercase first letter of a name, for avatars.
pub fn initial(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into())
}

/// Converts one output line into styled spans.
pub fn output_row(parser: &mut AnsiParser, line: &OutputLine, index: u64) -> OutputRow {
    let spans: Vec<Span> = parser
        .parse_line(&line.text)
        .into_iter()
        .map(|s| {
            let (colored, color, dark_color) = match s.fg {
                Some(c) => {
                    let (r, g, b) = c.to_rgb(false);
                    let (dr, dg, db) = c.to_rgb(true);
                    (
                        true,
                        slint::Color::from_rgb_u8(r, g, b),
                        slint::Color::from_rgb_u8(dr, dg, db),
                    )
                }
                None => (false, slint::Color::default(), slint::Color::default()),
            };
            Span {
                text: s.text.into(),
                color,
                dark_color,
                colored,
                bold: s.bold,
            }
        })
        .collect();
    OutputRow {
        run_index: index.min(i32::MAX as u64) as i32,
        plain_text: spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>()
            .into(),
        selected_from: -1,
        selected_to: -1,
        spans: ModelRc::new(VecModel::from(spans)),
        stderr: line.stderr,
    }
}

/// Short time label: `HH:mm` today, `dd MMM HH:mm` otherwise.
pub fn time_label(start: &DateTime<FixedOffset>) -> String {
    let local = start.with_timezone(&Local);
    if local.date_naive() == Local::now().date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%d %b %H:%M").to_string()
    }
}

/// Short duration label: `850 ms`, `12.3 s`, `4 min 05 s`, `1 h 02 min`.
pub fn duration_label(d: chrono::Duration) -> String {
    let ms = d.num_milliseconds().max(0);
    match ms {
        0..1_000 => format!("{ms} ms"),
        1_000..60_000 => format!("{:.1} s", ms as f64 / 1000.0),
        60_000..3_600_000 => format!("{} min {:02} s", ms / 60_000, ms / 1000 % 60),
        _ => format!("{} h {:02} min", ms / 3_600_000, ms / 60_000 % 60),
    }
}

/// The moment `Clock.now` counts from: the first time a clock value is needed.
fn clock_base() -> DateTime<Utc> {
    static BASE: std::sync::OnceLock<DateTime<Utc>> = std::sync::OnceLock::new();
    *BASE.get_or_init(Utc::now)
}

/// `t` in `Clock` seconds; earlier runs are negative.
pub fn clock_seconds<Tz: TimeZone>(t: &DateTime<Tz>) -> i32 {
    (t.with_timezone(&Utc) - clock_base()).num_seconds() as i32
}

/// The current `Clock.now` value.
pub fn clock_now() -> i32 {
    clock_seconds(&Utc::now())
}

/// Final duration label of a finished run; empty while running or when the end is unknown.
pub fn final_duration(
    start: &DateTime<FixedOffset>,
    end: Option<&DateTime<FixedOffset>>,
) -> String {
    end.map(|end| duration_label(*end - *start))
        .unwrap_or_default()
}

/// A run row for the navigation (`tab == false`) or the tab strip.
pub fn run_row(info: &RunInfo, displayed: Option<&str>, tab: bool) -> RunRow {
    RunRow {
        run_id: info.run_id.clone().into(),
        title: if tab {
            format!("{} · {}", info.app_name, info.action_name)
        } else {
            info.action_name.clone()
        }
        .into(),
        status: info.status.as_str().into(),
        busy: info.is_busy(),
        agent: info.agent.clone().unwrap_or_default().into(),
        time: time_label(&info.start).into(),
        duration: final_duration(&info.start, info.end.as_ref()).into(),
        started: clock_seconds(&info.start),
        selected: displayed == Some(info.run_id.as_str()),
    }
}

/// A run row for a history record.
pub fn record_row(record: &RunRecord, action_name: &str, displayed: Option<&str>) -> RunRow {
    RunRow {
        run_id: record.run_id.clone().into(),
        title: action_name.into(),
        status: record.status.as_str().into(),
        busy: false,
        agent: record.agent.clone().unwrap_or_default().into(),
        time: time_label(&record.start).into(),
        duration: final_duration(&record.start, record.end.as_ref()).into(),
        started: clock_seconds(&record.start),
        selected: displayed == Some(record.run_id.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn output_keeps_both_palettes_and_preserves_explicit_colors() {
        let mut parser = AnsiParser::new();
        let row = output_row(
            &mut parser,
            &OutputLine {
                text: "\x1b[31mred\x1b[38;5;200mindexed\x1b[38;2;12;34;56mrgb\x1b[0mplain".into(),
                stderr: false,
            },
            42,
        );
        assert_eq!(row.plain_text, "redindexedrgbplain");
        assert_eq!(row.run_index, 42);
        let spans: Vec<_> = row.spans.iter().collect();
        assert_ne!(spans[0].color, spans[0].dark_color);
        assert_eq!(spans[1].color, spans[1].dark_color);
        assert_eq!(spans[2].color, slint::Color::from_rgb_u8(12, 34, 56));
        assert_eq!(spans[2].color, spans[2].dark_color);
        assert!(!spans[3].colored);
    }
}
