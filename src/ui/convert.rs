//! Conversions from runner and data types to the Slint models.

use chrono::{DateTime, FixedOffset, Local};
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
pub fn output_row(parser: &mut AnsiParser, line: &OutputLine, dark: bool) -> OutputRow {
    let spans: Vec<Span> = parser
        .parse_line(&line.text)
        .into_iter()
        .map(|s| {
            let (colored, color) = match s.fg {
                Some(c) => {
                    let (r, g, b) = c.to_rgb(dark);
                    (true, slint::Color::from_rgb_u8(r, g, b))
                }
                None => (false, slint::Color::default()),
            };
            Span {
                text: s.text.into(),
                color,
                colored,
                bold: s.bold,
            }
        })
        .collect();
    OutputRow {
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
        agent: info.agent.clone().unwrap_or_default().into(),
        time: time_label(&info.start).into(),
        selected: displayed == Some(info.run_id.as_str()),
    }
}

/// A run row for a history record.
pub fn record_row(record: &RunRecord, action_name: &str, displayed: Option<&str>) -> RunRow {
    RunRow {
        run_id: record.run_id.clone().into(),
        title: action_name.into(),
        status: record.status.as_str().into(),
        agent: record.agent.clone().unwrap_or_default().into(),
        time: time_label(&record.start).into(),
        selected: displayed == Some(record.run_id.as_str()),
    }
}
