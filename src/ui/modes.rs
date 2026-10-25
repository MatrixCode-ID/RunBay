//! Display modes: switching between Full, Compact, and Icon, their window positions,
//! always on top, the floating icon's drag and edge snap, and the per-app output popup.

use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, PhysicalPosition, PhysicalSize, SharedString, VecModel};

use super::{Controller, convert, with_controller};
use crate::ansi::AnsiParser;
use crate::data::{IconSize, Mode, WindowRect, WindowState};
use crate::platform::chrome::{self, Kind};
use crate::platform::{self, monitors};
use crate::runner::{OutputLine, RunRequest};
use crate::{CompactWindow, IconPopup, IconWindow, MiniBridge, OutputRow};

/// The Compact and Icon windows (created once, shown when their mode is active).
pub struct ModeWindows {
    pub compact: CompactWindow,
    pub icon: IconWindow,
    pub popup: IconPopup,
}

/// The run shown in the output popup of Compact and Icon mode.
#[derive(Default)]
pub struct PopupState {
    pub run_id: Option<String>,
    pub output: Rc<VecModel<OutputRow>>,
    pub parser: AnsiParser,
    pub selection: super::selection::Selection,
}

/// Edge snapping of the floating icon: the nearest left or right edge of the work area of
/// the monitor under the icon center, with an 8 px margin. Returns the rectangle and
/// whether it sits on the right edge.
pub fn snap(rect: WindowRect, areas: &[monitors::Area]) -> (WindowRect, bool) {
    let rect = monitors::fit(rect, areas);
    let (cx, cy) = (
        rect.x + rect.width as i32 / 2,
        rect.y + rect.height as i32 / 2,
    );
    let area = areas
        .iter()
        .copied()
        .find(|&(l, t, r, b)| cx >= l && cx < r && cy >= t && cy < b)
        .or_else(|| areas.first().copied());
    let Some((l, t, r, b)) = area else {
        return (rect, false);
    };
    let margin = 8;
    let (w, h) = (rect.width as i32, rect.height as i32);
    let right = cx > (l + r) / 2;
    let x = if right { r - w - margin } else { l + margin };
    let y = rect.y.clamp(t + margin, (b - h - margin).max(t + margin));
    (
        WindowRect {
            x,
            y,
            width: rect.width,
            height: rect.height,
        },
        right,
    )
}

fn window_rect(window: &slint::Window) -> WindowRect {
    let p = window.position();
    let s = window.size();
    WindowRect {
        x: p.x,
        y: p.y,
        width: s.width,
        height: s.height,
    }
}

impl Controller {
    /// Calls `f` with the shared data global of the Compact and the Icon window.
    pub(super) fn each_mini(&self, f: impl Fn(MiniBridge<'_>)) {
        f(self.modes.compact.global::<MiniBridge>());
        f(self.modes.icon.global::<MiniBridge>());
        f(self.modes.popup.global::<MiniBridge>());
    }

    /// Switches to `mode` and shows its window.
    pub(super) fn switch_mode(&self, mode: Mode) {
        self.save_window_state();
        self.state.borrow_mut().mode = mode;
        if mode != Mode::Full {
            let _ = self.ui().hide();
        }
        if mode != Mode::Compact {
            let _ = self.modes.compact.hide();
        }
        if mode != Mode::Icon {
            let _ = self.modes.icon.hide();
            let _ = self.modes.popup.hide();
        }
        self.apply_mode_settings();
        self.show_window();
        self.save_window_state();
        self.refresh_tray();
    }

    /// Shows the window of the active mode.
    pub(super) fn show_window(&self) {
        let state = self.store.load_window_state();
        let mode = self.state.borrow().mode;
        match mode {
            Mode::Full => self.focus_full(),
            Mode::Compact => {
                let window = self.modes.compact.window();
                if !window.is_visible() {
                    let rect = state.compact.unwrap_or_else(|| self.default_compact_rect());
                    let rect = monitors::fit(rect, &monitors::work_areas());
                    window.set_position(PhysicalPosition::new(rect.x, rect.y));
                    window.set_size(PhysicalSize::new(rect.width, rect.height));
                }
                let _ = self.modes.compact.show();
                self.apply_taskbar_buttons();
                if let Some(hwnd) = convert::hwnd(window) {
                    platform::bring_to_front(hwnd);
                }
            }
            Mode::Icon => {
                let window = self.modes.icon.window();
                if !window.is_visible() {
                    let px = self.icon_physical_size();
                    let rect = state.icon.unwrap_or_else(|| self.default_icon_rect(px));
                    let (rect, right) = snap(
                        WindowRect {
                            width: px,
                            height: px,
                            ..rect
                        },
                        &monitors::work_areas(),
                    );
                    self.state.borrow_mut().icon_right = right;
                    window.set_position(PhysicalPosition::new(rect.x, rect.y));
                }
                let _ = self.modes.icon.show();
                if let Some(hwnd) = convert::hwnd(window) {
                    platform::set_tool_window(hwnd);
                }
            }
        }
    }

    fn icon_physical_size(&self) -> u32 {
        let logical = self.settings.read().unwrap().icon_size.px() as f32;
        (logical * self.modes.icon.window().scale_factor()).round() as u32
    }

    fn primary_area(&self) -> monitors::Area {
        monitors::work_areas()
            .first()
            .copied()
            .unwrap_or((0, 0, 1280, 720))
    }

    fn default_compact_rect(&self) -> WindowRect {
        let (_, _, r, b) = self.primary_area();
        let scale = self.modes.compact.window().scale_factor();
        let (w, h) = ((360.0 * scale) as i32, (420.0 * scale) as i32);
        WindowRect {
            x: r - w - 24,
            y: b - h - 24,
            width: w as u32,
            height: h as u32,
        }
    }

    fn default_icon_rect(&self, px: u32) -> WindowRect {
        let (_, t, r, b) = self.primary_area();
        WindowRect {
            x: r - px as i32 - 8,
            y: t + (b - t) / 2,
            width: px,
            height: px,
        }
    }

    /// Toggles always on top for the active mode (Full mode does not use it).
    pub(super) fn toggle_always_on_top(&self) {
        let mut s = self.settings.read().unwrap().clone();
        match self.state.borrow().mode {
            Mode::Full => return,
            Mode::Compact => s.always_on_top_compact = !s.always_on_top_compact,
            Mode::Icon => s.always_on_top_icon = !s.always_on_top_icon,
        }
        self.apply_settings(s);
    }

    /// Applies always on top and the icon size to the mode windows.
    pub(super) fn apply_mode_settings(&self) {
        let s = self.settings.read().unwrap().clone();
        self.modes.compact.set_pinned(s.always_on_top_compact);
        self.modes.icon.set_pinned(s.always_on_top_icon);
        self.modes.popup.set_pinned(s.always_on_top_icon);
        self.modes.icon.set_size(s.icon_size.px() as f32);
        self.apply_taskbar_buttons();
    }

    /// Shows or hides the taskbar buttons of the Full and Compact windows. It runs after
    /// every show, because the windowing backend may restore its own extended style.
    pub(super) fn apply_taskbar_buttons(&self) {
        let s = self.settings.read().unwrap();
        let ui = self.ui();
        for (window, visible, kind) in [
            (ui.window(), s.taskbar_full, Kind::Full),
            (
                self.modes.compact.window(),
                s.taskbar_compact,
                Kind::Compact,
            ),
        ] {
            if let Some(hwnd) = convert::hwnd(window) {
                if kind == Kind::Full {
                    self.install_full_chrome(hwnd);
                } else {
                    chrome::install(hwnd, Kind::Compact, |event| {
                        super::lifecycle::chrome_event(Kind::Compact, event);
                    });
                    self.sync_caption_metrics(Kind::Compact);
                }
                platform::set_taskbar_button(hwnd, visible);
            }
        }
    }

    /// Records the rectangles of the visible Compact and Icon windows.
    pub(super) fn save_mode_windows(&self, state: &mut WindowState) {
        let compact = self.modes.compact.window();
        if compact.is_visible() && !compact.is_minimized() {
            state.compact = Some(window_rect(compact));
        }
        let icon = self.modes.icon.window();
        if icon.is_visible() {
            state.icon = Some(window_rect(icon));
        }
    }

    /// Shows the output of an app's last run in the popup.
    fn open_output(&self, app_id: &str) {
        let run_id = self
            .manager
            .last_for_app(app_id)
            .map(|r| r.id())
            .or_else(|| {
                self.state
                    .borrow()
                    .history
                    .get(app_id)
                    .and_then(|h| h.last())
                    .map(|r| r.run_id.clone())
            });
        let limit = self.settings.read().unwrap().output_line_limit.max(100) as usize;
        let (first, lines): (u64, Vec<OutputLine>) = match &run_id {
            Some(id) => match self.manager.find(id) {
                Some(run) => run.lines(),
                None => self
                    .find_record(id)
                    .and_then(|(_, path)| std::fs::read_to_string(path).ok())
                    .map(|text| {
                        let all: Vec<&str> = text.lines().collect();
                        let first = all.len().saturating_sub(limit);
                        let lines = all[first..]
                            .iter()
                            .map(|line| OutputLine {
                                text: (*line).to_string(),
                                stderr: false,
                            })
                            .collect();
                        (first as u64, lines)
                    })
                    .unwrap_or_default(),
            },
            None => (0, Vec::new()),
        };
        {
            let mut st = self.state.borrow_mut();
            st.popup.run_id = run_id;
            st.popup.parser = AnsiParser::new();
            st.popup.selection.reset(first);
            st.popup.output.set_vec(Vec::new());
        }
        self.append_popup_lines(&lines);
        self.refresh_popup_header();
        for bridge in [
            self.modes.compact.global::<crate::OutputBridge>(),
            self.modes.popup.global::<crate::OutputBridge>(),
        ] {
            bridge.set_popup_generation(bridge.get_popup_generation().wrapping_add(1));
        }
    }

    pub(super) fn append_popup_lines(&self, lines: &[OutputLine]) {
        let mut st = self.state.borrow_mut();
        let limit = self.settings.read().unwrap().output_line_limit.max(100) as usize;
        let model = st.popup.output.clone();
        let rows: Vec<OutputRow> = lines
            .iter()
            .map(|line| {
                let index = st.popup.selection.next();
                let row = convert::output_row(&mut st.popup.parser, line, index);
                st.popup
                    .selection
                    .lines
                    .push_back(row.plain_text.to_string());
                row
            })
            .collect();
        st.popup.selection.trim(limit);
        drop(st);
        for row in rows {
            model.push(row);
        }
        let excess = slint::Model::row_count(&*model).saturating_sub(limit);
        for _ in 0..excess {
            model.remove(0);
        }
        let columns = slint::Model::iter(&*model)
            .map(|row| row.plain_text.chars().count())
            .max()
            .unwrap_or(0);
        self.each_mini(|bridge| bridge.set_output_columns(columns as i32));
        self.refresh_selection(true);
    }

    /// Updates the popup title, status, and agent.
    pub(super) fn refresh_popup_header(&self) {
        let run_id = self.state.borrow().popup.run_id.clone();
        let busy = run_id
            .as_deref()
            .and_then(|id| self.manager.find(id))
            .is_some_and(|run| run.info().is_busy());
        let (title, status, agent, duration, started) = match run_id.as_deref() {
            Some(id) => match self.manager.find(id) {
                Some(run) => {
                    let i = run.info();
                    (
                        format!("{} · {}", i.app_name, i.action_name),
                        i.status.as_str().to_string(),
                        i.agent.clone().unwrap_or_default(),
                        convert::final_duration(&i.start, i.end.as_ref()),
                        convert::clock_seconds(&i.start),
                    )
                }
                None => match self.find_record(id) {
                    Some((r, _)) => (
                        format!(
                            "{} · {}",
                            self.app_name_of_record(&r).unwrap_or_default(),
                            r.action_id
                        ),
                        r.status.as_str().to_string(),
                        r.agent.clone().unwrap_or_default(),
                        convert::final_duration(&r.start, r.end.as_ref()),
                        convert::clock_seconds(&r.start),
                    ),
                    None => Default::default(),
                },
            },
            None => Default::default(),
        };
        let output = ModelRc::from(self.state.borrow().popup.output.clone());
        self.each_mini(|g| {
            g.set_output(output.clone());
            g.set_output_title(title.clone().into());
            g.set_output_status(status.clone().into());
            g.set_output_duration(duration.clone().into());
            g.set_output_started(started);
            g.set_output_busy(busy);
            g.set_output_agent(agent.clone().into());
            g.set_output_run_id(run_id.clone().unwrap_or_default().into());
        });
    }

    /// Starts polling the mouse button after a press on the floating icon, so the icon
    /// snaps to the nearest edge once a drag ends.
    fn icon_pressed(&self) {
        let start = window_rect(self.modes.icon.window());
        let timer = Rc::new(slint::Timer::default());
        let keep = Rc::downgrade(&timer);
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(30),
            move || {
                if platform::left_button_down() {
                    return;
                }
                if let Some(t) = keep.upgrade() {
                    t.stop();
                }
                with_controller(|c| c.icon_released(start));
            },
        );
        *self.icon_timer.borrow_mut() = Some(timer);
    }

    fn icon_released(&self, start: WindowRect) {
        let now = window_rect(self.modes.icon.window());
        if (now.x - start.x).abs() < 3 && (now.y - start.y).abs() < 3 {
            return;
        }
        let (rect, right) = snap(now, &monitors::work_areas());
        self.modes
            .icon
            .window()
            .set_position(PhysicalPosition::new(rect.x, rect.y));
        self.state.borrow_mut().icon_right = right;
        self.save_window_state();
    }

    fn icon_menu(&self, item: &str) {
        match item {
            "full" => self.switch_mode(Mode::Full),
            "compact" => self.switch_mode(Mode::Compact),
            "pin" => self.toggle_always_on_top(),
            "copy-mcp-setup" => self.copy_mcp_setup(),
            "small" | "medium" | "large" => {
                let mut s = self.settings.read().unwrap().clone();
                s.icon_size = match item {
                    "small" => IconSize::Small,
                    "large" => IconSize::Large,
                    _ => IconSize::Medium,
                };
                self.apply_settings(s);
                // Re-snap so the icon stays on its edge with the new size.
                let px = self.icon_physical_size();
                let window = self.modes.icon.window();
                let rect = WindowRect {
                    width: px,
                    height: px,
                    ..window_rect(window)
                };
                let (rect, right) = snap(rect, &monitors::work_areas());
                window.set_position(PhysicalPosition::new(rect.x, rect.y));
                self.state.borrow_mut().icon_right = right;
            }
            "exit" => self.request_exit(),
            _ => {}
        }
    }

    pub(super) fn bind_modes(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.modes.compact.on_cycle_theme(move || {
            if let Some(c) = weak.upgrade() {
                c.cycle_theme();
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.compact.on_caption_minimize(move || {
            if let Some(c) = weak.upgrade() {
                c.caption_minimize(Kind::Compact);
            }
        });
        let ui = self.modes.compact.as_weak();
        self.modes.compact.on_caption_close(move || {
            if let Some(ui) = ui.upgrade()
                && let Some(hwnd) = convert::hwnd(ui.window())
            {
                chrome::close(hwnd);
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.compact.on_caption_metrics_changed(move || {
            if let Some(c) = weak.upgrade() {
                c.sync_caption_metrics(Kind::Compact);
            }
        });
        let weak = Rc::downgrade(self);
        self.ui().on_switch_mode(move |mode| {
            if let Some(c) = weak.upgrade() {
                c.switch_mode(mode_of(&mode));
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.compact.on_toggle_pin(move || {
            if let Some(c) = weak.upgrade() {
                c.toggle_always_on_top();
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.compact.window().on_close_requested(move || {
            if let Some(c) = weak.upgrade() {
                c.save_window_state();
            }
            slint::CloseRequestResponse::HideWindow
        });
        let weak = Rc::downgrade(self);
        self.modes.icon.on_pressed(move || {
            if let Some(c) = weak.upgrade() {
                c.icon_pressed();
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.icon.on_clicked(move || {
            if let Some(c) = weak.upgrade() {
                c.toggle_icon_popup("apps");
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.icon.on_right_clicked(move || {
            if let Some(c) = weak.upgrade() {
                c.toggle_icon_popup("menu");
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.popup.on_menu_action(move |item| {
            if let Some(c) = weak.upgrade() {
                c.hide_icon_popup();
                c.icon_menu(&item);
            }
        });
        let weak = Rc::downgrade(self);
        self.modes.popup.on_dismissed(move || {
            if let Some(c) = weak.upgrade() {
                c.hide_icon_popup();
            }
        });
        self.modes
            .icon
            .window()
            .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);

        let version = SharedString::from(crate::VERSION);
        let bind = |g: MiniBridge<'_>| {
            g.set_version(version.clone());
            let weak = Rc::downgrade(self);
            g.on_copy_mcp_setup(move || {
                if let Some(c) = weak.upgrade() {
                    c.copy_mcp_setup();
                }
            });
            let weak = Rc::downgrade(self);
            g.on_run_action(move |app, action| {
                if let Some(c) = weak.upgrade() {
                    c.start(RunRequest::ui(&app, &action));
                }
            });
            let weak = Rc::downgrade(self);
            g.on_open_output(move |app| {
                if let Some(c) = weak.upgrade() {
                    c.open_output(&app);
                }
            });
            let weak = Rc::downgrade(self);
            g.on_stop_run(move |run| {
                if let Some(c) = weak.upgrade() {
                    c.manager.stop(&run);
                }
            });
            let weak = Rc::downgrade(self);
            g.on_stop_app(move |app| {
                if let Some(c) = weak.upgrade() {
                    c.stop_app(&app);
                }
            });
            let weak = Rc::downgrade(self);
            g.on_open_in_full(move |run| {
                if let Some(c) = weak.upgrade() {
                    c.switch_mode(Mode::Full);
                    if !run.is_empty() {
                        c.display(Some(run.to_string()));
                        c.refresh_nav();
                    }
                }
            });
            let weak = Rc::downgrade(self);
            g.on_switch_mode(move |mode| {
                if let Some(c) = weak.upgrade() {
                    c.switch_mode(mode_of(&mode));
                }
            });
            let weak = Rc::downgrade(self);
            g.on_version_clicked(move || {
                if let Some(c) = weak.upgrade() {
                    c.switch_mode(Mode::Full);
                    c.show_about();
                }
            });
            let weak = Rc::downgrade(self);
            g.on_undo_clicked(move || {
                if let Some(c) = weak.upgrade() {
                    c.undo();
                }
            });
        };
        bind(self.modes.compact.global::<MiniBridge>());
        bind(self.modes.icon.global::<MiniBridge>());
        bind(self.modes.popup.global::<MiniBridge>());
        self.apply_mode_settings();
    }

    /// Opens the popup next to the floating icon with `view` ("apps", "menu", or
    /// "snackbar"), or closes it when it already shows that view.
    pub(super) fn toggle_icon_popup(&self, view: &str) {
        let popup = &self.modes.popup;
        if popup.window().is_visible() && popup.get_view() == view {
            self.hide_icon_popup();
        } else {
            self.show_icon_popup(view);
        }
    }

    pub(super) fn show_icon_popup(&self, view: &str) {
        let popup = &self.modes.popup;
        popup.set_view(view.into());
        let scale = popup.window().scale_factor();
        let (w, h) = match view {
            "apps" => (320.0, 420.0),
            "menu" => (250.0, 8.0 * 50.0 + 10.0),
            _ => (300.0, 60.0),
        };
        let (w, h) = ((w * scale) as i32, (h * scale) as i32);
        let icon = window_rect(self.modes.icon.window());
        let right = self.state.borrow().icon_right;
        let x = if right {
            icon.x - w - 4
        } else {
            icon.x + icon.width as i32 + 4
        };
        let rect = monitors::fit(
            WindowRect {
                x,
                y: icon.y,
                width: w as u32,
                height: h as u32,
            },
            &monitors::work_areas(),
        );
        // Keep the popup on the icon's side even when fitting moved it.
        let window = popup.window();
        window.set_position(PhysicalPosition::new(rect.x, rect.y));
        window.set_size(PhysicalSize::new(rect.width, rect.height));
        let _ = popup.show();
        if let Some(hwnd) = convert::hwnd(window) {
            platform::set_tool_window(hwnd);
            if view != "snackbar" {
                platform::bring_to_front(hwnd);
            }
        }
        if view != "snackbar" {
            self.watch_icon_popup();
        }
    }

    pub(super) fn hide_icon_popup(&self) {
        let _ = self.modes.popup.hide();
        if let Some(t) = self.popup_timer.borrow_mut().take() {
            t.stop();
        }
    }

    /// Closes the popup when another window takes the focus (an outside click). The timer
    /// runs only while the popup is open.
    fn watch_icon_popup(&self) {
        let timer = Rc::new(slint::Timer::default());
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(200),
            move || {
                with_controller(|c| {
                    let popup = convert::hwnd(c.modes.popup.window());
                    let icon = convert::hwnd(c.modes.icon.window());
                    let front = platform::foreground_window();
                    // Popup windows opened from the popup itself (menus) belong to it too.
                    if Some(front) != popup && Some(front) != icon && !platform::same_process(front)
                    {
                        c.hide_icon_popup();
                    }
                });
            },
        );
        *self.popup_timer.borrow_mut() = Some(timer);
    }

    /// Shows the Full window above the active mode for a dialog (ask, confirmations).
    pub(super) fn show_full_for_dialog(&self) {
        if self.state.borrow().mode != Mode::Full {
            self.state.borrow_mut().full_for_dialog = true;
        }
        self.focus_full();
    }

    /// Hides the Full window again after a dialog when another mode is active.
    pub(super) fn end_full_for_dialog(&self) {
        let hide = {
            let st = self.state.borrow();
            st.full_for_dialog
                && st.confirmations.is_empty()
                && st.pending_ask.is_none()
                && st.mode != Mode::Full
        };
        if hide {
            self.state.borrow_mut().full_for_dialog = false;
            let _ = self.ui().hide();
        }
    }
}

fn mode_of(text: &str) -> Mode {
    match text {
        "compact" => Mode::Compact,
        "icon" => Mode::Icon,
        _ => Mode::Full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREAS: [monitors::Area; 2] = [(0, 0, 1920, 1040), (1920, 0, 3840, 1040)];

    fn rect(x: i32, y: i32) -> WindowRect {
        WindowRect {
            x,
            y,
            width: 56,
            height: 56,
        }
    }

    #[test]
    fn snaps_to_the_nearest_edge_of_its_monitor() {
        assert_eq!(snap(rect(300, 400), &AREAS), (rect(8, 400), false));
        assert_eq!(snap(rect(1500, 400), &AREAS), (rect(1856, 400), true));
        // On the second monitor, near its left edge.
        assert_eq!(snap(rect(2000, 10), &AREAS), (rect(1928, 10), false));
        // Dragged below the work area: kept inside.
        assert_eq!(snap(rect(100, 1030), &AREAS), (rect(8, 976), false));
    }

    #[test]
    fn off_screen_icon_comes_back() {
        let (r, _) = snap(rect(9000, 400), &AREAS);
        assert!(r.x >= 1920 && r.x + 56 <= 3840, "{r:?}");
    }
}
