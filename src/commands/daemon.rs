//! @acp:module "Daemon Command"
//! @acp:summary "Manage the ACP daemon (acpd)"
//! @acp:domain cli
//! @acp:layer handler
//!
//! Implements `acp daemon` command for daemon lifecycle management.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use console::style;

/// Daemon subcommands
#[derive(Debug, Clone)]
pub enum DaemonSubcommand {
    /// Start the daemon
    Start {
        /// Run in foreground mode
        foreground: bool,
        /// HTTP server port
        port: u16,
    },
    /// Stop the daemon
    Stop,
    /// Check daemon status
    Status,
    /// View daemon logs
    Logs {
        /// Number of lines to show
        lines: usize,
        /// Follow log output
        follow: bool,
    },
}

/// Execute daemon subcommands
pub fn execute_daemon(cmd: DaemonSubcommand) -> Result<()> {
    let acp_dir = PathBuf::from(".acp");
    let pid_file = acp_dir.join("daemon.pid");
    let log_file = acp_dir.join("daemon.log");

    match cmd {
        DaemonSubcommand::Start { foreground, port } => {
            // Check if already running
            if let Some(pid) = read_pid_file(&pid_file) {
                if is_process_running(pid) {
                    println!(
                        "{} Daemon already running with PID {}",
                        style("!").yellow(),
                        pid
                    );
                    return Ok(());
                }
                // Stale PID file
                let _ = std::fs::remove_file(&pid_file);
            }

            // Check if port is already in use
            if is_port_in_use(port) {
                eprintln!("{} Port {} is already in use", style("✗").red(), port);
                eprintln!("  Another process may be using this port.");
                eprintln!("  Try a different port with: acp daemon start --port <PORT>");
                return Err(anyhow::anyhow!("Port {} is already in use", port));
            }

            // Ensure .acp directory exists
            if !acp_dir.exists() {
                std::fs::create_dir_all(&acp_dir)?;
            }

            // Find the acpd binary
            let acpd_path = find_acpd_binary()?;

            if foreground {
                // Run in foreground - exec the daemon
                println!(
                    "{} Starting daemon in foreground mode...",
                    style("→").cyan()
                );
                let status = Command::new(&acpd_path)
                    .arg("--port")
                    .arg(port.to_string())
                    .arg("run")
                    .status()?;

                if !status.success() {
                    eprintln!("{} Daemon exited with error", style("✗").red());
                    std::process::exit(1);
                }
            } else {
                // Start in background
                let log = std::fs::File::create(&log_file)?;
                let log_err = log.try_clone()?;

                let child = Command::new(&acpd_path)
                    .arg("--port")
                    .arg(port.to_string())
                    .arg("run")
                    .stdout(log)
                    .stderr(log_err)
                    .spawn()?;

                let pid = child.id();
                // Store pid:port for status command to use
                std::fs::write(&pid_file, format!("{}:{}", pid, port))?;

                println!(
                    "{} Daemon started with PID {} (port {})",
                    style("✓").green(),
                    pid,
                    port
                );
                println!("  Log file: {}", log_file.display());
                println!("  API: http://127.0.0.1:{}/health", port);
            }
        }

        DaemonSubcommand::Stop => match read_pid_file(&pid_file) {
            Some(pid) => {
                if is_process_running(pid) {
                    // Send SIGTERM
                    #[cfg(unix)]
                    {
                        let _ = Command::new("kill")
                            .arg("-TERM")
                            .arg(pid.to_string())
                            .status();
                    }

                    #[cfg(not(unix))]
                    {
                        eprintln!(
                            "{} Stopping daemon not supported on this platform",
                            style("✗").red()
                        );
                    }

                    println!(
                        "{} Sent stop signal to daemon (PID {})",
                        style("✓").green(),
                        pid
                    );
                } else {
                    println!(
                        "{} Daemon not running (stale PID file)",
                        style("!").yellow()
                    );
                }
                let _ = std::fs::remove_file(&pid_file);
            }
            None => {
                println!("{} No daemon running", style("•").dim());
            }
        },

        DaemonSubcommand::Status => match read_pid_file(&pid_file) {
            Some(pid) => {
                if is_process_running(pid) {
                    // Read port from PID file, default to 9222 for backwards compat
                    let port = read_port_from_pid_file(&pid_file).unwrap_or(9222);
                    println!(
                        "{} Daemon is running (PID {}, port {})",
                        style("✓").green(),
                        pid,
                        port
                    );

                    // Try to check health endpoint
                    if let Ok(health) = check_daemon_health(port) {
                        println!("  Health: {}", health);
                    }
                } else {
                    println!(
                        "{} Daemon not running (stale PID file)",
                        style("!").yellow()
                    );
                    let _ = std::fs::remove_file(&pid_file);
                }
            }
            None => {
                println!("{} Daemon not running", style("•").dim());
            }
        },

        DaemonSubcommand::Logs { lines, follow } => {
            if !log_file.exists() {
                println!(
                    "{} No log file found at {}",
                    style("!").yellow(),
                    log_file.display()
                );
                return Ok(());
            }

            if follow {
                // Use tail -f
                let mut child = Command::new("tail")
                    .arg("-f")
                    .arg("-n")
                    .arg(lines.to_string())
                    .arg(&log_file)
                    .spawn()?;

                child.wait()?;
            } else {
                // Read last N lines
                let file = std::fs::File::open(&log_file)?;
                let reader = BufReader::new(file);
                let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
                let start = if all_lines.len() > lines {
                    all_lines.len() - lines
                } else {
                    0
                };

                for line in &all_lines[start..] {
                    println!("{}", line);
                }
            }
        }
    }

    Ok(())
}

/// Read PID file content. Format: "pid" or "pid:port"
fn read_pid_file(path: &PathBuf) -> Option<u32> {
    std::fs::read_to_string(path).ok().and_then(|s| {
        let content = s.trim();
        // Support format "pid" or "pid:port"
        content
            .split(':')
            .next()
            .and_then(|pid_str| pid_str.parse().ok())
    })
}

/// Read port from PID file if stored. Format: "pid:port"
fn read_port_from_pid_file(path: &PathBuf) -> Option<u16> {
    std::fs::read_to_string(path).ok().and_then(|s| {
        let parts: Vec<&str> = s.trim().split(':').collect();
        if parts.len() >= 2 {
            parts[1].parse().ok()
        } else {
            None
        }
    })
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true // Assume running on non-Unix
    }
}

fn is_port_in_use(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_err()
}

fn find_acpd_binary() -> Result<PathBuf> {
    // First check if acpd is in PATH
    if let Ok(output) = Command::new("which").arg("acpd").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    // Check common locations relative to current binary
    let current_exe = std::env::current_exe()?;
    if let Some(bin_dir) = current_exe.parent() {
        let acpd_path = bin_dir.join("acpd");
        if acpd_path.exists() {
            return Ok(acpd_path);
        }
    }

    // Check target/debug and target/release
    for dir in &["target/debug/acpd", "target/release/acpd"] {
        let path = PathBuf::from(dir);
        if path.exists() {
            return Ok(path);
        }
    }

    Err(anyhow::anyhow!(
        "Could not find acpd binary. Make sure it's installed or built.\n\
         Try: cargo build -p acpd"
    ))
}

fn check_daemon_health(port: u16) -> std::result::Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("curl")
        .arg("-s")
        .arg("-m")
        .arg("2") // 2 second timeout
        .arg(format!("http://127.0.0.1:{}/health", port))
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err("Failed to connect".into())
    }
}
