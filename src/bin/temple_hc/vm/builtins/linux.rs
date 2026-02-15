use super::super::prelude::*;
use super::super::{Value, Vm};

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
                let Value::Str(url) = v else {
                    return Err("LinuxBrowse expects a string url".to_string());
                };

                self.clear_last_host_error();
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
                let Value::Str(target) = v else {
                    return Err("LinuxOpen expects a string path".to_string());
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
                let Value::Str(cmdline) = v else {
                    return Err("LinuxRun expects a string command line".to_string());
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
            _ => Err(format!("internal: call_builtin_linux cannot handle {name}")),
        }
    }
}
