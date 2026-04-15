use super::super::prelude::*;
use super::super::{Value, Vm};

fn helper_pid(output: &std::process::Output) -> Result<i64, String> {
    let s = String::from_utf8_lossy(&output.stdout);
    let s = s.trim();
    if s.is_empty() {
        return Err("bridge helper returned an empty pid".to_string());
    }
    let pid = s
        .parse::<i64>()
        .map_err(|_| format!("bridge helper returned an invalid pid: {s}"))?;
    if pid <= 0 {
        return Err(format!("bridge helper returned a non-positive pid: {pid}"));
    }
    Ok(pid)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|err| format!("{}: {err}", dst.display()))?;

    let entries = std::fs::read_dir(src).map_err(|err| format!("{}: {err}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let ty = entry.file_type().map_err(|err| err.to_string())?;
        let src_path = entry.path();
        if ty.is_symlink() {
            return Err(format!("refusing to copy symlink: {}", src_path.display()));
        }
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
            continue;
        }
        if ty.is_file() {
            if let Some(parent) = dst_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("{}: {err}", parent.display()))?;
            }
            std::fs::copy(&src_path, &dst_path).map_err(|err| {
                format!(
                    "copy {} -> {}: {err}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
            continue;
        }

        return Err(format!(
            "refusing to copy non-file/non-dir: {}",
            src_path.display()
        ));
    }

    Ok(())
}

fn copy_any(src: &Path, dst: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src).map_err(|err| format!("{}: {err}", src.display()))?;
    let ty = meta.file_type();

    if ty.is_symlink() {
        return Err(format!("refusing to copy symlink: {}", src.display()));
    }

    if ty.is_file() {
        let mut dst_file = dst.to_path_buf();
        if let Ok(dst_meta) = std::fs::metadata(dst) {
            if dst_meta.is_dir() {
                let name = src
                    .file_name()
                    .ok_or_else(|| "source file has no name".to_string())?;
                dst_file = dst.join(name);
            } else if !dst_meta.is_file() {
                return Err(format!(
                    "destination exists and is not a file: {}",
                    dst.display()
                ));
            }
        }

        if let Some(parent) = dst_file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("{}: {err}", parent.display()))?;
        }
        std::fs::copy(src, &dst_file)
            .map_err(|err| format!("copy {} -> {}: {err}", src.display(), dst_file.display()))?;
        return Ok(());
    }

    if ty.is_dir() {
        let mut dst_dir = dst.to_path_buf();
        if let Ok(dst_meta) = std::fs::metadata(dst) {
            if dst_meta.is_dir() {
                let name = src
                    .file_name()
                    .ok_or_else(|| "source directory has no name".to_string())?;
                dst_dir = dst.join(name);
            } else {
                return Err(format!(
                    "destination exists and is not a directory: {}",
                    dst.display()
                ));
            }
        }

        copy_dir_recursive(src, &dst_dir)?;
        return Ok(());
    }

    Err(format!("refusing to copy special file: {}", src.display()))
}

impl Vm {
    pub(super) fn call_builtin_linux(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> Result<Value, String> {
        match name {
            "LinuxLastErr" => {
                if !args.is_empty() {
                    return Err("LinuxLastErr expects 0 args".to_string());
                }
                Ok(Value::Str(self.last_host_error.clone().unwrap_or_default()))
            }
            "LinuxBrowse" => {
                if args.len() != 1 {
                    return Err("LinuxBrowse(\"url\") expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                let url = match v {
                    Value::Str(url) => url,
                    Value::Int(0) => return Err("LinuxBrowse expects a string url".to_string()),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "LinuxBrowse expects a string or pointer, got {other:?}"
                        ));
                    }
                };

                self.clear_last_host_error();
                for helper in self.bridge_helper_candidates() {
                    match std::process::Command::new(&helper)
                        .arg("browse")
                        .arg(&url)
                        .output()
                    {
                        Ok(out) => {
                            if out.status.success() {
                                match helper_pid(&out) {
                                    Ok(pid) => {
                                        self.maybe_auto_linux_ws();
                                        return Ok(Value::Int(pid));
                                    }
                                    Err(err) => {
                                        self.set_last_host_error(format!("LinuxBrowse: {err}"));
                                        return Ok(Value::Int(0));
                                    }
                                }
                            }

                            let msg = String::from_utf8_lossy(&out.stderr);
                            let msg = msg.trim();
                            if msg.is_empty() {
                                self.set_last_host_error(
                                    "LinuxBrowse: bridge helper failed".to_string(),
                                );
                            } else {
                                self.set_last_host_error(format!("LinuxBrowse: {msg}"));
                            }
                            return Ok(Value::Int(0));
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(err) => {
                            self.set_last_host_error(format!("LinuxBrowse: bridge helper: {err}"));
                            return Ok(Value::Int(0));
                        }
                    }
                }
                match std::process::Command::new("xdg-open").arg(&url).spawn() {
                    Ok(child) => {
                        self.maybe_auto_linux_ws();
                        Ok(Value::Int(child.id() as i64))
                    }
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxBrowse: xdg-open: {err}"));
                        Ok(Value::Int(0))
                    }
                }
            }
            "LinuxOpen" => {
                if args.len() != 1 {
                    return Err("LinuxOpen(\"path\") expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                let target = match v {
                    Value::Str(target) => target,
                    Value::Int(0) => return Err("LinuxOpen expects a string path".to_string()),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "LinuxOpen expects a string or pointer, got {other:?}"
                        ));
                    }
                };

                self.clear_last_host_error();
                let host = match self.resolve_temple_fs_target_read(&target) {
                    Ok(p) => p,
                    Err(err) => {
                        self.set_last_host_error(err);
                        return Ok(Value::Int(0));
                    }
                };

                if !host.exists() {
                    self.set_last_host_error(format!("LinuxOpen: not found: {}", host.display()));
                    return Ok(Value::Int(0));
                }

                for helper in self.bridge_helper_candidates() {
                    match std::process::Command::new(&helper)
                        .arg("open")
                        .arg(&host)
                        .output()
                    {
                        Ok(out) => {
                            if out.status.success() {
                                match helper_pid(&out) {
                                    Ok(pid) => {
                                        self.maybe_auto_linux_ws();
                                        return Ok(Value::Int(pid));
                                    }
                                    Err(err) => {
                                        self.set_last_host_error(format!("LinuxOpen: {err}"));
                                        return Ok(Value::Int(0));
                                    }
                                }
                            }

                            let msg = String::from_utf8_lossy(&out.stderr);
                            let msg = msg.trim();
                            if msg.is_empty() {
                                self.set_last_host_error(
                                    "LinuxOpen: bridge helper failed".to_string(),
                                );
                            } else {
                                self.set_last_host_error(format!("LinuxOpen: {msg}"));
                            }
                            return Ok(Value::Int(0));
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(err) => {
                            self.set_last_host_error(format!("LinuxOpen: bridge helper: {err}"));
                            return Ok(Value::Int(0));
                        }
                    }
                }
                match std::process::Command::new("xdg-open").arg(&host).spawn() {
                    Ok(child) => {
                        self.maybe_auto_linux_ws();
                        Ok(Value::Int(child.id() as i64))
                    }
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxOpen: xdg-open: {err}"));
                        Ok(Value::Int(0))
                    }
                }
            }
            "LinuxRun" => {
                if args.len() != 1 {
                    return Err("LinuxRun(\"cmd...\") expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                let cmdline = match v {
                    Value::Str(cmdline) => cmdline,
                    Value::Int(0) => {
                        return Err("LinuxRun expects a string command line".to_string());
                    }
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "LinuxRun expects a string or pointer, got {other:?}"
                        ));
                    }
                };

                self.clear_last_host_error();

                let argv = match Self::split_cmdline(&cmdline) {
                    Ok(v) => v,
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxRun: {err}"));
                        return Ok(Value::Int(0));
                    }
                };
                let Some((program, args)) = argv.split_first() else {
                    self.set_last_host_error("LinuxRun: missing program".to_string());
                    return Ok(Value::Int(0));
                };

                let allow = self.linux_run_allowlist();
                if allow.is_empty() {
                    self.set_last_host_error(
                        "LinuxRun: disabled (set TEMPLE_LINUX_RUN_ALLOW or create TEMPLE_ROOT/Cfg/LinuxRunAllow.txt)"
                            .to_string(),
                    );
                    return Ok(Value::Int(0));
                }

                let prog = program.to_ascii_lowercase();
                let base = Path::new(program)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| program.clone())
                    .to_ascii_lowercase();
                if !allow.iter().any(|a| a == &prog || a == &base) {
                    self.set_last_host_error(format!("LinuxRun: not allowed: {program}"));
                    return Ok(Value::Int(0));
                }

                let host_cwd = std::env::current_dir()
                    .ok()
                    .unwrap_or_else(|| PathBuf::from("."));

                for helper in self.bridge_helper_candidates() {
                    match std::process::Command::new(&helper)
                        .arg("run")
                        .arg(program)
                        .args(args)
                        .current_dir(&host_cwd)
                        .output()
                    {
                        Ok(out) => {
                            if out.status.success() {
                                match helper_pid(&out) {
                                    Ok(pid) => {
                                        self.maybe_auto_linux_ws();
                                        return Ok(Value::Int(pid));
                                    }
                                    Err(err) => {
                                        self.set_last_host_error(format!("LinuxRun: {err}"));
                                        return Ok(Value::Int(0));
                                    }
                                }
                            }

                            let msg = String::from_utf8_lossy(&out.stderr);
                            let msg = msg.trim();
                            if msg.is_empty() {
                                self.set_last_host_error(
                                    "LinuxRun: bridge helper failed".to_string(),
                                );
                            } else {
                                self.set_last_host_error(format!("LinuxRun: {msg}"));
                            }
                            return Ok(Value::Int(0));
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(err) => {
                            self.set_last_host_error(format!("LinuxRun: bridge helper: {err}"));
                            return Ok(Value::Int(0));
                        }
                    }
                }

                let mut cmd = std::process::Command::new(program);
                cmd.args(args).current_dir(host_cwd);
                match cmd.spawn() {
                    Ok(child) => {
                        self.maybe_auto_linux_ws();
                        Ok(Value::Int(child.id() as i64))
                    }
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxRun: {program}: {err}"));
                        Ok(Value::Int(0))
                    }
                }
            }
            "LinuxCopyToHost" => {
                if args.len() != 2 {
                    return Err(
                        "LinuxCopyToHost(\"temple_src\", \"host_dst\") expects 2 args".to_string(),
                    );
                }
                let v0 = self.eval_expr(&args[0])?;
                let temple_src = match v0 {
                    Value::Str(s) => s,
                    Value::Int(0) => {
                        return Err("LinuxCopyToHost expects a string temple_src".to_string());
                    }
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "LinuxCopyToHost expects a string or pointer, got {other:?}"
                        ));
                    }
                };

                let v1 = self.eval_expr(&args[1])?;
                let host_dst = match v1 {
                    Value::Str(s) => s,
                    Value::Int(0) => {
                        return Err("LinuxCopyToHost expects a string host_dst".to_string());
                    }
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "LinuxCopyToHost expects a string or pointer, got {other:?}"
                        ));
                    }
                };

                self.clear_last_host_error();
                let src = match self.resolve_temple_fs_target_read(&temple_src) {
                    Ok(p) => p,
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxCopyToHost: {err}"));
                        return Ok(Value::Int(0));
                    }
                };
                if !src.exists() {
                    self.set_last_host_error(format!(
                        "LinuxCopyToHost: not found: {}",
                        src.display()
                    ));
                    return Ok(Value::Int(0));
                }

                let dst = match self.resolve_linux_host_path(&host_dst) {
                    Ok(p) => p,
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxCopyToHost: {err}"));
                        return Ok(Value::Int(0));
                    }
                };

                match copy_any(&src, &dst) {
                    Ok(()) => Ok(Value::Int(1)),
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxCopyToHost: {err}"));
                        Ok(Value::Int(0))
                    }
                }
            }
            "LinuxCopyFromHost" => {
                if args.len() != 2 {
                    return Err(
                        "LinuxCopyFromHost(\"host_src\", \"temple_dst\") expects 2 args"
                            .to_string(),
                    );
                }
                let v0 = self.eval_expr(&args[0])?;
                let host_src = match v0 {
                    Value::Str(s) => s,
                    Value::Int(0) => {
                        return Err("LinuxCopyFromHost expects a string host_src".to_string());
                    }
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "LinuxCopyFromHost expects a string or pointer, got {other:?}"
                        ));
                    }
                };

                let v1 = self.eval_expr(&args[1])?;
                let temple_dst = match v1 {
                    Value::Str(s) => s,
                    Value::Int(0) => {
                        return Err("LinuxCopyFromHost expects a string temple_dst".to_string());
                    }
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "LinuxCopyFromHost expects a string or pointer, got {other:?}"
                        ));
                    }
                };

                self.clear_last_host_error();

                let src = match self.resolve_linux_host_path(&host_src) {
                    Ok(p) => p,
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxCopyFromHost: {err}"));
                        return Ok(Value::Int(0));
                    }
                };
                if !src.exists() {
                    self.set_last_host_error(format!(
                        "LinuxCopyFromHost: not found: {}",
                        src.display()
                    ));
                    return Ok(Value::Int(0));
                }

                let dst = match self.resolve_temple_fs_target_write(&temple_dst) {
                    Ok(p) => p,
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxCopyFromHost: {err}"));
                        return Ok(Value::Int(0));
                    }
                };

                match copy_any(&src, &dst) {
                    Ok(()) => Ok(Value::Int(1)),
                    Err(err) => {
                        self.set_last_host_error(format!("LinuxCopyFromHost: {err}"));
                        Ok(Value::Int(0))
                    }
                }
            }
            _ => Err(format!("internal: call_builtin_linux cannot handle {name}")),
        }
    }
}
