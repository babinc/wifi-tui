mod app;
mod event;
mod nmcli;
mod ui;

use std::time::Duration;

use color_eyre::eyre::{Result, WrapErr};
use ratatui::crossterm::event::KeyEventKind;
use ratatui::DefaultTerminal;

fn main() -> Result<()> {
    color_eyre::install()?;

    // Detect WiFi device before entering TUI
    let device = match nmcli::detect_wifi_device() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Install a panic hook that restores the terminal before printing the error.
    // Without this, a panic leaves the terminal in raw/alternate-screen mode.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));

    let terminal = ratatui::init();
    let result = run(terminal, device);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal, device: String) -> Result<()> {
    let mut app = app::App::new(device);
    let events = event::EventLoop::new(Duration::from_millis(250));

    loop {
        // Draw
        terminal
            .draw(|frame| ui::draw(frame, &app))
            .wrap_err("failed to draw")?;

        // Process all pending events
        while let Some(ev) = events.try_recv() {
            match ev {
                event::Event::Key(key) => {
                    // Only handle key press events (ignore release/repeat on some platforms)
                    if key.kind == KeyEventKind::Press {
                        app.handle_key(key, &events);
                    }
                }
                event::Event::Tick => {
                    app.handle_tick(&events);
                }
                event::Event::TaskResult(result) => {
                    app.handle_task_result(result);
                }
            }
        }

        if !app.running {
            return Ok(());
        }

        // Small sleep to avoid busy-waiting when no events
        std::thread::sleep(Duration::from_millis(16));
    }
}
