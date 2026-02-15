use super::super::prelude::*;
use super::super::{Obj, Value, Vm};

impl Vm {
    pub(super) fn call_builtin_doc_fs_settings(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> Result<Value, String> {
        match name {
            "DocClear" => {
                self.rt.clear(0);
                self.text_x = 0;
                self.text_y = 0;
                self.text_fg = 15;
                self.text_bg = 0;
                Ok(Value::Void)
            }
            "Cd" => {
                if args.len() > 2 {
                    return Err("Cd(dir=\"...\", make_dirs=FALSE) expects 0-2 args".to_string());
                }

                let dir_v = match args.first() {
                    None | Some(Expr::DefaultArg) => Value::Int(0),
                    Some(expr) => self.eval_expr(expr)?,
                };
                let mut dir = match dir_v {
                    Value::Int(0) => "~".to_string(),
                    Value::Str(s) => s,
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "Cd: dir must be a string or pointer, got {other:?}"
                        ));
                    }
                };
                dir = dir.trim().to_string();
                if dir.is_empty() {
                    return Ok(Value::Int(1));
                }

                let make_dirs = match args.get(1) {
                    None | Some(Expr::DefaultArg) => false,
                    Some(expr) => self.eval_expr(expr)?.as_i64()? != 0,
                };

                // We treat the vendored TempleOS tree as a read-only base layer, and the Temple
                // root as an overlay. For compatibility, allow `Cd("::/...")` but map it into the
                // overlay namespace (`/...`) so relative writes still go into `TEMPLE_ROOT`.
                if let Some(rest) = dir.strip_prefix("::/") {
                    dir = format!("/{rest}");
                }

                let new_cwd = match self.resolve_temple_spec_write(&dir) {
                    Ok(v) => v,
                    Err(err) => return Err(format!("Cd: {err}")),
                };

                let ok = match self.resolve_temple_fs_target_read(&new_cwd) {
                    Ok(host) => std::fs::metadata(&host)
                        .map(|m| m.is_dir())
                        .unwrap_or(false),
                    Err(_) => false,
                };

                if ok {
                    self.cwd = new_cwd.clone();
                    if let Some(Value::Obj(fs)) = self.env.get("Fs") {
                        fs.borrow_mut()
                            .fields
                            .insert("cur_dir".to_string(), Value::Str(new_cwd));
                    }
                    return Ok(Value::Int(1));
                }

                if make_dirs {
                    let host = self
                        .resolve_temple_fs_target_write(&new_cwd)
                        .map_err(|err| format!("Cd: {err}"))?;
                    if std::fs::create_dir_all(&host).is_ok() {
                        self.cwd = new_cwd.clone();
                        if let Some(Value::Obj(fs)) = self.env.get("Fs") {
                            fs.borrow_mut()
                                .fields
                                .insert("cur_dir".to_string(), Value::Str(new_cwd));
                        }
                        return Ok(Value::Int(1));
                    }
                }

                Ok(Value::Int(0))
            }
            "RegDft" => {
                if args.len() != 2 {
                    return Err("RegDft(key, code) expects 2 args".to_string());
                }
                let key_v = self.eval_expr(&args[0])?;
                let key = match key_v {
                    Value::Str(s) => s,
                    Value::Int(0) => return Err("RegDft: key must be non-NULL".to_string()),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "RegDft: key must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                let code_v = self.eval_expr(&args[1])?;
                let code = match code_v {
                    Value::Str(s) => s,
                    Value::Int(0) => String::new(),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "RegDft: code must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                self.reg_defaults.insert(key, code);
                Ok(Value::Void)
            }
            "RegExe" => {
                if args.len() != 1 {
                    return Err("RegExe(key) expects 1 arg".to_string());
                }
                let key_v = self.eval_expr(&args[0])?;
                let key = match key_v {
                    Value::Str(s) => s,
                    Value::Int(0) => return Err("RegExe: key must be non-NULL".to_string()),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "RegExe: key must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                let Some(code) = self.reg_defaults.get(&key).cloned() else {
                    return Ok(Value::Void);
                };

                let label: Arc<str> = Arc::from(format!("<reg:{key}>"));
                self.exec_snippet(label, &code)?;
                Ok(Value::Void)
            }
            "RegWrite" => {
                if args.len() < 2 {
                    return Err("RegWrite(key, fmt, ...) expects at least 2 args".to_string());
                }
                let key_v = self.eval_expr(&args[0])?;
                let key = match key_v {
                    Value::Str(s) => s,
                    Value::Int(0) => return Err("RegWrite: key must be non-NULL".to_string()),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "RegWrite: key must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                let fmt_v = self.eval_expr(&args[1])?;
                let fmt = match fmt_v {
                    Value::Str(s) => s,
                    Value::Int(0) => String::new(),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "RegWrite: fmt must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                let mut values: Vec<Value> = Vec::new();
                for expr in args.get(2..).unwrap_or(&[]) {
                    if matches!(expr, Expr::DefaultArg) {
                        values.push(Value::Int(0));
                    } else {
                        values.push(self.eval_expr(expr)?);
                    }
                }

                let rendered = format_temple_fmt_with_cstr(
                    &fmt,
                    &values,
                    |ptr| self.read_cstr_lossy(ptr),
                    |idx, name| self.define_sub(idx, name),
                )?;

                self.reg_defaults.insert(key.clone(), rendered.clone());
                let label: Arc<str> = Arc::from(format!("<reg:{key}>"));
                self.exec_snippet(label, &rendered)?;
                Ok(Value::Void)
            }
            "PopUpOk" => {
                if args.len() != 1 {
                    return Err("PopUpOk(msg) expects 1 arg".to_string());
                }
                let _msg = self.eval_expr(&args[0])?;
                Ok(Value::Void)
            }
            "DefineLstLoad" => {
                if args.len() != 2 {
                    return Err("DefineLstLoad(name, entries) expects 2 args".to_string());
                }

                let name_v = self.eval_expr(&args[0])?;
                let name = match name_v {
                    Value::Str(s) => s,
                    Value::Int(0) => return Err("DefineLstLoad: name must be non-NULL".to_string()),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "DefineLstLoad: name must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                let entries_v = self.eval_expr(&args[1])?;
                let entries = match entries_v {
                    Value::Str(s) => s,
                    Value::Int(0) => String::new(),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "DefineLstLoad: entries must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                let mut parts: Vec<String> = entries.split('\0').map(|s| s.to_string()).collect();
                if parts.last().is_some_and(|s| s.is_empty()) {
                    parts.pop();
                }
                self.define_lists.insert(name, parts);
                Ok(Value::Void)
            }
            "DefineSub" => {
                if args.len() != 2 {
                    return Err("DefineSub(index, list_name) expects 2 args".to_string());
                }
                let idx = self.eval_expr(&args[0])?.as_i64()?;

                let name_v = self.eval_expr(&args[1])?;
                let name = match name_v {
                    Value::Str(s) => s,
                    Value::Int(0) => return Ok(Value::Str(String::new())),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "DefineSub: list_name must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                Ok(Value::Str(self.define_sub(idx, &name).unwrap_or_default()))
            }
            "FileFind" => {
                if args.is_empty() {
                    return Err("FileFind(path, ...) expects at least 1 arg".to_string());
                }
                let path = match self.eval_expr(&args[0])? {
                    Value::Str(s) => s,
                    _ => return Err("FileFind: path must be a string".to_string()),
                };

                // TempleOS API: FileFind(filename, tmp=NULL, flags=FUG_FILE_FIND)
                // We only implement a minimal subset used by installers: existence + directory check.
                let flags = match args.get(2) {
                    None | Some(Expr::DefaultArg) => 0i64,
                    Some(expr) => self.eval_expr(expr)?.as_i64()?,
                };
                let want_dirs_only = (flags & 0x0000_400) != 0;

                let host_path = self.resolve_temple_fs_target_read(&path)?;
                let ok = match std::fs::metadata(&host_path) {
                    Ok(meta) => {
                        if want_dirs_only {
                            meta.is_dir()
                        } else {
                            true
                        }
                    }
                    Err(_) => false,
                };
                Ok(Value::Int(ok as i64))
            }
            "DirMk" => {
                if args.len() != 1 {
                    return Err("DirMk(path) expects 1 arg".to_string());
                }
                let path = match self.eval_expr(&args[0])? {
                    Value::Str(s) => s,
                    _ => return Err("DirMk: path must be a string".to_string()),
                };
                let host_path = self.resolve_temple_fs_target_write(&path)?;
                std::fs::create_dir_all(&host_path)
                    .map_err(|err| format!("DirMk: {}: {err}", host_path.display()))?;
                Ok(Value::Int(1))
            }
            "WinMax" => {
                if args.len() > 1 {
                    return Err("WinMax(flag=ON) expects 0-1 args".to_string());
                }
                Ok(Value::Void)
            }
            "WinBorder" => {
                if args.len() > 1 {
                    return Err("WinBorder(flag=ON) expects 0-1 args".to_string());
                }
                Ok(Value::Void)
            }
            "DocCursor" => {
                if !args.is_empty() {
                    return Err("DocCursor expects 0 args".to_string());
                }
                Ok(Value::Void)
            }
            "DocBottom" => {
                if !args.is_empty() {
                    return Err("DocBottom expects 0 args".to_string());
                }
                Ok(Value::Void)
            }
            "DocScroll" => {
                if !args.is_empty() {
                    return Err("DocScroll expects 0 args".to_string());
                }
                Ok(Value::Void)
            }
            "SettingsPush" => {
                if args.len() > 2 {
                    return Err("SettingsPush(task=NULL, flags=0) expects 0-2 args".to_string());
                }
                self.rt.settings_push().map_err(|e| e.to_string())?;
                Ok(Value::Void)
            }
            "SettingsPop" => {
                if args.len() > 2 {
                    return Err("SettingsPop(task=NULL, flags=0) expects 0-2 args".to_string());
                }
                self.rt.settings_pop().map_err(|e| e.to_string())?;
                Ok(Value::Void)
            }
            "AutoComplete" => {
                if !args.is_empty() {
                    return Err("AutoComplete expects 0 args".to_string());
                }
                Ok(Value::Void)
            }
            "Spawn" => {
                if args.is_empty() {
                    return Err("Spawn(func, ...) expects at least 1 arg".to_string());
                }
                // Minimal stub: TempleOS uses Spawn for lightweight tasks; implement as a no-op so
                // upstream sources that set up background animation continue to run.
                Ok(Value::Int(1))
            }
            "PutExcept" => {
                if !args.is_empty() {
                    return Err("PutExcept expects 0 args".to_string());
                }
                Ok(Value::Void)
            }
            "DCFill" => {
                self.rt.clear(0);
                self.present_with_overlays()?;
                Ok(Value::Void)
            }
            "DCAlias" => {
                if args.len() > 2 {
                    return Err("DCAlias(dc=NULL,task=NULL) expects 0-2 args".to_string());
                }

                let dc_arg = match args.get(0) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => Some(self.eval_expr(expr)?),
                };
                let task_arg = match args.get(1) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => Some(self.eval_expr(expr)?),
                };

                let base = match dc_arg {
                    None | Some(Value::Int(0)) => self.dc_alias.clone(),
                    Some(Value::Obj(obj)) => obj,
                    Some(Value::VarRef(name)) => match self.env.get(&name) {
                        Some(Value::Obj(obj)) => obj,
                        _ => self.dc_alias.clone(),
                    },
                    _ => self.dc_alias.clone(),
                };

                let mut fields = base.borrow().fields.clone();
                fields.entry("color".to_string()).or_insert(Value::Int(15));
                fields.entry("thick".to_string()).or_insert(Value::Int(1));
                fields.entry("flags".to_string()).or_insert(Value::Int(0));

                if let Some(Value::Obj(task)) = task_arg {
                    fields.insert("win_task".to_string(), Value::Obj(task.clone()));
                    fields.insert("mem_task".to_string(), Value::Obj(task));
                }

                Ok(Value::Obj(Rc::new(RefCell::new(Obj { fields }))))
            }
            "DCSymmetrySet" => {
                // Minimal stub: record symmetry line endpoints on the dc for sprite mirroring.
                // TempleOS signature: DCSymmetrySet(dc, x1, y1, x2, y2).
                let (dc, x1, y1, x2, y2) = match args.len() {
                    4 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[2],
                        &args[3],
                    ),
                    5 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], &args[4])
                    }
                    _ => {
                        return Err(
                            "DCSymmetrySet(dc?, x1, y1, x2, y2) expects 4 or 5 args".to_string()
                        );
                    }
                };

                let x1 = self.eval_expr(x1)?.as_i64()?;
                let y1 = self.eval_expr(y1)?.as_i64()?;
                let x2 = self.eval_expr(x2)?.as_i64()?;
                let y2 = self.eval_expr(y2)?.as_i64()?;

                if let Value::Obj(dc) = dc {
                    let mut dc = dc.borrow_mut();
                    dc.fields.insert("sym_x1".to_string(), Value::Int(x1));
                    dc.fields.insert("sym_y1".to_string(), Value::Int(y1));
                    dc.fields.insert("sym_x2".to_string(), Value::Int(x2));
                    dc.fields.insert("sym_y2".to_string(), Value::Int(y2));
                }

                Ok(Value::Void)
            }
            "DCDel" => Ok(Value::Void),
            "PressAKey" => {
                if !args.is_empty() {
                    return Err("PressAKey expects 0 args".to_string());
                }
                // TempleOS draws directly to VRAM; flush at least once before we block so the
                // user sees the frame.
                self.present_with_overlays()?;
                let mut last_present = std::time::Instant::now();
                loop {
                    self.poll_events()?;
                    if let Some(code) = self.key_queue.pop_front() {
                        return Ok(Value::Int(code as i64));
                    }
                    if last_present.elapsed() >= Duration::from_millis(16) {
                        self.present_with_overlays()?;
                        last_present = std::time::Instant::now();
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
            "GetStr" => {
                if args.len() > 3 {
                    return Err("GetStr(msg=NULL,dft=NULL,flags=0) expects 0-3 args".to_string());
                }

                const GSF_WITH_NEW_LINE: i64 = 2;
                const MAX_BYTES: usize = 4096;

                let msg = match args.get(0) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => match self.eval_expr(expr)? {
                        Value::Int(0) => None,
                        Value::Str(s) => Some(s),
                        Value::Int(ptr) => Some(self.read_cstr_lossy(ptr)?),
                        Value::Ptr { addr, .. } => Some(self.read_cstr_lossy(addr)?),
                        _ => return Err("GetStr: msg must be a string or NULL".to_string()),
                    },
                };

                let dft = match args.get(1) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => match self.eval_expr(expr)? {
                        Value::Int(0) => None,
                        Value::Str(s) => Some(s),
                        Value::Int(ptr) => Some(self.read_cstr_lossy(ptr)?),
                        Value::Ptr { addr, .. } => Some(self.read_cstr_lossy(addr)?),
                        _ => return Err("GetStr: dft must be a string or NULL".to_string()),
                    },
                };

                let flags = match args.get(2) {
                    None | Some(Expr::DefaultArg) => 0i64,
                    Some(expr) => self.eval_expr(expr)?.as_i64()?,
                };
                let with_new_line = (flags & GSF_WITH_NEW_LINE) != 0;

                let (sw, sh) = self.rt.size();
                let sw = sw as i32;
                let sh = sh as i32;

                // Simple prompt overlay at the bottom of the app window.
                let input_lines = if with_new_line { 4i32 } else { 1i32 };
                let prompt_h = 8i32 * (1 + input_lines + 1);
                let prompt_y = (sh - prompt_h).max(0);

                let underlay = Self::menu_capture_underlay(&mut self.rt, 0, prompt_y, sw, prompt_h);

                let mut input = dft.unwrap_or_default();
                let mut dirty = true;
                let mut cancel_null = false;

                loop {
                    if dirty {
                        // Header line.
                        self.rt.fill_rect(0, prompt_y, sw, 8, 1);
                        // Input + hint lines.
                        self.rt
                            .fill_rect(0, prompt_y + 8, sw, 8 * (input_lines + 1), 0);

                        if let Some(msg) = msg.as_deref() {
                            self.rt.draw_text(0, prompt_y, 15, 1, msg);
                        } else {
                            self.rt.draw_text(0, prompt_y, 15, 1, "Input:");
                        }

                        let cols = (sw / 8).max(0) as usize;

                        if with_new_line {
                            let raw_lines: Vec<&str> = input.split('\n').collect();
                            let total = raw_lines.len().max(1);
                            let start = total.saturating_sub(input_lines as usize);
                            let visible = &raw_lines[start..];
                            for i in 0..input_lines as usize {
                                let raw = visible.get(i).copied().unwrap_or("");
                                let prefix = if i == 0 { "> " } else { "  " };
                                let max_chars = cols.saturating_sub(prefix.len() + 1);
                                let mut shown = raw.to_string();
                                let shown_len = shown.chars().count();
                                if shown_len > max_chars {
                                    let skip = shown_len - max_chars;
                                    shown = shown.chars().skip(skip).collect();
                                }
                                let mut line = format!("{prefix}{shown}");
                                if i + 1 == visible.len() {
                                    line.push('_');
                                }
                                let y = prompt_y + 8 + (i as i32) * 8;
                                self.rt.draw_text(0, y, 15, 0, &line);
                            }
                            self.rt.draw_text(
                                0,
                                prompt_y + 8 + input_lines * 8,
                                7,
                                0,
                                "Enter=NewLine  Esc=Done  Shift+Esc=Empty",
                            );
                        } else {
                            let prefix = "> ";
                            let max_chars = cols.saturating_sub(prefix.len() + 1);
                            let mut shown = input.clone();
                            let shown_len = shown.chars().count();
                            if shown_len > max_chars {
                                let skip = shown_len - max_chars;
                                shown = shown.chars().skip(skip).collect();
                            }
                            let line = format!("{prefix}{shown}_");
                            self.rt.draw_text(0, prompt_y + 8, 15, 0, &line);
                            self.rt
                                .draw_text(0, prompt_y + 16, 7, 0, "Enter=OK  Esc=Cancel");
                        }

                        self.present_with_overlays()?;
                        dirty = false;
                    }

                    self.poll_events()?;
                    let Some(code) = self.key_queue.pop_front() else {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    };

                    match code {
                        0x08 => {
                            // CH_BACKSPACE
                            input.pop();
                            dirty = true;
                        }
                        0x1B | 0x1C => {
                            // CH_ESC / CH_SHIFT_ESC
                            if with_new_line {
                                if code == 0x1C {
                                    input.clear();
                                }
                                break;
                            } else {
                                cancel_null = true;
                                break;
                            }
                        }
                        10 => {
                            if with_new_line {
                                if input.len() < MAX_BYTES.saturating_sub(1) {
                                    input.push('\n');
                                    dirty = true;
                                }
                            } else {
                                break;
                            }
                        }
                        other => {
                            if let Ok(b) = u8::try_from(other) {
                                if b.is_ascii_graphic() || b == b' ' {
                                    if input.len() >= MAX_BYTES.saturating_sub(1) {
                                        continue;
                                    }
                                    if with_new_line {
                                        let cols = (sw / 8).max(0) as usize;
                                        let max_chars = cols.saturating_sub(3);
                                        let cur_line_len =
                                            input.split('\n').last().unwrap_or("").chars().count();
                                        if cur_line_len < max_chars {
                                            input.push(b as char);
                                            dirty = true;
                                        }
                                    } else {
                                        let max_chars = ((sw / 8) as usize).saturating_sub(3);
                                        if input.chars().count() < max_chars {
                                            input.push(b as char);
                                            dirty = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(underlay) = underlay.as_ref() {
                    Self::menu_restore_underlay(&mut self.rt, underlay);
                    self.present_with_overlays()?;
                }

                if cancel_null {
                    return Ok(Value::Int(0));
                }

                let bytes = input.as_bytes();
                let addr = self.heap_alloc(bytes.len() + 1, true);
                self.heap_write_bytes(addr, bytes)?;
                let _ = self.heap_write_u8(addr + bytes.len() as i64, 0);
                Ok(Value::Int(addr))
            }
            "ClipPutS" => {
                if args.len() != 1 {
                    return Err("ClipPutS(\"text\") expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                let Value::Str(text) = v else {
                    return Err("ClipPutS expects a string".to_string());
                };
                self.rt
                    .clipboard_set_text(&text)
                    .map_err(|e| e.to_string())?;
                Ok(Value::Void)
            }
            _ => Err(format!(
                "internal: call_builtin_doc_fs_settings cannot handle {name}"
            )),
        }
    }
}
