use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn usage() -> &'static str {
    r#"temple-bridge-helper

USAGE:
  temple-bridge-helper browse <url>
  temple-bridge-helper open <path>
  temple-bridge-helper run <program> [args...]

ENV:
  TEMPLE_LINUX_RUN_ALLOW    Comma/whitespace-separated allowlist entries.
  TEMPLE_ROOT               If set, also reads allowlist from:
                              $TEMPLE_ROOT/Cfg/LinuxRunAllow.txt

NOTES:
  - This helper spawns the target command and prints its PID to stdout.
  - Spawned processes are detached from the helper's stdio."#
}

fn linux_run_allowlist() -> Vec<String> {
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

fn is_allowed(program: &OsStr, allow: &[String]) -> bool {
    let prog = program.to_string_lossy().to_ascii_lowercase();
    let base = Path::new(program)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| program.to_string_lossy().to_string())
        .to_ascii_lowercase();
    allow.iter().any(|a| a == &prog || a == &base)
}

fn spawn_and_print_pid(mut cmd: Command) -> Result<(), String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = cmd.spawn().map_err(|err| err.to_string())?;
    println!("{}", child.id());
    Ok(())
}

fn require_one_arg(args: &mut std::env::ArgsOs, what: &str) -> Result<OsString, String> {
    let Some(v) = args.next() else {
        return Err(format!("{what}: missing argument"));
    };
    if args.next().is_some() {
        return Err(format!("{what}: expected exactly 1 argument"));
    }
    Ok(v)
}

fn main() {
    let mut args = std::env::args_os();
    let _exe = args.next();

    let Some(sub) = args.next() else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };

    if sub == "-h" || sub == "--help" || sub == "help" {
        eprintln!("{}", usage());
        return;
    }

    let res: Result<(), String> = (|| match sub.to_string_lossy().as_ref() {
        "browse" => {
            let url = require_one_arg(&mut args, "browse")?;
            let mut cmd = Command::new("xdg-open");
            cmd.arg(url);
            spawn_and_print_pid(cmd).map_err(|e| format!("browse: xdg-open: {e}"))
        }
        "open" => {
            let target = require_one_arg(&mut args, "open")?;
            let mut cmd = Command::new("xdg-open");
            cmd.arg(target);
            spawn_and_print_pid(cmd).map_err(|e| format!("open: xdg-open: {e}"))
        }
        "run" => {
            let Some(program) = args.next() else {
                return Err("run: missing program".to_string());
            };
            let rest: Vec<OsString> = args.collect();

            let allow = linux_run_allowlist();
            if allow.is_empty() {
                return Err(
                        "run: disabled (set TEMPLE_LINUX_RUN_ALLOW or create TEMPLE_ROOT/Cfg/LinuxRunAllow.txt)"
                            .to_string(),
                    );
            }
            if !is_allowed(&program, &allow) {
                return Err(format!("run: not allowed: {}", program.to_string_lossy()));
            }

            let mut cmd = Command::new(program);
            cmd.args(rest);
            spawn_and_print_pid(cmd).map_err(|e| format!("run: {e}"))
        }
        other => Err(format!("unknown subcommand: {other}")),
    })();

    if let Err(err) = res {
        exit_err(&err);
    }
}

fn exit_err(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}
