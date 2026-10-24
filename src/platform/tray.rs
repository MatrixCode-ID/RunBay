//! The system tray icon with the aggregated badge, the agent connection dot, and its menu.

use anyhow::{Context, Result};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::data::Mode;

/// A command from the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    /// Left click: show the window of the active mode.
    Show,
    /// Switch to a mode.
    Mode(Mode),
    /// Toggle always on top for the active mode.
    AlwaysOnTop,
    /// Toggle load with Windows.
    Autostart,
    /// Check for updates, or restart to update when one is ready.
    Update,
    /// Exit RunBay.
    Exit,
}

const LOGO_PNG: &[u8] = include_bytes!("../../ui/logo/runbay-64.png");

/// The tray icon and its menu items.
pub struct Tray {
    icon: TrayIcon,
    base: image::RgbaImage,
    full: CheckMenuItem,
    compact: CheckMenuItem,
    floating: CheckMenuItem,
    on_top: CheckMenuItem,
    autostart: CheckMenuItem,
    update: MenuItem,
    last: std::cell::RefCell<(String, bool, bool)>,
}

impl Tray {
    /// Creates the tray icon; `on_command` is called on the UI thread.
    pub fn new(on_command: impl Fn(TrayCommand) + Send + Sync + 'static) -> Result<Self> {
        let base = image::load_from_memory(LOGO_PNG)
            .context("cannot decode the tray icon")?
            .to_rgba8();
        let full = CheckMenuItem::new("Full", true, false, None);
        let compact = CheckMenuItem::new("Compact", true, false, None);
        let floating = CheckMenuItem::new("Icon", true, false, None);
        let on_top = CheckMenuItem::new("Always on top", true, false, None);
        let autostart = CheckMenuItem::new("Load with Windows", true, false, None);
        let update = MenuItem::new("Check for updates", true, None);
        let exit = MenuItem::new("Exit", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &full,
            &compact,
            &floating,
            &PredefinedMenuItem::separator(),
            &on_top,
            &autostart,
            &update,
            &PredefinedMenuItem::separator(),
            &exit,
        ])?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("RunBay")
            .with_icon(render(&base, "", false, false)?)
            .build()
            .context("cannot create the tray icon")?;

        let ids: Vec<(MenuId, TrayCommand)> = vec![
            (full.id().clone(), TrayCommand::Mode(Mode::Full)),
            (compact.id().clone(), TrayCommand::Mode(Mode::Compact)),
            (floating.id().clone(), TrayCommand::Mode(Mode::Icon)),
            (on_top.id().clone(), TrayCommand::AlwaysOnTop),
            (autostart.id().clone(), TrayCommand::Autostart),
            (update.id().clone(), TrayCommand::Update),
            (exit.id().clone(), TrayCommand::Exit),
        ];
        let on_command = std::sync::Arc::new(on_command);
        let menu_command = on_command.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, command)) = ids.iter().find(|(id, _)| *id == event.id) {
                menu_command(*command);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                on_command(TrayCommand::Show);
            }
        }));
        Ok(Self {
            icon,
            base,
            full,
            compact,
            floating,
            on_top,
            autostart,
            update,
            last: std::cell::RefCell::new((String::new(), false, false)),
        })
    }

    /// Updates the badge (`running`, `failed`, `success`, or empty), the agent dot, and
    /// the tooltip. Active work uses blue; active Run steps use a green lamp.
    pub fn set_status(
        &self,
        status: &str,
        busy: bool,
        agents: &[String],
        update_ready: Option<&str>,
    ) {
        let connected = !agents.is_empty();
        let changed = {
            let mut last = self.last.borrow_mut();
            let changed = last.0 != status || last.1 != connected || last.2 != busy;
            *last = (status.to_string(), connected, busy);
            changed
        };
        if changed && let Ok(icon) = render(&self.base, status, busy, connected) {
            let _ = self.icon.set_icon(Some(icon));
        }
        let mut tooltip = "RunBay".to_string();
        if connected {
            tooltip.push_str(&format!("\nAgents: {}", agents.join(", ")));
        }
        if let Some(version) = update_ready {
            tooltip.push_str(&format!("\nUpdate ready: {version}"));
        }
        let _ = self.icon.set_tooltip(Some(tooltip));
        self.update.set_text(if update_ready.is_some() {
            "Restart to update"
        } else {
            "Check for updates"
        });
    }

    /// Updates the check marks.
    pub fn set_checks(&self, mode: Mode, on_top: bool, autostart: bool) {
        self.full.set_checked(mode == Mode::Full);
        self.compact.set_checked(mode == Mode::Compact);
        self.floating.set_checked(mode == Mode::Icon);
        self.on_top.set_checked(on_top);
        self.on_top.set_enabled(mode != Mode::Full);
        self.autostart.set_checked(autostart);
    }
}

/// Draws the badge (bottom right) and the agent dot (top right) on the logo.
fn render(base: &image::RgbaImage, status: &str, busy: bool, agent: bool) -> Result<Icon> {
    let mut img = base.clone();
    let badge = match status {
        "running" if busy => Some([0x1e, 0x88, 0xe5]),
        "running" => Some([0x2e, 0x7d, 0x32]),
        "failed" => Some([0xd3, 0x2f, 0x2f]),
        "success" => Some([0x2e, 0x7d, 0x32]),
        _ => None,
    };
    let size = img.width() as f32;
    if let Some(color) = badge {
        circle(&mut img, size * 0.76, size * 0.76, size * 0.24, color);
    }
    if agent {
        circle(
            &mut img,
            size * 0.82,
            size * 0.18,
            size * 0.16,
            [0x7d, 0xd9, 0x8f],
        );
    }
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).context("invalid tray icon")
}

/// A filled anti-aliased circle with a white ring.
fn circle(img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, color: [u8; 3]) {
    let ring = r;
    let inner = r - (r * 0.25).max(2.0);
    for y in 0..img.height() {
        for x in 0..img.width() {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            if d > ring + 0.5 {
                continue;
            }
            let alpha = (ring + 0.5 - d).clamp(0.0, 1.0);
            let fill = if d <= inner { color } else { [255, 255, 255] };
            let p = img.get_pixel_mut(x, y);
            for (channel, value) in p.0.iter_mut().zip(fill) {
                *channel = (value as f32 * alpha + *channel as f32 * (1.0 - alpha)) as u8;
            }
            p.0[3] = (255.0 * alpha + p.0[3] as f32 * (1.0 - alpha)) as u8;
        }
    }
}
