//! Connects the shared output component to each view's cached selection model.

use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, VecModel};

use super::{Controller, selection::Selection};
use crate::{OutputBridge, OutputRow, platform};

impl Controller {
    /// Reads only lines not already displayed, avoiding duplicate batched events
    /// when a run is opened between output collection and event delivery.
    pub(super) fn append_pending_lines(&self, run_id: &str, popup: bool) {
        let next = {
            let state = self.state.borrow();
            if popup {
                state.popup.selection.next()
            } else {
                state.selection.next()
            }
        };
        let Some(run) = self.manager.find(run_id) else {
            return;
        };
        let (first, lines) = run.lines_from(next);
        if first > next {
            let model = {
                let mut state = self.state.borrow_mut();
                if popup {
                    state.popup.selection.reset(first);
                    state.popup.parser = crate::ansi::AnsiParser::new();
                    state.popup.output.clone()
                } else {
                    state.selection.reset(first);
                    state.parser = crate::ansi::AnsiParser::new();
                    state.columns = 0;
                    state.output.clone()
                }
            };
            model.set_vec(Vec::new());
        }
        if popup {
            self.append_popup_lines(&lines);
        } else {
            self.append_lines(&lines);
        }
    }

    pub(super) fn bind_selection(self: &Rc<Self>) {
        let bind = |bridge: OutputBridge<'_>| {
            let weak = Rc::downgrade(self);
            bridge.on_selection_event(move |popup, kind, row, column, shift| {
                if let Some(controller) = weak.upgrade() {
                    controller.selection_event(popup, &kind, row, column, shift);
                }
            });
        };
        bind(self.ui().global::<OutputBridge>());
        bind(self.modes.compact.global::<OutputBridge>());
        bind(self.modes.popup.global::<OutputBridge>());
    }

    fn selection_event(&self, popup: bool, kind: &str, row: i32, column: i32, shift: bool) {
        if kind == "copy" {
            let text = {
                let state = self.state.borrow();
                if popup {
                    state.popup.selection.text()
                } else {
                    state.selection.text()
                }
            };
            if !text.is_empty() {
                self.report(platform::copy_text(&text));
            }
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            let mut clicks = 1;
            if kind == "start" {
                let now = Instant::now();
                if !shift
                    && let Some((previous, previous_popup, previous_row, previous_column, count)) =
                        state.selection_click
                    && previous_popup == popup
                    && previous_row == row
                    && (previous_column - column).abs() <= 1
                    && now.duration_since(previous) <= Duration::from_millis(500)
                {
                    clicks = count % 3 + 1;
                }
                state.selection_click = Some((now, popup, row, column, clicks));
            } else {
                state.selection_click = None;
            }
            let selection = if popup {
                &mut state.popup.selection
            } else {
                &mut state.selection
            };
            let position = selection.position(row, column);
            match kind {
                "start" if shift => selection.extend(position),
                "start" if clicks == 2 => selection.word(position),
                "start" if clicks == 3 => selection.line(position),
                "start" => selection.start(position),
                "extend" => selection.extend(position),
                "word" => selection.word(position),
                "line" => selection.line(position),
                "all" => selection.all(),
                "clear" => selection.clear(),
                key => selection.key(key),
            }
        }
        self.refresh_selection(popup);
    }

    pub(super) fn refresh_selection(&self, popup: bool) {
        let (selected, focus, changes, model) = {
            let state = self.state.borrow();
            let (selection, model) = if popup {
                (&state.popup.selection, state.popup.output.clone())
            } else {
                (&state.selection, state.output.clone())
            };
            (
                selection.range().is_some(),
                selection.focus_row(),
                row_changes(selection, &model),
                model,
            )
        };
        for (index, row) in changes {
            model.set_row_data(index, row);
        }
        if popup {
            for window in [
                self.modes.compact.global::<OutputBridge>(),
                self.modes.popup.global::<OutputBridge>(),
            ] {
                window.set_popup_selected(selected);
                window.set_popup_focus(focus);
            }
        } else {
            let ui = self.ui();
            let bridge = ui.global::<OutputBridge>();
            bridge.set_full_selected(selected);
            bridge.set_full_focus(focus);
        }
    }
}

fn row_changes(selection: &Selection, model: &VecModel<OutputRow>) -> Vec<(usize, OutputRow)> {
    model
        .iter()
        .enumerate()
        .filter_map(|(index, mut row)| {
            let (from, to) = selection.columns(selection.first + index as u64);
            if (row.selected_from, row.selected_to) == (from, to) {
                return None;
            }
            row.selected_from = from;
            row.selected_to = to;
            Some((index, row))
        })
        .collect()
}
