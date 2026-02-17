use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Network {
    pub ssid: String,
    pub signal: u8,
    pub security: String,
    pub in_use: bool,
}

#[derive(Debug, Clone)]
pub struct SavedNetwork {
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectionStatus {
    pub ssid: Option<String>,
    pub signal: Option<u8>,
    pub ip: Option<String>,
    pub speed: Option<String>,
}

/// Detect the WiFi device name (e.g. wlp3s0, wlan0).
pub fn detect_wifi_device() -> Result<String, String> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE", "device"])
        .output()
        .map_err(|e| friendly_error(&e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let fields = parse_terse_line(line);
        if fields.len() >= 2 && fields[1] == "wifi" {
            return Ok(fields[0].clone());
        }
    }
    Err("No WiFi adapter found. Make sure your WiFi hardware is enabled.".to_string())
}

/// Scan for available networks. Returns deduplicated list sorted by signal strength.
pub fn scan_networks(device: &str) -> Result<Vec<Network>, String> {
    // Trigger a rescan first (best-effort, ignore errors)
    let _ = Command::new("nmcli")
        .args(["device", "wifi", "rescan", "ifname", device])
        .output();

    let output = Command::new("nmcli")
        .args([
            "-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "ifname", device,
        ])
        .output()
        .map_err(|e| friendly_error(&e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(friendly_error(stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut best: HashMap<String, Network> = HashMap::new();

    for line in stdout.lines() {
        let fields = parse_terse_line(line);
        if fields.len() < 4 {
            continue;
        }

        let ssid = fields[1].clone();
        if ssid.is_empty() {
            continue;
        }

        let signal: u8 = fields[2].parse().unwrap_or(0);
        let security = fields[3].clone();
        let in_use = fields[0].trim() == "*";

        let net = Network {
            ssid: ssid.clone(),
            signal,
            security,
            in_use,
        };

        // Keep the entry with highest signal, but always prefer the in_use one
        if let Some(existing) = best.get(&ssid) {
            if in_use || (!existing.in_use && signal > existing.signal) {
                best.insert(ssid, net);
            }
        } else {
            best.insert(ssid, net);
        }
    }

    let mut networks: Vec<Network> = best.into_values().collect();
    // Sort: in_use first, then by signal descending
    networks.sort_by(|a, b| {
        b.in_use
            .cmp(&a.in_use)
            .then(b.signal.cmp(&a.signal))
    });

    Ok(networks)
}

/// Get the current connection status.
pub fn get_status(device: &str) -> ConnectionStatus {
    let mut status = ConnectionStatus {
        ssid: None,
        signal: None,
        ip: None,
        speed: None,
    };

    // Get SSID + signal from the in-use wifi entry (gives actual broadcast SSID,
    // not the NM profile name which GENERAL.CONNECTION returns)
    if let Ok(output) = Command::new("nmcli")
        .args([
            "-t", "-f", "IN-USE,SSID,SIGNAL",
            "device", "wifi", "list", "ifname", device,
        ])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let fields = parse_terse_line(line);
            if fields.len() >= 3 && fields[0].trim() == "*" && !fields[1].is_empty() {
                status.ssid = Some(fields[1].clone());
                status.signal = fields[2].parse().ok();
                break;
            }
        }
    }

    // If connected, get IP and speed
    if status.ssid.is_some() {
        // Get IP address
        if let Ok(output) = Command::new("nmcli")
            .args(["-t", "-f", "IP4.ADDRESS", "device", "show", device])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let fields = parse_terse_line(line);
                if fields.len() >= 2 && fields[0].starts_with("IP4.ADDRESS") {
                    let ip = fields[1].split('/').next().unwrap_or(&fields[1]);
                    status.ip = Some(ip.to_string());
                    break;
                }
            }
        }

        // Get link speed via iw
        if let Ok(output) = Command::new("iw")
            .args(["dev", device, "link"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("tx bitrate:") {
                    let rate = trimmed
                        .trim_start_matches("tx bitrate:")
                        .trim()
                        .split_whitespace()
                        .take(2)
                        .collect::<Vec<&str>>()
                        .join(" ");
                    status.speed = Some(rate);
                    break;
                }
            }
        }
    }

    status
}

/// List saved (known) WiFi connections.
pub fn saved_networks() -> Result<Vec<SavedNetwork>, String> {
    let output = Command::new("nmcli")
        .args([
            "-t",
            "-f",
            "NAME,TYPE,ACTIVE",
            "connection",
            "show",
        ])
        .output()
        .map_err(|e| friendly_error(&e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(friendly_error(stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut networks = Vec::new();

    for line in stdout.lines() {
        let fields = parse_terse_line(line);
        if fields.len() >= 3 && fields[1].contains("wireless") {
            networks.push(SavedNetwork {
                name: fields[0].clone(),
                active: fields[2] == "yes",
            });
        }
    }

    Ok(networks)
}

/// Connect to a network. If password is Some, use `device wifi connect` for new connections.
/// If None, use `connection up` to reconnect to a saved network.
pub fn connect(ssid: &str, password: Option<&str>) -> Result<String, String> {
    let output = match password {
        Some(pw) if !pw.is_empty() => {
            Command::new("nmcli")
                .args(["device", "wifi", "connect", ssid, "password", pw])
                .output()
                .map_err(|e| friendly_error(&e.to_string()))?
        }
        Some(_) => {
            // Open network (no password)
            Command::new("nmcli")
                .args(["device", "wifi", "connect", ssid])
                .output()
                .map_err(|e| friendly_error(&e.to_string()))?
        }
        None => {
            // Saved network - reconnect
            Command::new("nmcli")
                .args(["connection", "up", ssid])
                .output()
                .map_err(|e| friendly_error(&e.to_string()))?
        }
    };

    if output.status.success() {
        Ok(format!("Connected to {}", ssid))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(friendly_error(stderr.trim()))
    }
}

/// Disconnect from the current network.
pub fn disconnect(device: &str) -> Result<String, String> {
    let output = Command::new("nmcli")
        .args(["device", "disconnect", device])
        .output()
        .map_err(|e| friendly_error(&e.to_string()))?;

    if output.status.success() {
        Ok("Disconnected.".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(friendly_error(stderr.trim()))
    }
}

/// Forget (delete) a saved network connection.
pub fn forget(name: &str) -> Result<String, String> {
    let output = Command::new("nmcli")
        .args(["connection", "delete", name])
        .output()
        .map_err(|e| friendly_error(&e.to_string()))?;

    if output.status.success() {
        Ok(format!("Forgot network '{}'.", name))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(friendly_error(stderr.trim()))
    }
}

/// Check if an error message indicates that a password is needed to connect.
pub fn error_needs_password(msg: &str) -> bool {
    msg.contains("Password required") || msg.contains("Incorrect password")
}

/// Parse nmcli terse output line, handling `\:` escaped colons within fields.
fn parse_terse_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next == ':' {
                    current.push(':');
                    chars.next();
                    continue;
                }
            }
            current.push(ch);
        } else if ch == ':' {
            fields.push(current.clone());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    fields.push(current);

    fields
}

/// Translate nmcli error messages into beginner-friendly text.
fn friendly_error(msg: &str) -> String {
    if msg.contains("No network with SSID") {
        "Network not found. It may be out of range or hidden.".to_string()
    } else if msg.contains("Secrets were required, but not provided") {
        "Password required. This network needs a password to connect.".to_string()
    } else if msg.contains("No suitable device found") {
        "No WiFi adapter found. Make sure your WiFi hardware is enabled.".to_string()
    } else if msg.contains("is not running") {
        "NetworkManager is not running. Start it with: sudo systemctl start NetworkManager"
            .to_string()
    } else if msg.contains("Error: Connection") && msg.contains("not found") {
        "Saved connection not found. It may have already been removed.".to_string()
    } else if msg.contains("Passwords or encryption keys are required") {
        "Incorrect password. Please try again.".to_string()
    } else if msg.contains("permission") || msg.contains("not authorized") {
        "Permission denied. You may need to run with appropriate privileges.".to_string()
    } else if msg.is_empty() {
        "An unknown error occurred.".to_string()
    } else {
        msg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_terse_line_basic() {
        let fields = parse_terse_line("*:MyWifi:85:WPA2");
        assert_eq!(fields, vec!["*", "MyWifi", "85", "WPA2"]);
    }

    #[test]
    fn test_parse_terse_line_escaped_colon() {
        let fields = parse_terse_line(r"*:My\:Wifi:85:WPA2");
        assert_eq!(fields, vec!["*", "My:Wifi", "85", "WPA2"]);
    }

    #[test]
    fn test_parse_terse_line_empty_field() {
        let fields = parse_terse_line("*::85:WPA2");
        assert_eq!(fields, vec!["*", "", "85", "WPA2"]);
    }
}
