//! Agent support in the window: confirmation dialogs, the connected-agents indicator, and
//! snackbars with Undo for agent changes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use slint::SharedString;

use super::{Controller, with_controller};
use crate::ipc::host::{AgentUi, ConfirmRequest, Decision, Undo};

/// Unanswered confirmations are denied after this long (specification §15.2).
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

/// A confirmation waiting for an answer.
pub struct PendingConfirmation {
    id: u64,
    request: ConfirmRequest,
    answer: mpsc::Sender<Decision>,
}

/// [`AgentUi`] for the window: every call is forwarded to the UI thread.
pub struct UiAgent;

impl AgentUi for UiAgent {
    fn confirm(&self, request: ConfirmRequest) -> Decision {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        let pending = PendingConfirmation {
            id,
            request,
            answer: tx,
        };
        if slint::invoke_from_event_loop(move || {
            with_controller(|c| c.enqueue_confirmation(pending))
        })
        .is_err()
        {
            return Decision::Deny;
        }
        match rx.recv_timeout(CONFIRM_TIMEOUT) {
            Ok(decision) => decision,
            Err(_) => {
                let _ = slint::invoke_from_event_loop(move || {
                    with_controller(|c| c.drop_confirmation(id))
                });
                Decision::Deny
            }
        }
    }

    fn agents_changed(&self, agents: Vec<String>) {
        let _ = slint::invoke_from_event_loop(move || {
            with_controller(|c| c.ui().set_agents(agents.join(", ").into()))
        });
    }

    fn changed(&self, _agent: &str, description: String, undo: Undo) {
        let _ = slint::invoke_from_event_loop(move || {
            with_controller(|c| c.agent_changed(description, undo))
        });
    }
}

/// [`AgentUi`] for `--headless` test instances: nobody can confirm, so confirmations are
/// denied; changes are only logged.
pub struct HeadlessAgent;

impl AgentUi for HeadlessAgent {
    fn confirm(&self, request: ConfirmRequest) -> Decision {
        log::info!(
            "headless: denying \"{}\" from {}",
            request.operation,
            request.agent
        );
        Decision::Deny
    }

    fn agents_changed(&self, agents: Vec<String>) {
        log::info!("headless: connected agents: {agents:?}");
    }

    fn changed(&self, _agent: &str, description: String, _undo: Undo) {
        log::info!("headless: {description}");
    }
}

impl Controller {
    pub(super) fn enqueue_confirmation(&self, pending: PendingConfirmation) {
        let first = {
            let mut st = self.state.borrow_mut();
            st.confirmations.push_back(pending);
            st.confirmations.len() == 1
        };
        if first {
            self.show_confirmation();
        }
    }

    fn show_confirmation(&self) {
        let ui = self.ui();
        let st = self.state.borrow();
        match st.confirmations.front() {
            Some(p) => {
                ui.set_agent_ask_agent(p.request.agent.clone().into());
                ui.set_agent_ask_operation(p.request.operation.clone().into());
                ui.set_agent_ask_single(p.request.single_only);
                ui.set_agent_ask_visible(true);
                drop(st);
                self.show_window();
            }
            None => ui.set_agent_ask_visible(false),
        }
    }

    pub(super) fn drop_confirmation(&self, id: u64) {
        let was_first = {
            let mut st = self.state.borrow_mut();
            let first = st.confirmations.front().is_some_and(|p| p.id == id);
            st.confirmations.retain(|p| p.id != id);
            first
        };
        if was_first {
            self.show_confirmation();
        }
    }

    pub(super) fn answer_confirmation(&self, answer: &str) {
        let decision = match answer {
            "allow" => Decision::Allow,
            "all" => Decision::AllowAll,
            _ => Decision::Deny,
        };
        let pending = self.state.borrow_mut().confirmations.pop_front();
        if let Some(p) = pending {
            let _ = p.answer.send(decision);
        }
        self.show_confirmation();
    }

    pub(super) fn agent_changed(&self, description: String, undo: Undo) {
        self.after_agent_change();
        self.snackbar_for(description, Duration::from_secs(8));
        self.state.borrow_mut().undo = Some(undo);
        self.ui().set_snackbar_undo(true);
    }

    /// Reloads what an agent may have changed: apps, settings, and the theme.
    fn after_agent_change(&self) {
        let settings = self.settings.read().unwrap().clone();
        self.manager.set_line_limit(settings.output_line_limit);
        self.apply_theme();
        self.reload_apps();
    }

    pub(super) fn undo(&self) {
        let undo = self.state.borrow_mut().undo.take();
        let ui = self.ui();
        ui.set_snackbar_undo(false);
        ui.set_snackbar(SharedString::new());
        if let Some(undo) = undo {
            let result = self.host.undo(undo);
            self.after_agent_change();
            match result {
                Ok(()) => self.snackbar("Change undone.".into()),
                Err(e) => self.snackbar(format!("Undo failed: {e:#}")),
            }
        }
    }
}
