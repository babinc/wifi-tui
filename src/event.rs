use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event as CEvent, KeyEvent};

use crate::nmcli;

/// Events that the main loop receives.
pub enum Event {
    /// A keyboard event.
    Key(KeyEvent),
    /// A periodic tick (for auto-refresh, spinner animation).
    Tick,
    /// A background task completed.
    TaskResult(TaskResult),
}

/// Tasks sent to the background worker.
pub enum Task {
    Scan(String),                          // device
    Connect(String, Option<String>),       // ssid, password
    Disconnect(String),                    // device
    Forget(String),                        // network name
    RefreshStatus(String),                 // device
    RefreshSaved,
}

/// Results from background tasks.
pub enum TaskResult {
    ScanComplete(Result<Vec<nmcli::Network>, String>),
    /// (result, ssid) - ssid carried through for password retry
    ConnectComplete(Result<String, String>, String),
    DisconnectComplete(Result<String, String>),
    ForgetComplete(Result<String, String>),
    StatusUpdate(nmcli::ConnectionStatus),
    SavedUpdate(Result<Vec<nmcli::SavedNetwork>, String>),
}

pub struct EventLoop {
    rx: mpsc::Receiver<Event>,
    task_tx: mpsc::Sender<Task>,
}

impl EventLoop {
    /// Start the event loop with keyboard polling and a background worker.
    pub fn new(tick_rate: Duration) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let (task_tx, task_rx) = mpsc::channel::<Task>();

        // Keyboard + tick polling thread
        let tx = event_tx.clone();
        thread::spawn(move || {
            let mut last_tick = Instant::now();
            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::ZERO);

                if event::poll(timeout).unwrap_or(false) {
                    if let Ok(CEvent::Key(key)) = event::read() {
                        if tx.send(Event::Key(key)).is_err() {
                            return;
                        }
                    }
                }

                if last_tick.elapsed() >= tick_rate {
                    if tx.send(Event::Tick).is_err() {
                        return;
                    }
                    last_tick = Instant::now();
                }
            }
        });

        // Background worker thread - serializes all nmcli operations
        let tx = event_tx;
        thread::spawn(move || {
            for task in task_rx {
                let result = match task {
                    Task::Scan(device) => {
                        TaskResult::ScanComplete(nmcli::scan_networks(&device))
                    }
                    Task::Connect(ssid, password) => {
                        let result = nmcli::connect(&ssid, password.as_deref());
                        TaskResult::ConnectComplete(result, ssid)
                    }
                    Task::Disconnect(device) => {
                        TaskResult::DisconnectComplete(nmcli::disconnect(&device))
                    }
                    Task::Forget(name) => {
                        TaskResult::ForgetComplete(nmcli::forget(&name))
                    }
                    Task::RefreshStatus(device) => {
                        TaskResult::StatusUpdate(nmcli::get_status(&device))
                    }
                    Task::RefreshSaved => {
                        TaskResult::SavedUpdate(nmcli::saved_networks())
                    }
                };
                if tx.send(Event::TaskResult(result)).is_err() {
                    return;
                }
            }
        });

        Self {
            rx: event_rx,
            task_tx,
        }
    }

    /// Try to receive the next event (non-blocking).
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Send a task to the background worker.
    pub fn send_task(&self, task: Task) {
        let _ = self.task_tx.send(task);
    }
}
