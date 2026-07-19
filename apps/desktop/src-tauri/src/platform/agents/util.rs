use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};

use serde_json::{json, Value};

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

pub(crate) fn app_data_dir() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local").join("share")))
}

pub(crate) fn expand_path_template(value: &str, project_path: Option<&Path>) -> PathBuf {
    let mut expanded = value.to_string();
    if let Some(home) = home_dir() {
        let home = home.to_string_lossy();
        expanded = expanded
            .replace("${HOME}", home.as_ref())
            .replace("${USERPROFILE}", home.as_ref())
            .replace('~', home.as_ref());
    }
    if let Some(app_data) = app_data_dir() {
        expanded = expanded.replace("${LOCALAPPDATA}", app_data.to_string_lossy().as_ref());
    }
    if let Some(project) = project_path {
        expanded = expanded.replace("${PROJECT}", project.to_string_lossy().as_ref());
    }
    PathBuf::from(expanded)
}

pub(crate) fn modified_millis(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

pub(crate) fn shell_quote(value: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub(crate) fn list_session_terminals() -> Result<Value, String> {
    let mut terminals = vec![json!({ "id": "auto" })];
    #[cfg(windows)]
    {
        let has_windows_terminal = command_available("wt.exe");
        let has_powershell_7 = command_available("pwsh.exe");
        let has_windows_powershell = command_available("powershell.exe");
        let has_cmd = command_available("cmd.exe");

        if has_windows_terminal && (has_powershell_7 || has_windows_powershell || has_cmd) {
            terminals.push(json!({ "id": "windowsTerminal" }));
        }
        if has_powershell_7 {
            terminals.push(json!({ "id": "powershell7" }));
        }
        if has_windows_powershell {
            terminals.push(json!({ "id": "windowsPowerShell" }));
        }
        if has_cmd {
            terminals.push(json!({ "id": "cmd" }));
        }
    }
    Ok(Value::Array(terminals))
}

pub(crate) fn launch_terminal(
    command: &str,
    cwd: Option<&str>,
    terminal: Option<&str>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        return launch_windows_terminal(command, cwd, terminal.unwrap_or("auto"));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = terminal;
        let script = if let Some(cwd) = cwd {
            format!("cd {} && {}", shell_quote(cwd), command)
        } else {
            command.to_string()
        };
        Command::new("osascript")
            .args([
                "-e",
                &format!("tell application \"Terminal\" to do script {:?}", script),
            ])
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = terminal;
        let script = if let Some(cwd) = cwd {
            format!("cd {} && {}; exec $SHELL", shell_quote(cwd), command)
        } else {
            format!("{}; exec $SHELL", command)
        };
        for terminal in ["x-terminal-emulator", "gnome-terminal", "konsole"] {
            let result = if terminal == "gnome-terminal" {
                Command::new(terminal)
                    .args(["--", "sh", "-lc", &script])
                    .spawn()
            } else {
                Command::new(terminal)
                    .args(["-e", "sh", "-lc", &script])
                    .spawn()
            };
            if result.is_ok() {
                return Ok(());
            }
        }
        Err("No supported terminal emulator was found".to_string())
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum WindowsShell {
    PowerShell7,
    WindowsPowerShell,
    Cmd,
}

#[cfg(windows)]
fn command_available(command: &str) -> bool {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("where.exe")
        .arg(command)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn preferred_windows_shell() -> Option<WindowsShell> {
    if command_available("pwsh.exe") {
        Some(WindowsShell::PowerShell7)
    } else if command_available("powershell.exe") {
        Some(WindowsShell::WindowsPowerShell)
    } else if command_available("cmd.exe") {
        Some(WindowsShell::Cmd)
    } else {
        None
    }
}

#[cfg(windows)]
fn resolve_windows_terminal(preference: &str) -> Result<(bool, WindowsShell), String> {
    let selected = match preference {
        "windowsTerminal" if command_available("wt.exe") => {
            preferred_windows_shell().map(|shell| (true, shell))
        }
        "powershell7" if command_available("pwsh.exe") => Some((false, WindowsShell::PowerShell7)),
        "windowsPowerShell" if command_available("powershell.exe") => {
            Some((false, WindowsShell::WindowsPowerShell))
        }
        "cmd" if command_available("cmd.exe") => Some((false, WindowsShell::Cmd)),
        _ => None,
    };
    if let Some(selected) = selected {
        return Ok(selected);
    }

    let shell = preferred_windows_shell()
        .ok_or_else(|| "No supported Windows command shell was found".to_string())?;
    Ok((command_available("wt.exe"), shell))
}

#[cfg(windows)]
fn launch_windows_terminal(
    command: &str,
    cwd: Option<&str>,
    preference: &str,
) -> Result<(), String> {
    let (use_windows_terminal, shell) = resolve_windows_terminal(preference)?;
    let script = match shell {
        WindowsShell::PowerShell7 | WindowsShell::WindowsPowerShell => {
            if let Some(cwd) = cwd {
                format!("Set-Location -LiteralPath {}; {command}", shell_quote(cwd))
            } else {
                command.to_string()
            }
        }
        WindowsShell::Cmd => {
            if let Some(cwd) = cwd {
                format!("cd /d {} && {command}", cmd_quote(cwd))
            } else {
                command.to_string()
            }
        }
    };

    let executable = match shell {
        WindowsShell::PowerShell7 => "pwsh.exe",
        WindowsShell::WindowsPowerShell => "powershell.exe",
        WindowsShell::Cmd => "cmd.exe",
    };
    let shell_args = match shell {
        WindowsShell::PowerShell7 | WindowsShell::WindowsPowerShell => {
            vec!["-NoExit", "-NoProfile", "-Command", script.as_str()]
        }
        WindowsShell::Cmd => vec!["/k", script.as_str()],
    };

    let mut process = if use_windows_terminal {
        let mut process = Command::new("wt.exe");
        process.args(["-w", "new", "new-tab", executable]);
        process
    } else {
        Command::new(executable)
    };
    process
        .args(shell_args)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
fn cmd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
