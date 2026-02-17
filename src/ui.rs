use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Padding, Paragraph, Tabs, Wrap,
};
use ratatui::Frame;

use crate::app::{App, BgStatus, Modal, View};

const SPINNER: &[&str] = &["◐", "◓", "◑", "◒"];
const SSID_WIDTH: usize = 28;

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(2),  // status bar
        Constraint::Min(6),    // main content
        Constraint::Length(3), // help bar
    ])
    .split(frame.area());

    draw_status_bar(frame, app, chunks[0]);
    draw_main(frame, app, chunks[1]);
    draw_help_bar(frame, app, chunks[2]);

    // Draw modal overlay on top if active
    if let Some(ref modal) = app.modal {
        draw_modal(frame, app, modal);
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let line = if app.status.ssid.is_some() {
        build_status_line(app)
    } else {
        let mut spans = vec![Span::styled(
            " Not connected",
            Style::default().fg(Color::DarkGray),
        )];
        if let Some(bg_text) = bg_status_text(app) {
            spans.push(Span::raw("  │  "));
            spans.push(Span::styled(bg_text, Style::default().fg(Color::Yellow)));
        }
        Line::from(spans)
    };

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, chunks[0]);
}

fn build_status_line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();

    if let Some(ref ssid) = app.status.ssid {
        spans.push(Span::styled(
            format!(" Connected: {}", ssid),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(signal) = app.status.signal {
        spans.push(Span::raw("  │  "));
        let color = signal_color(signal);
        spans.push(Span::styled(
            format!("Signal: {} {}%", signal_bars(signal), signal),
            Style::default().fg(color),
        ));
    }

    if let Some(ref ip) = app.status.ip {
        spans.push(Span::raw("  │  "));
        spans.push(Span::styled(
            format!("IP: {}", ip),
            Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(ref speed) = app.status.speed {
        spans.push(Span::raw("  │  "));
        spans.push(Span::styled(
            format!("Speed: {}", speed),
            Style::default().fg(Color::Cyan),
        ));
    }

    if let Some(bg_text) = bg_status_text(app) {
        spans.push(Span::raw("  │  "));
        spans.push(Span::styled(bg_text, Style::default().fg(Color::Yellow)));
    }

    Line::from(spans)
}

fn bg_status_text(app: &App) -> Option<String> {
    match app.bg_status {
        BgStatus::Idle => None,
        BgStatus::Scanning => Some(format!("{} Scanning...", SPINNER[app.spinner_frame])),
        BgStatus::Connecting => Some(format!("{} Connecting...", SPINNER[app.spinner_frame])),
        BgStatus::Disconnecting => Some(format!("{} Disconnecting...", SPINNER[app.spinner_frame])),
        BgStatus::Forgetting => Some(format!("{} Forgetting...", SPINNER[app.spinner_frame])),
    }
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    let tab_labels = vec![
        format!(" Available ({}) ", app.networks.len()),
        format!(" Saved ({}) ", app.saved.len()),
    ];
    let selected = match app.view {
        View::AvailableNetworks => 0,
        View::SavedNetworks => 1,
    };

    let tabs = Tabs::new(tab_labels)
        .select(selected)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider("│");

    let block = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);

    let tab_chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);

    frame.render_widget(block, area);
    frame.render_widget(tabs, tab_chunks[0]);

    match app.view {
        View::AvailableNetworks => draw_available_networks(frame, app, tab_chunks[1]),
        View::SavedNetworks => draw_saved_networks(frame, app, tab_chunks[1]),
    }
}

fn draw_available_networks(frame: &mut Frame, app: &App, area: Rect) {
    if app.networks.is_empty() {
        let text = if app.bg_status == BgStatus::Scanning {
            "Scanning for networks..."
        } else {
            "No networks found. Press R to scan."
        };
        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .networks
        .iter()
        .enumerate()
        .map(|(i, net)| {
            let selected = i == app.net_index;
            let marker = if net.in_use { "● " } else { "  " };
            let color = signal_color(net.signal);
            let is_open = net.security.is_empty() || net.security == "--";

            let security_text = if is_open {
                "Open".to_string()
            } else {
                simplify_security(&net.security)
            };

            let line = Line::from(vec![
                Span::styled(
                    marker.to_string(),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    truncate_pad(&net.ssid, SSID_WIDTH),
                    if selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
                Span::styled(
                    format!(" {}  {:>3}%", signal_bars(net.signal), net.signal),
                    Style::default().fg(color),
                ),
                Span::styled(
                    format!("  {}", security_text),
                    if is_open {
                        Style::default().fg(Color::Yellow)
                    } else if selected {
                        Style::default().fg(Color::Gray)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
            ]);

            if selected {
                ListItem::new(line).style(Style::default().bg(Color::Indexed(236)))
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, area);
}

fn draw_saved_networks(frame: &mut Frame, app: &App, area: Rect) {
    if app.saved.is_empty() {
        let paragraph = Paragraph::new("No saved networks.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .saved
        .iter()
        .enumerate()
        .map(|(i, net)| {
            let selected = i == app.saved_index;
            let status_str = if net.active {
                "(connected)"
            } else {
                "(saved)"
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("  {}", truncate_pad(&net.name, SSID_WIDTH)),
                    if selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
                Span::styled(
                    format!(" {}", status_str),
                    if net.active {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(if selected { Color::Gray } else { Color::DarkGray })
                    },
                ),
            ]);

            if selected {
                ListItem::new(line).style(Style::default().bg(Color::Indexed(236)))
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, area);
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = if app.modal.is_some() {
        match &app.modal {
            Some(Modal::PasswordInput) => {
                help_line(&[("Enter", "Submit"), ("Esc", "Cancel"), ("Tab", "Show/Hide")])
            }
            Some(Modal::ConfirmDisconnect) | Some(Modal::ConfirmForget(_)) => {
                help_line(&[("Y", "Confirm"), ("N", "Cancel")])
            }
            Some(Modal::Message(_)) => help_line(&[("Any key", "Dismiss")]),
            None => unreachable!(),
        }
    } else {
        match app.view {
            View::AvailableNetworks => help_line(&[
                ("Tab", "Switch view"),
                ("Enter", "Connect"),
                ("D", "Disconnect"),
                ("R", "Refresh"),
                ("Q", "Quit"),
                ("↑↓", "Navigate"),
            ]),
            View::SavedNetworks => help_line(&[
                ("Tab", "Switch view"),
                ("Enter", "Reconnect"),
                ("F", "Forget"),
                ("D", "Disconnect"),
                ("R", "Refresh"),
                ("Q", "Quit"),
                ("↑↓", "Navigate"),
            ]),
        }
    };

    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}

/// Build a styled help line: keys are bright, descriptions are dim.
fn help_line(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(
            format!("[{}]", key),
            Style::default().fg(Color::Indexed(248)),
        ));
        spans.push(Span::styled(
            format!(" {}", desc),
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

fn draw_modal(frame: &mut Frame, app: &App, modal: &Modal) {
    let area = frame.area();
    let modal_width = 50u16.min(area.width.saturating_sub(4));
    let modal_height = match modal {
        Modal::PasswordInput => 7,
        Modal::ConfirmDisconnect | Modal::ConfirmForget(_) => 6,
        Modal::Message(_) => 6,
    };

    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    // Clear background
    frame.render_widget(Clear, modal_area);

    match modal {
        Modal::PasswordInput => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" Connect to {} ", app.password_target_ssid))
                .style(Style::default().fg(Color::Yellow));

            let inner = block.inner(modal_area);
            frame.render_widget(block, modal_area);

            let chunks =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
                    .split(inner);

            let label = Paragraph::new("Password:").style(Style::default().fg(Color::White));
            frame.render_widget(label, chunks[0]);

            let display_pw = if app.password_visible {
                app.password.clone()
            } else {
                "●".repeat(app.password.len())
            };

            let pw_line = Line::from(vec![
                Span::styled(
                    format!(" {} ", display_pw),
                    Style::default().fg(Color::White).bg(Color::DarkGray),
                ),
                Span::styled("█", Style::default().fg(Color::White)),
            ]);

            let pw_input = Paragraph::new(pw_line);
            frame.render_widget(pw_input, chunks[1]);

            let hint = help_line(&[("Tab", "show/hide"), ("Enter", "submit"), ("Esc", "cancel")]);
            let hint_p = Paragraph::new(hint).alignment(Alignment::Center);
            frame.render_widget(hint_p, chunks[2]);
        }
        Modal::ConfirmDisconnect => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Disconnect ")
                .style(Style::default().fg(Color::Yellow));

            let inner = block.inner(modal_area);
            frame.render_widget(block, modal_area);

            let ssid = app
                .status
                .ssid
                .as_deref()
                .unwrap_or("current network");

            let chunks =
                Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);

            let text = Paragraph::new(format!("Disconnect from {}?", ssid))
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Center);
            frame.render_widget(text, chunks[0]);

            let hint = help_line(&[("Y", "Yes"), ("N", "No")]);
            let hint_p = Paragraph::new(hint).alignment(Alignment::Center);
            frame.render_widget(hint_p, chunks[1]);
        }
        Modal::ConfirmForget(name) => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Forget Network ")
                .style(Style::default().fg(Color::Red));

            let inner = block.inner(modal_area);
            frame.render_widget(block, modal_area);

            let chunks =
                Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);

            let text = Paragraph::new(format!(
                "Forget '{}'?\nYou'll need the password to reconnect.",
                name
            ))
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center);
            frame.render_widget(text, chunks[0]);

            let hint = help_line(&[("Y", "Yes"), ("N", "No")]);
            let hint_p = Paragraph::new(hint).alignment(Alignment::Center);
            frame.render_widget(hint_p, chunks[1]);
        }
        Modal::Message(msg) => {
            let color = if msg.starts_with("Connected")
                || msg.starts_with("Disconnected")
                || msg.starts_with("Forgot")
            {
                Color::Green
            } else if msg.starts_with("Already") {
                Color::Yellow
            } else {
                Color::Red
            };

            let block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(color));

            let inner = block.inner(modal_area);
            frame.render_widget(block, modal_area);

            let chunks =
                Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).split(inner);

            let text = Paragraph::new(msg.clone())
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false });
            frame.render_widget(text, chunks[0]);

            let hint = Paragraph::new("[Any key] Dismiss")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(hint, chunks[1]);
        }
    }
}

/// Truncate a string to max_len chars with ellipsis, then pad to max_len.
fn truncate_pad(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        let truncated: String = chars[..max_len - 1].iter().collect();
        format!("{:<width$}", format!("{}…", truncated), width = max_len)
    }
}

/// Simplify verbose security strings from nmcli.
fn simplify_security(sec: &str) -> String {
    // nmcli can return things like "WPA2 WPA3" or "WPA1 WPA2"
    // Show the highest/most relevant standard
    if sec.contains("WPA3") && sec.contains("WPA2") {
        "WPA3".to_string()
    } else if sec.contains("802.1X") {
        "Enterprise".to_string()
    } else {
        sec.to_string()
    }
}

fn signal_bars(signal: u8) -> &'static str {
    match signal {
        80..=100 => "▂▄▆█",
        60..=79 => "▂▄▆ ",
        40..=59 => "▂▄  ",
        20..=39 => "▂   ",
        _ => "    ",
    }
}

fn signal_color(signal: u8) -> Color {
    match signal {
        80..=100 => Color::Green,
        50..=79 => Color::Yellow,
        _ => Color::Red,
    }
}
