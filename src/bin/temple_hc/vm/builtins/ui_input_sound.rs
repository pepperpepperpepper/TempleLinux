use super::super::prelude::*;
use super::super::{Value, Vm};

impl Vm {
    pub(super) fn call_builtin_ui_input_sound(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> Result<Value, String> {
        match name {
            "GridInit" => {
                if !args.is_empty() {
                    return Err("GridInit expects 0 args".to_string());
                }

                if let Some(Value::Obj(ms_grid)) = self.env.get("ms_grid") {
                    let mut g = ms_grid.borrow_mut();
                    g.fields.insert("snap".to_string(), Value::Int(0));
                    g.fields.insert("x".to_string(), Value::Int(0));
                    g.fields.insert("y".to_string(), Value::Int(0));
                }

                Ok(Value::Void)
            }
            "SetPixel" => {
                let x = self.eval_arg_i64(args, 0)? as i32;
                let y = self.eval_arg_i64(args, 1)? as i32;
                let c = self.eval_arg_i64(args, 2)? as u8;
                self.rt.set_pixel(x, y, c);
                Ok(Value::Void)
            }
            "FillRect" => {
                let x = self.eval_arg_i64(args, 0)? as i32;
                let y = self.eval_arg_i64(args, 1)? as i32;
                let w = self.eval_arg_i64(args, 2)? as i32;
                let h = self.eval_arg_i64(args, 3)? as i32;
                let c = self.eval_arg_i64(args, 4)? as u8;
                self.rt.fill_rect(x, y, w, h, c);
                Ok(Value::Void)
            }
            "Text" => {
                if args.len() != 5 {
                    return Err("Text(x,y,fg,bg,\"str\") expects 5 args".to_string());
                }
                let x = self.eval_arg_i64(args, 0)? as i32;
                let y = self.eval_arg_i64(args, 1)? as i32;
                let fg = self.eval_arg_i64(args, 2)? as u8;
                let bg = self.eval_arg_i64(args, 3)? as u8;
                let s = self.eval_expr(&args[4])?;
                let Value::Str(text) = s else {
                    return Err("Text expects last arg to be a string literal".to_string());
                };
                self.rt.draw_text(x, y, fg, bg, &text);
                Ok(Value::Void)
            }
            "TextChar" => {
                if args.len() != 5 {
                    return Err("TextChar(task,raw_cursor,x,y,c) expects 5 args".to_string());
                }
                let col = self.eval_arg_i64(args, 2)? as i32;
                let row = self.eval_arg_i64(args, 3)? as i32;
                let c_v = self.eval_expr(&args[4])?;
                let bits = match c_v {
                    Value::Char(v) => v,
                    _ => c_v.as_i64()? as u64,
                };

                let ch = (bits & 0xFF) as u8;
                let fg = ((bits >> 8) & 0x0F) as u8;
                let bg = ((bits >> 12) & 0x0F) as u8;

                let (pan_x, pan_y) = match self.env.get("gr") {
                    Some(Value::Obj(gr)) => {
                        let gr = gr.borrow();
                        let px = gr
                            .fields
                            .get("pan_text_x")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(0) as i32;
                        let py = gr
                            .fields
                            .get("pan_text_y")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(0) as i32;
                        (px, py)
                    }
                    _ => (0, 0),
                };

                let x = col.saturating_mul(8).saturating_sub(pan_x);
                let y = row.saturating_mul(8).saturating_sub(pan_y);
                let ch = temple_rt::assets::decode_cp437_byte(ch);
                self.rt.draw_char_8x8(x, y, fg, bg, ch);
                Ok(Value::Void)
            }
            "Present" => {
                self.present_with_overlays()?;
                Ok(Value::Void)
            }
            "Refresh" => {
                self.poll_events()?;
                self.present_with_overlays()?;
                Ok(Value::Void)
            }
            "Yield" => {
                self.poll_events()?;
                self.present_with_overlays()?;
                Ok(Value::Void)
            }
            "Sleep" => {
                let ms = self.eval_arg_i64(args, 0)?;
                self.poll_events()?;
                self.present_with_overlays()?;
                if ms > 0 {
                    thread::sleep(Duration::from_millis(ms as u64));
                }
                Ok(Value::Void)
            }
            "Seed" => {
                let seed = match args.len() {
                    0 => 0u64,
                    1 if matches!(args[0], Expr::DefaultArg) => 0u64,
                    1 => self.eval_expr(&args[0])?.as_i64()? as u64,
                    _ => return Err("Seed(seed=0) expects 0 or 1 args".to_string()),
                };
                self.set_seed(seed);
                Ok(Value::Void)
            }
            "RandI16" => {
                if !args.is_empty() {
                    return Err("RandI16 expects 0 args".to_string());
                }
                Ok(Value::Int(self.rand_i16() as i64))
            }
            "RandU16" => {
                if !args.is_empty() {
                    return Err("RandU16 expects 0 args".to_string());
                }
                Ok(Value::Int((self.rand_next_u64() >> 48) as u16 as i64))
            }
            "Rand" => {
                if !args.is_empty() {
                    return Err("Rand expects 0 args".to_string());
                }
                // TempleOS Rand() returns an F64 in [0,1).
                let raw = self.rand_next_u64() >> 11; // 53 bits
                let denom = (1u64 << 53) as f64;
                Ok(Value::Float((raw as f64) / denom))
            }
            "SignI64" => {
                let v = self.eval_arg_i64(args, 0)?;
                let s = if v > 0 {
                    1
                } else if v < 0 {
                    -1
                } else {
                    0
                };
                Ok(Value::Int(s))
            }
            "ClampI64" => {
                if args.len() != 3 {
                    return Err("ClampI64(v, min, max) expects 3 args".to_string());
                }
                let v = self.eval_expr(&args[0])?.as_i64()?;
                let min_v = self.eval_expr(&args[1])?.as_i64()?;
                let max_v = self.eval_expr(&args[2])?.as_i64()?;
                Ok(Value::Int(v.clamp(min_v, max_v)))
            }
            "GetChar" => {
                let echo = match args.len() {
                    0 => true,
                    1 => true, // ignore _scan_code
                    2 => match &args[1] {
                        Expr::DefaultArg => true,
                        _ => self.eval_expr(&args[1])?.truthy(),
                    },
                    3 => match &args[1] {
                        Expr::DefaultArg => true,
                        _ => self.eval_expr(&args[1])?.truthy(),
                    },
                    _ => {
                        return Err(
                            "GetChar(_scan_code=NULL, echo=TRUE, raw_cursor=FALSE) expects 0-3 args"
                                .to_string(),
                        );
                    }
                };

                // Flush at least once before we block so the user sees the current frame.
                self.present_with_overlays()?;
                let mut last_present = std::time::Instant::now();

                loop {
                    self.poll_events()?;
                    if let Some(code) = self.key_queue.pop_front() {
                        if echo {
                            if let Ok(b) = u8::try_from(code) {
                                let ch = b as char;
                                if ch == '\n' || ch == '\t' || ch == ' ' || ch.is_ascii_graphic() {
                                    self.put_char(ch);
                                    self.present_with_overlays()?;
                                    last_present = std::time::Instant::now();
                                }
                            }
                        }
                        return Ok(Value::Int(code as i64));
                    }
                    if last_present.elapsed() >= Duration::from_millis(16) {
                        self.present_with_overlays()?;
                        last_present = std::time::Instant::now();
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
            "GetKey" => {
                if args.len() > 1 {
                    return Err("GetKey(_scan_code=NULL) expects 0-1 args".to_string());
                }

                let scan_ptr = match args.get(0) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => match self.eval_expr(expr)? {
                        Value::Int(0) => None,
                        other => Some(other),
                    },
                };

                fn write_i64(vm: &mut Vm, ptr: Value, value: i64) -> Result<(), String> {
                    match ptr {
                        Value::VarRef(name) => vm.env.assign(&name, Value::Int(value)),
                        Value::Ptr { addr, elem_bytes } => {
                            vm.heap_write_i64_le(addr, elem_bytes.max(1), value)
                        }
                        Value::ArrayPtr { arr, index } => {
                            let idx: usize = index
                                .try_into()
                                .map_err(|_| "GetKey: negative array pointer".to_string())?;
                            let mut arr = arr.borrow_mut();
                            if idx >= arr.elems.len() {
                                return Err("GetKey: array pointer out of range".to_string());
                            }
                            arr.elems[idx] = Value::Int(value);
                            Ok(())
                        }
                        Value::ObjFieldRef { obj, field } => {
                            obj.borrow_mut().fields.insert(field, Value::Int(value));
                            Ok(())
                        }
                        Value::Int(addr) => vm.heap_write_i64_le(addr, 8, value),
                        other => Err(format!("GetKey: unsupported pointer: {other:?}")),
                    }
                }

                // Flush at least once before we block so the user sees the current frame.
                self.present_with_overlays()?;
                let mut last_present = std::time::Instant::now();

                loop {
                    self.poll_events()?;
                    if let Some(code) = self.key_queue.pop_front() {
                        let mut ascii = 0i64;
                        let mut sc = 0i64;

                        if code <= 0xFF {
                            ascii = code as i64;
                        } else {
                            sc = match code {
                                protocol::KEY_LEFT => 0x4B,
                                protocol::KEY_RIGHT => 0x4D,
                                protocol::KEY_UP => 0x48,
                                protocol::KEY_DOWN => 0x50,
                                protocol::KEY_HOME => 0x47,
                                protocol::KEY_END => 0x4F,
                                protocol::KEY_PAGE_UP => 0x49,
                                protocol::KEY_PAGE_DOWN => 0x51,
                                protocol::KEY_INSERT => 0x52,
                                protocol::KEY_DELETE => 0x53,
                                _ => 0,
                            };
                        }

                        if let Some(ptr) = scan_ptr.clone() {
                            write_i64(self, ptr, sc)?;
                        }
                        return Ok(Value::Int(ascii));
                    }
                    if last_present.elapsed() >= Duration::from_millis(16) {
                        self.present_with_overlays()?;
                        last_present = std::time::Instant::now();
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
            "ScanMsg" => {
                if args.len() > 4 {
                    return Err(
                        "ScanMsg(_arg1=NULL,_arg2=NULL,mask=~1,task=NULL) expects 0-4 args"
                            .to_string(),
                    );
                }

                let arg1_ptr = match args.get(0) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => match self.eval_expr(expr)? {
                        Value::VarRef(name) => Some(name),
                        Value::Int(0) => None,
                        _ => return Err("ScanMsg: _arg1 must be &var or NULL".to_string()),
                    },
                };
                let arg2_ptr = match args.get(1) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => match self.eval_expr(expr)? {
                        Value::VarRef(name) => Some(name),
                        Value::Int(0) => None,
                        _ => return Err("ScanMsg: _arg2 must be &var or NULL".to_string()),
                    },
                };

                let mask_i64 = match args.get(2) {
                    None | Some(Expr::DefaultArg) => !1i64,
                    Some(expr) => self.eval_expr(expr)?.as_i64()?,
                };
                let mask = mask_i64 as u64;

                self.poll_events()?;
                if let Some(msg) = self.scan_msg_mask(mask) {
                    if let Some(name) = arg1_ptr {
                        self.env.assign(&name, Value::Int(msg.arg1))?;
                    }
                    if let Some(name) = arg2_ptr {
                        self.env.assign(&name, Value::Int(msg.arg2))?;
                    }
                    Ok(Value::Int(msg.code))
                } else {
                    if let Some(name) = arg1_ptr {
                        self.env.assign(&name, Value::Int(0))?;
                    }
                    if let Some(name) = arg2_ptr {
                        self.env.assign(&name, Value::Int(0))?;
                    }
                    Ok(Value::Int(0))
                }
            }
            "GetMsg" => {
                if args.len() > 4 {
                    return Err(
                        "GetMsg(_arg1=NULL,_arg2=NULL,mask=~1,task=NULL) expects 0-4 args"
                            .to_string(),
                    );
                }

                let arg1_ptr = match args.get(0) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => match self.eval_expr(expr)? {
                        Value::VarRef(name) => Some(name),
                        Value::Int(0) => None,
                        _ => return Err("GetMsg: _arg1 must be &var or NULL".to_string()),
                    },
                };
                let arg2_ptr = match args.get(1) {
                    None | Some(Expr::DefaultArg) => None,
                    Some(expr) => match self.eval_expr(expr)? {
                        Value::VarRef(name) => Some(name),
                        Value::Int(0) => None,
                        _ => return Err("GetMsg: _arg2 must be &var or NULL".to_string()),
                    },
                };

                let mask_i64 = match args.get(2) {
                    None | Some(Expr::DefaultArg) => !1i64,
                    Some(expr) => self.eval_expr(expr)?.as_i64()?,
                };
                let mask = mask_i64 as u64;

                loop {
                    self.poll_events()?;
                    if let Some(msg) = self.scan_msg_mask(mask) {
                        if let Some(name) = arg1_ptr.as_ref() {
                            self.env.assign(name, Value::Int(msg.arg1))?;
                        }
                        if let Some(name) = arg2_ptr.as_ref() {
                            self.env.assign(name, Value::Int(msg.arg2))?;
                        }
                        return Ok(Value::Int(msg.code));
                    }
                    let _ = self.call("Yield", &[])?;
                    thread::sleep(Duration::from_millis(1));
                }
            }
            "MenuPush" => {
                if args.len() != 1 {
                    return Err("MenuPush(\"spec\") expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                let Value::Str(spec) = v else {
                    return Err("MenuPush expects a string".to_string());
                };
                self.menu_push(&spec)?;
                Ok(Value::Void)
            }
            "MenuPop" => {
                if !args.is_empty() {
                    return Err("MenuPop expects 0 args".to_string());
                }
                self.menu_pop()?;
                Ok(Value::Void)
            }
            "MenuEntryFind" => {
                if args.len() != 2 {
                    return Err("MenuEntryFind(menu, \"path\") expects 2 args".to_string());
                }
                let menu_v = match &args[0] {
                    Expr::DefaultArg => {
                        let fs = self
                            .env
                            .get("Fs")
                            .ok_or_else(|| "missing Fs global".to_string())?;
                        self.get_field(fs, "cur_menu")?
                    }
                    _ => self.eval_expr(&args[0])?,
                };
                let root = match menu_v {
                    Value::Obj(o) => o,
                    Value::Int(0) => return Ok(Value::Int(0)),
                    _ => return Err("MenuEntryFind: menu must be an object or NULL".to_string()),
                };

                let path_v = self.eval_expr(&args[1])?;
                let Value::Str(path) = path_v else {
                    return Err("MenuEntryFind: path must be a string".to_string());
                };

                for menu in self.menu_stack.iter().rev() {
                    if Rc::ptr_eq(&menu.root, &root) {
                        if let Some(entry) = menu.entries_by_path.get(&path) {
                            return Ok(Value::Obj(entry.clone()));
                        }
                        return Ok(Value::Int(0));
                    }
                }

                Ok(Value::Int(0))
            }
            "Snd" => {
                let ona = match args.len() {
                    0 => 0i64,
                    1 if matches!(args[0], Expr::DefaultArg) => 0i64,
                    1 => self.eval_expr(&args[0])?.as_i64()?,
                    _ => return Err("Snd(ona=0) expects 0 or 1 args".to_string()),
                };
                let ona = ona.clamp(i8::MIN as i64, i8::MAX as i64) as i8;
                self.rt.snd(ona).map_err(|e| e.to_string())?;
                Ok(Value::Void)
            }
            "SndRst" => {
                self.rt.snd(0).map_err(|e| e.to_string())?;
                Ok(Value::Void)
            }
            "Beep" => {
                let ona = match args.len() {
                    0 => 62i64,
                    1 if matches!(args[0], Expr::DefaultArg) => 62i64,
                    1 => self.eval_expr(&args[0])?.as_i64()?,
                    2 if matches!(args[0], Expr::DefaultArg) => 62i64,
                    2 => self.eval_expr(&args[0])?.as_i64()?,
                    _ => return Err("Beep(ona=62, busy=FALSE) expects 0-2 args".to_string()),
                };
                let _busy = match args.len() {
                    2 if matches!(args[1], Expr::DefaultArg) => false,
                    2 => self.eval_expr(&args[1])?.truthy(),
                    _ => false,
                };

                let ona = ona.clamp(i8::MIN as i64, i8::MAX as i64) as i8;
                self.rt.snd(ona).map_err(|e| e.to_string())?;
                self.poll_events()?;
                thread::sleep(Duration::from_millis(500));
                self.rt.snd(0).map_err(|e| e.to_string())?;
                self.poll_events()?;
                thread::sleep(Duration::from_millis(200));
                Ok(Value::Void)
            }
            "Mute" => {
                let val = self.eval_arg_i64(args, 0)? != 0;
                let old = self.is_mute;
                self.is_mute = val;
                if val {
                    let _ = self.rt.snd(0);
                }
                self.rt.mute(val).map_err(|e| e.to_string())?;
                Ok(Value::Int(old as i64))
            }
            "IsMute" => Ok(Value::Int(self.is_mute as i64)),
            "Ona2Freq" => {
                let ona = self.eval_arg_i64(args, 0)?;
                if ona == 0 {
                    return Ok(Value::Float(0.0));
                }
                let freq = 440.0 / 32.0 * 2.0_f64.powf(ona as f64 / 12.0);
                Ok(Value::Float(freq))
            }
            "Freq2Ona" => {
                let freq = self.eval_arg_f64(args, 0)?;
                if freq <= 0.0 {
                    return Ok(Value::Int(0));
                }
                let raw = 12.0 * ((32.0 / 440.0) * freq).log2();
                let mut ona = raw as i64;
                ona = ona.clamp(1, i8::MAX as i64);
                Ok(Value::Int(ona))
            }
            "NextKey" => {
                self.poll_events()?;
                Ok(Value::Int(self.key_queue.pop_front().unwrap_or(0) as i64))
            }
            _ => Err(format!(
                "internal: call_builtin_ui_input_sound cannot handle {name}"
            )),
        }
    }
}
