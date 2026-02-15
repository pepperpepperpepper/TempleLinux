use super::Vm;
use super::prelude::*;

impl Vm {
    pub(crate) fn enable_capture(&mut self) {
        self.capture = Some(String::new());
    }

    pub(crate) fn captured_output(&self) -> Option<&str> {
        self.capture.as_deref()
    }

    pub(super) fn clear_last_host_error(&mut self) {
        self.last_host_error = None;
    }

    pub(super) fn set_last_host_error(&mut self, msg: impl Into<String>) {
        self.last_host_error = Some(msg.into());
    }

    pub(super) fn linux_run_allowlist(&self) -> Vec<String> {
        if let Ok(v) = std::env::var("TEMPLE_LINUX_RUN_ALLOW") {
            let v = v.trim();
            if !v.is_empty() {
                return v
                    .split(|ch: char| ch == ',' || ch.is_whitespace())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_ascii_lowercase())
                    .collect();
            }
        }

        let Ok(root) = std::env::var("TEMPLE_ROOT") else {
            return Vec::new();
        };
        let path = PathBuf::from(root).join("Cfg/LinuxRunAllow.txt");
        let Ok(text) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        text.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|s| s.to_ascii_lowercase())
            .collect()
    }

    fn env_u32(&self, name: &str, default: u32) -> u32 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(default)
    }

    fn env_bool(&self, name: &str) -> bool {
        let Some(v) = std::env::var(name).ok() else {
            return false;
        };
        matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    }

    fn sway_workspace_number(&self, number: u32) -> Result<(), String> {
        if std::env::var_os("SWAYSOCK").is_none() {
            return Err("SWAYSOCK is not set".to_string());
        }

        let output = std::process::Command::new("swaymsg")
            .arg(format!("workspace number {number}"))
            .output()
            .map_err(|err| format!("swaymsg: {err}"))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut msg = String::new();
        if !stderr.trim().is_empty() {
            msg.push_str(stderr.trim());
        }
        if msg.is_empty() && !stdout.trim().is_empty() {
            msg.push_str(stdout.trim());
        }
        if msg.is_empty() {
            msg.push_str("swaymsg failed");
        }
        Err(msg)
    }

    pub(super) fn maybe_auto_linux_ws(&self) {
        if !self.env_bool("TEMPLE_AUTO_LINUX_WS") {
            return;
        }
        let linux_ws = self.env_u32("TEMPLE_WS_LINUX", 2);
        let _ = self.sway_workspace_number(linux_ws);
    }

    pub(super) fn split_cmdline(cmdline: &str) -> Result<Vec<String>, String> {
        let mut args: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;

        for ch in cmdline.chars() {
            if escaped {
                cur.push(ch);
                escaped = false;
                continue;
            }

            if in_single {
                if ch == '\'' {
                    in_single = false;
                } else {
                    cur.push(ch);
                }
                continue;
            }

            if in_double {
                match ch {
                    '"' => in_double = false,
                    '\\' => escaped = true,
                    _ => cur.push(ch),
                }
                continue;
            }

            match ch {
                '\'' => in_single = true,
                '"' => in_double = true,
                '\\' => escaped = true,
                ch if ch.is_whitespace() => {
                    if !cur.is_empty() {
                        args.push(std::mem::take(&mut cur));
                    }
                }
                _ => cur.push(ch),
            }
        }

        if escaped {
            cur.push('\\');
        }
        if in_single || in_double {
            return Err("unterminated quote in command line".to_string());
        }
        if !cur.is_empty() {
            args.push(cur);
        }
        Ok(args)
    }

    fn resolve_linux_open_target(&self, target: &str) -> Result<PathBuf, String> {
        if target.starts_with("::/") {
            let root = discover_templeos_root().ok_or_else(|| {
                "LinuxOpen: TempleOS tree not found (needed for ::/ paths)".to_string()
            })?;
            return Ok(root.join(target.trim_start_matches("::/")));
        }

        if target.starts_with('/') {
            let root = std::env::var("TEMPLE_ROOT")
                .map_err(|_| "LinuxOpen: TEMPLE_ROOT is not set".to_string())?;
            let rel = target.trim_start_matches('/');
            let temple_root_candidate = PathBuf::from(&root).join(rel);
            if temple_root_candidate.exists() {
                return Ok(temple_root_candidate);
            }

            if let Some(troot) = discover_templeos_root() {
                let templeos_candidate = troot.join(rel);
                if templeos_candidate.exists() {
                    return Ok(templeos_candidate);
                }
            }

            return Ok(temple_root_candidate);
        }

        let base =
            std::env::current_dir().map_err(|err| format!("LinuxOpen: current_dir: {err}"))?;
        Ok(base.join(target))
    }

    fn resolve_linux_open_target_temple_root_only(&self, target: &str) -> Result<PathBuf, String> {
        if target.starts_with('/') {
            let root =
                std::env::var("TEMPLE_ROOT").map_err(|_| "TEMPLE_ROOT is not set".to_string())?;
            let rel = target.trim_start_matches('/');
            return Ok(PathBuf::from(root).join(rel));
        }

        let base = std::env::current_dir().map_err(|err| format!("current_dir: {err}"))?;
        Ok(base.join(target))
    }

    fn normalize_temple_path(path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for comp in path.split('/') {
            if comp.is_empty() || comp == "." {
                continue;
            }
            if comp == ".." {
                parts.pop();
                continue;
            }
            parts.push(comp);
        }
        if parts.is_empty() {
            return "/".to_string();
        }
        format!("/{}", parts.join("/"))
    }

    fn resolve_temple_spec_read(&self, target: &str) -> Result<String, String> {
        let target = target.trim();

        if target.starts_with("::/") {
            return Ok(target.to_string());
        }

        let mut abs = if let Some(rest) = target.strip_prefix("~/") {
            format!("/Home/{rest}")
        } else if target == "~" {
            "/Home".to_string()
        } else if target.starts_with('/') {
            target.to_string()
        } else {
            let base = self.cwd.trim_end_matches('/');
            if base.is_empty() || base == "/" {
                format!("/{target}")
            } else {
                format!("{base}/{target}")
            }
        };

        if !abs.starts_with('/') {
            abs.insert(0, '/');
        }

        Ok(Self::normalize_temple_path(&abs))
    }

    pub(super) fn resolve_temple_spec_write(&self, target: &str) -> Result<String, String> {
        let target = target.trim();
        if target.starts_with("::/") {
            return Err("refusing to write into ::/ (vendored TempleOS tree)".to_string());
        }

        let mut abs = if let Some(rest) = target.strip_prefix("~/") {
            format!("/Home/{rest}")
        } else if target == "~" {
            "/Home".to_string()
        } else if target.starts_with('/') {
            target.to_string()
        } else {
            let base = self.cwd.trim_end_matches('/');
            if base.is_empty() || base == "/" {
                format!("/{target}")
            } else {
                format!("{base}/{target}")
            }
        };

        if !abs.starts_with('/') {
            abs.insert(0, '/');
        }

        Ok(Self::normalize_temple_path(&abs))
    }

    pub(super) fn resolve_temple_fs_target_read(&self, target: &str) -> Result<PathBuf, String> {
        let spec = self.resolve_temple_spec_read(target)?;
        self.resolve_linux_open_target(&spec)
    }

    pub(super) fn resolve_temple_fs_target_write(&self, target: &str) -> Result<PathBuf, String> {
        let spec = self.resolve_temple_spec_write(target)?;
        self.resolve_linux_open_target_temple_root_only(&spec)
    }
}
