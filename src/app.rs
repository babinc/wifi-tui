use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::event::{EventLoop, Task};
use crate::nmcli::{ConnectionStatus, Network, SavedNetwork};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    AvailableNetworks,
    SavedNetworks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    PasswordInput,
    ConfirmDisconnect,
    ConfirmForget(String), // network name
    Message(String),       // message text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgStatus {
    Idle,
    Scanning,
    Connecting,
    Disconnecting,
    Forgetting,
}

pub struct App {
    pub running: bool,
    pub view: View,
    pub modal: Option<Modal>,
    pub bg_status: BgStatus,

    // Network data
    pub networks: Vec<Network>,
    pub saved: Vec<SavedNetwork>,
    pub status: ConnectionStatus,
    pub device: String,

    // List selection
    pub net_index: usize,
    pub saved_index: usize,

    // Password input
    pub password: String,
    pub password_visible: bool,
    pub password_target_ssid: String,

    // Auto-refresh
    pub ticks_since_scan: u32,
    pub spinner_frame: usize,
}

const AUTO_REFRESH_TICKS: u32 = 120; // 30s at 250ms tick rate

impl App {
    pub fn new(device: String) -> Self {
        Self {
            running: true,
            view: View::AvailableNetworks,
            modal: None,
            bg_status: BgStatus::Idle,

            networks: Vec::new(),
            saved: Vec::new(),
            status: ConnectionStatus {
                ssid: None,
                signal: None,
                ip: None,
                speed: None,
            },
            device,

            net_index: 0,
            saved_index: 0,

            password: String::new(),
            password_visible: false,
            password_target_ssid: String::new(),

            ticks_since_scan: AUTO_REFRESH_TICKS, // trigger immediate scan
            spinner_frame: 0,
        }
    }

    /// Handle a keyboard event. Returns true if the event was consumed.
    pub fn handle_key(&mut self, key: KeyEvent, events: &EventLoop) {
        // Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        // Modal gets priority
        if let Some(ref modal) = self.modal.clone() {
            self.handle_modal_key(key, modal, events);
            return;
        }

        // Global keys
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.running = false;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.view = match self.view {
                    View::AvailableNetworks => View::SavedNetworks,
                    View::SavedNetworks => View::AvailableNetworks,
                };
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if self.bg_status == BgStatus::Idle {
                    self.start_scan(events);
                }
            }
            _ => match self.view {
                View::AvailableNetworks => self.handle_available_key(key, events),
                View::SavedNetworks => self.handle_saved_key(key, events),
            },
        }
    }

    fn handle_available_key(&mut self, key: KeyEvent, events: &EventLoop) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.net_index > 0 {
                    self.net_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.networks.is_empty() && self.net_index < self.networks.len() - 1 {
                    self.net_index += 1;
                }
            }
            KeyCode::Enter => {
                if self.bg_status != BgStatus::Idle {
                    return;
                }
                if let Some(net) = self.networks.get(self.net_index) {
                    if net.in_use {
                        self.modal = Some(Modal::Message("Already connected to this network.".to_string()));
                        return;
                    }
                    let ssid = net.ssid.clone();
                    // Always try connecting first - nmcli will use saved
                    // credentials if available. If it needs a password,
                    // the result handler will show the password modal.
                    self.bg_status = BgStatus::Connecting;
                    events.send_task(Task::Connect(ssid, Some(String::new())));
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.bg_status == BgStatus::Idle && self.status.ssid.is_some() {
                    self.modal = Some(Modal::ConfirmDisconnect);
                }
            }
            _ => {}
        }
    }

    fn handle_saved_key(&mut self, key: KeyEvent, events: &EventLoop) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.saved_index > 0 {
                    self.saved_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.saved.is_empty() && self.saved_index < self.saved.len() - 1 {
                    self.saved_index += 1;
                }
            }
            KeyCode::Enter => {
                if self.bg_status != BgStatus::Idle {
                    return;
                }
                if let Some(saved) = self.saved.get(self.saved_index) {
                    if saved.active {
                        self.modal = Some(Modal::Message("Already connected to this network.".to_string()));
                        return;
                    }
                    let name = saved.name.clone();
                    self.bg_status = BgStatus::Connecting;
                    events.send_task(Task::Connect(name, None));
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                if self.bg_status != BgStatus::Idle {
                    return;
                }
                if let Some(saved) = self.saved.get(self.saved_index) {
                    let name = saved.name.clone();
                    self.modal = Some(Modal::ConfirmForget(name));
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.bg_status == BgStatus::Idle && self.status.ssid.is_some() {
                    self.modal = Some(Modal::ConfirmDisconnect);
                }
            }
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent, modal: &Modal, events: &EventLoop) {
        match modal {
            Modal::PasswordInput => match key.code {
                KeyCode::Esc => {
                    self.modal = None;
                    self.password.clear();
                }
                KeyCode::Enter => {
                    let ssid = self.password_target_ssid.clone();
                    let pw = self.password.clone();
                    self.modal = None;
                    self.bg_status = BgStatus::Connecting;
                    events.send_task(Task::Connect(ssid, Some(pw)));
                }
                KeyCode::Backspace => {
                    self.password.pop();
                }
                KeyCode::Tab => {
                    self.password_visible = !self.password_visible;
                }
                KeyCode::Char(c) => {
                    self.password.push(c);
                }
                _ => {}
            },
            Modal::ConfirmDisconnect => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.modal = None;
                    self.bg_status = BgStatus::Disconnecting;
                    events.send_task(Task::Disconnect(self.device.clone()));
                }
                _ => {
                    self.modal = None;
                }
            },
            Modal::ConfirmForget(name) => {
                let name = name.clone();
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.modal = None;
                        self.bg_status = BgStatus::Forgetting;
                        events.send_task(Task::Forget(name));
                    }
                    _ => {
                        self.modal = None;
                    }
                }
            }
            Modal::Message(_) => {
                // Any key dismisses
                self.modal = None;
            }
        }
    }

    /// Handle a tick event - auto-refresh, spinner.
    pub fn handle_tick(&mut self, events: &EventLoop) {
        self.spinner_frame = (self.spinner_frame + 1) % 4;

        self.ticks_since_scan += 1;
        if self.ticks_since_scan >= AUTO_REFRESH_TICKS && self.bg_status == BgStatus::Idle {
            self.start_scan(events);
        }
    }

    /// Start a scan + status refresh.
    fn start_scan(&mut self, events: &EventLoop) {
        self.bg_status = BgStatus::Scanning;
        self.ticks_since_scan = 0;
        events.send_task(Task::Scan(self.device.clone()));
        events.send_task(Task::RefreshStatus(self.device.clone()));
        events.send_task(Task::RefreshSaved);
    }

    /// Handle a completed background task.
    pub fn handle_task_result(&mut self, result: crate::event::TaskResult) {
        use crate::event::TaskResult;

        match result {
            TaskResult::ScanComplete(Ok(networks)) => {
                self.networks = networks;
                if self.net_index >= self.networks.len() && !self.networks.is_empty() {
                    self.net_index = self.networks.len() - 1;
                }
                self.bg_status = BgStatus::Idle;
            }
            TaskResult::ScanComplete(Err(e)) => {
                self.bg_status = BgStatus::Idle;
                self.modal = Some(Modal::Message(e));
            }
            TaskResult::ConnectComplete(Ok(msg), _ssid) => {
                self.bg_status = BgStatus::Idle;
                self.modal = Some(Modal::Message(msg));
                self.ticks_since_scan = AUTO_REFRESH_TICKS; // trigger refresh
            }
            TaskResult::ConnectComplete(Err(e), ssid) => {
                self.bg_status = BgStatus::Idle;
                if crate::nmcli::error_needs_password(&e) {
                    // Password needed - show password prompt instead of error
                    self.password.clear();
                    self.password_visible = false;
                    self.password_target_ssid = ssid;
                    self.modal = Some(Modal::PasswordInput);
                } else {
                    self.modal = Some(Modal::Message(e));
                }
            }
            TaskResult::DisconnectComplete(Ok(msg)) => {
                self.bg_status = BgStatus::Idle;
                self.modal = Some(Modal::Message(msg));
                self.ticks_since_scan = AUTO_REFRESH_TICKS;
            }
            TaskResult::DisconnectComplete(Err(e)) => {
                self.bg_status = BgStatus::Idle;
                self.modal = Some(Modal::Message(e));
            }
            TaskResult::ForgetComplete(Ok(msg)) => {
                self.bg_status = BgStatus::Idle;
                self.modal = Some(Modal::Message(msg));
                self.ticks_since_scan = AUTO_REFRESH_TICKS;
            }
            TaskResult::ForgetComplete(Err(e)) => {
                self.bg_status = BgStatus::Idle;
                self.modal = Some(Modal::Message(e));
            }
            TaskResult::StatusUpdate(status) => {
                self.status = status;
                // Don't change bg_status here - scan result will do that
            }
            TaskResult::SavedUpdate(Ok(saved)) => {
                self.saved = saved;
                if self.saved_index >= self.saved.len() && !self.saved.is_empty() {
                    self.saved_index = self.saved.len() - 1;
                }
            }
            TaskResult::SavedUpdate(Err(_)) => {
                // Silently ignore - saved list will just be stale
            }
        }
    }
}
