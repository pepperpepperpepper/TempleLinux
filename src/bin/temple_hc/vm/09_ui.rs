use super::prelude::*;

use super::{
    EnvScopeGuard, MenuAction, MenuGroup, MenuItem, MenuState, MenuUnderlay, Obj, ObjRef,
    TempleMsg, Value, Vm, VmPanic,
};

impl Vm {
    fn ctrl_find_at(&self, task: &ObjRef, x: i64, y: i64) -> Option<ObjRef> {
        const CTRLF_SHOW: i64 = 1;

        let head = match task.borrow().fields.get("last_ctrl").cloned() {
            Some(Value::Obj(head)) => head,
            _ => return None,
        };
        let mut cur = match head.borrow().fields.get("next").cloned() {
            Some(Value::Obj(cur)) => cur,
            _ => return None,
        };

        let mut steps = 0usize;
        while !Rc::ptr_eq(&cur, &head) {
            steps += 1;
            if steps > 4096 {
                break;
            }

            let (next, flags, left, right, top, bottom) = {
                let cur_b = cur.borrow();
                let next = cur_b.fields.get("next").cloned();
                let flags = cur_b
                    .fields
                    .get("flags")
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                let left = cur_b
                    .fields
                    .get("left")
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                let right = cur_b
                    .fields
                    .get("right")
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                let top = cur_b
                    .fields
                    .get("top")
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                let bottom = cur_b
                    .fields
                    .get("bottom")
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                (next, flags, left, right, top, bottom)
            };

            if (flags & CTRLF_SHOW) != 0 && x >= left && x <= right && y >= top && y <= bottom {
                return Some(cur);
            }

            cur = match next {
                Some(Value::Obj(next)) => next,
                _ => break,
            };
        }

        None
    }

    fn ctrl_call_left_click(
        &mut self,
        ctrl: &ObjRef,
        x: i64,
        y: i64,
        down: bool,
    ) -> Result<(), String> {
        let fp = ctrl.borrow().fields.get("left_click").cloned();
        let Some(Value::FuncRef(name)) = fp else {
            return Ok(());
        };

        let _scope = EnvScopeGuard::new(&mut self.env);
        self.env
            .define("__tl_ctrl".to_string(), Value::Obj(ctrl.clone()));
        self.env.define("__tl_x".to_string(), Value::Int(x));
        self.env.define("__tl_y".to_string(), Value::Int(y));
        self.env
            .define("__tl_down".to_string(), Value::Int(down as i64));
        let args = [
            Expr::Var("__tl_ctrl".to_string()),
            Expr::Var("__tl_x".to_string()),
            Expr::Var("__tl_y".to_string()),
            Expr::Var("__tl_down".to_string()),
        ];
        let _ = self.call(&name, &args)?;
        Ok(())
    }

    fn ctrl_handle_left_button(&mut self, down: bool, x: i64, y: i64) -> Result<(), String> {
        const CTRLF_CAPTURE_LEFT_MS: i64 = 4;

        let Some(Value::Obj(fs)) = self.env.get("Fs") else {
            return Ok(());
        };

        if down {
            self.ctrl_capture_left = None;

            if let Some(ctrl) = self.ctrl_find_at(&fs, x, y) {
                let flags = ctrl
                    .borrow()
                    .fields
                    .get("flags")
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                self.ctrl_call_left_click(&ctrl, x, y, true)?;
                if (flags & CTRLF_CAPTURE_LEFT_MS) != 0 {
                    self.ctrl_capture_left = Some(ctrl);
                }
            }
        } else if let Some(ctrl) = self.ctrl_capture_left.take() {
            self.ctrl_call_left_click(&ctrl, x, y, false)?;
        } else if let Some(ctrl) = self.ctrl_find_at(&fs, x, y) {
            self.ctrl_call_left_click(&ctrl, x, y, false)?;
        }

        Ok(())
    }

    pub(super) fn poll_events(&mut self) -> Result<(), String> {
        while let Some(ev) = self.rt.try_next_event() {
            match ev {
                Event::Key { code, down } => {
                    match code {
                        protocol::KEY_SHIFT => self.shift_down = down,
                        protocol::KEY_CONTROL => self.ctrl_down = down,
                        protocol::KEY_ALT => self.alt_down = down,
                        _ => {}
                    }

                    // TempleOS convention: Ctrl+Alt+C aborts the current task (often caught by try/catch).
                    if down
                        && self.ctrl_down
                        && self.alt_down
                        && (code == b'c' as u32 || code == b'C' as u32)
                    {
                        std::panic::panic_any(VmPanic::Throw);
                    }

                    let msg = self.map_key_event_to_msg(code, down);
                    self.msg_queue.push_back(msg);

                    if down
                        && !matches!(
                            code,
                            protocol::KEY_SHIFT | protocol::KEY_CONTROL | protocol::KEY_ALT
                        )
                    {
                        let mapped = self.map_key_code(code);
                        self.scan_char = mapped;
                        self.key_queue.push_back(mapped);
                    }
                }
                Event::MouseMove { x, y } => {
                    let (x, y) = {
                        let mut x = x as i64;
                        let mut y = y as i64;

                        if let Some(Value::Obj(grid)) = self.env.get("ms_grid") {
                            let grid = grid.borrow();
                            let snap = grid.fields.get("snap").is_some_and(|v| v.truthy());
                            if snap {
                                let gx = grid
                                    .fields
                                    .get("x")
                                    .and_then(|v| v.as_i64().ok())
                                    .unwrap_or(0);
                                let gy = grid
                                    .fields
                                    .get("y")
                                    .and_then(|v| v.as_i64().ok())
                                    .unwrap_or(0);

                                if gx > 0 {
                                    x = x - (x % gx);
                                }
                                if gy > 0 {
                                    y = y - (y % gy);
                                }
                            }
                        }

                        (x, y)
                    };

                    {
                        let mut pos = self.ms_pos.borrow_mut();
                        pos.fields.insert("x".to_string(), Value::Int(x));
                        pos.fields.insert("y".to_string(), Value::Int(y));
                    }
                    self.msg_queue.push_back(TempleMsg {
                        code: 4, // MSG_MS_MOVE
                        arg1: x,
                        arg2: y,
                    });
                    self.menu_update_hover(x as i32, y as i32);

                    if let Some(ctrl) = self.ctrl_capture_left.clone() {
                        let is_down = self
                            .ms
                            .borrow()
                            .fields
                            .get("lb")
                            .is_some_and(|v| v.truthy());
                        if is_down {
                            self.ctrl_call_left_click(&ctrl, x, y, true)?;
                        }
                    }
                }
                Event::MouseButton { button, down } => {
                    let (x, y) = {
                        let pos = self.ms_pos.borrow();
                        let x = pos
                            .fields
                            .get("x")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(0);
                        let y = pos
                            .fields
                            .get("y")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(0);
                        (x, y)
                    };

                    if button == protocol::MOUSE_BUTTON_LEFT {
                        self.ms
                            .borrow_mut()
                            .fields
                            .insert("lb".to_string(), Value::Int(down as i64));
                        self.msg_queue.push_back(TempleMsg {
                            code: if down { 5 } else { 6 }, // MSG_MS_L_DOWN / MSG_MS_L_UP
                            arg1: x,
                            arg2: y,
                        });
                        if down {
                            self.menu_handle_left_click(x as i32, y as i32);
                        }
                        self.ctrl_handle_left_button(down, x, y)?;
                    } else if button == protocol::MOUSE_BUTTON_RIGHT {
                        self.msg_queue.push_back(TempleMsg {
                            code: if down { 9 } else { 10 }, // MSG_MS_R_DOWN / MSG_MS_R_UP
                            arg1: x,
                            arg2: y,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn map_key_event_to_msg(&self, code: u32, down: bool) -> TempleMsg {
        // TempleOS message + scan code conventions (subset).
        const MSG_KEY_DOWN: i64 = 2;
        const MSG_KEY_UP: i64 = 3;

        const CH_BACKSPACE: i64 = 0x08;
        const CH_ESC: i64 = 0x1B;
        const CH_SHIFT_ESC: i64 = 0x1C;

        const SC_ESC: i64 = 0x01;
        const SC_BACKSPACE: i64 = 0x0E;
        const SC_TAB: i64 = 0x0F;
        const SC_ENTER: i64 = 0x1C;
        const SC_SHIFT: i64 = 0x2A;
        const SC_CTRL: i64 = 0x1D;
        const SC_ALT: i64 = 0x38;
        const SC_CURSOR_UP: i64 = 0x48;
        const SC_CURSOR_DOWN: i64 = 0x50;
        const SC_CURSOR_LEFT: i64 = 0x4B;
        const SC_CURSOR_RIGHT: i64 = 0x4D;
        const SC_PAGE_UP: i64 = 0x49;
        const SC_PAGE_DOWN: i64 = 0x51;
        const SC_HOME: i64 = 0x47;
        const SC_END: i64 = 0x4F;
        const SC_INS: i64 = 0x52;
        const SC_DELETE: i64 = 0x53;
        const SC_F1: i64 = 0x3B;
        const SC_F2: i64 = 0x3C;
        const SC_F3: i64 = 0x3D;
        const SC_F4: i64 = 0x3E;
        const SC_F5: i64 = 0x3F;
        const SC_F6: i64 = 0x40;
        const SC_F7: i64 = 0x41;
        const SC_F8: i64 = 0x42;
        const SC_F9: i64 = 0x43;
        const SC_F10: i64 = 0x44;
        const SC_F11: i64 = 0x57;
        const SC_F12: i64 = 0x58;

        const SCF_KEY_UP: i64 = 0x100;
        const SCF_SHIFT: i64 = 0x200;
        const SCF_CTRL: i64 = 0x400;
        const SCF_ALT: i64 = 0x800;
        const SCF_DELETE: i64 = 0x40000;
        const SCF_INS: i64 = 0x80000;

        let mut flags = 0i64;
        if !down {
            flags |= SCF_KEY_UP;
        }
        if self.shift_down {
            flags |= SCF_SHIFT;
        }
        if self.ctrl_down {
            flags |= SCF_CTRL;
        }
        if self.alt_down {
            flags |= SCF_ALT;
        }

        let msg_code = if down { MSG_KEY_DOWN } else { MSG_KEY_UP };

        let (ascii, scancode, extra_flags) = match code {
            protocol::KEY_ESCAPE => (
                if self.shift_down {
                    CH_SHIFT_ESC
                } else {
                    CH_ESC
                },
                SC_ESC,
                0,
            ),
            protocol::KEY_ENTER => (b'\n' as i64, SC_ENTER, 0),
            protocol::KEY_BACKSPACE => (CH_BACKSPACE, SC_BACKSPACE, 0),
            protocol::KEY_TAB => (b'\t' as i64, SC_TAB, 0),
            protocol::KEY_SHIFT => (0, SC_SHIFT, 0),
            protocol::KEY_CONTROL => (0, SC_CTRL, 0),
            protocol::KEY_ALT => (0, SC_ALT, 0),
            protocol::KEY_LEFT => (0, SC_CURSOR_LEFT, 0),
            protocol::KEY_RIGHT => (0, SC_CURSOR_RIGHT, 0),
            protocol::KEY_UP => (0, SC_CURSOR_UP, 0),
            protocol::KEY_DOWN => (0, SC_CURSOR_DOWN, 0),
            protocol::KEY_HOME => (0, SC_HOME, 0),
            protocol::KEY_END => (0, SC_END, 0),
            protocol::KEY_PAGE_UP => (0, SC_PAGE_UP, 0),
            protocol::KEY_PAGE_DOWN => (0, SC_PAGE_DOWN, 0),
            protocol::KEY_INSERT => (0, SC_INS, SCF_INS),
            protocol::KEY_DELETE => (0, SC_DELETE, SCF_DELETE),
            protocol::KEY_F1 => (0, SC_F1, 0),
            protocol::KEY_F2 => (0, SC_F2, 0),
            protocol::KEY_F3 => (0, SC_F3, 0),
            protocol::KEY_F4 => (0, SC_F4, 0),
            protocol::KEY_F5 => (0, SC_F5, 0),
            protocol::KEY_F6 => (0, SC_F6, 0),
            protocol::KEY_F7 => (0, SC_F7, 0),
            protocol::KEY_F8 => (0, SC_F8, 0),
            protocol::KEY_F9 => (0, SC_F9, 0),
            protocol::KEY_F10 => (0, SC_F10, 0),
            protocol::KEY_F11 => (0, SC_F11, 0),
            protocol::KEY_F12 => (0, SC_F12, 0),
            _ => {
                if let Ok(b) = u8::try_from(code) {
                    if self.ctrl_down && b.is_ascii_alphabetic() {
                        ((b.to_ascii_uppercase() & 0x1F) as i64, 0, 0)
                    } else {
                        (b as i64, 0, 0)
                    }
                } else {
                    (0, 0, 0)
                }
            }
        };

        TempleMsg {
            code: msg_code,
            arg1: ascii,
            arg2: scancode | flags | extra_flags,
        }
    }

    pub(super) fn scan_msg_mask(&mut self, mask: u64) -> Option<TempleMsg> {
        while let Some(msg) = self.msg_queue.pop_front() {
            if msg.code < 0 {
                continue;
            }
            let Some(bit) = 1u64.checked_shl(msg.code as u32) else {
                continue;
            };
            if (mask & bit) != 0 {
                return Some(msg);
            }
        }
        None
    }

    fn eval_int_expr_str(&mut self, expr: &str) -> Result<i64, String> {
        let mut lex = Lexer::new("<expr>".into(), expr.as_bytes(), 1, self.macros.clone());
        let mut tokens = Vec::new();
        loop {
            let t = lex.next_token().map_err(|e| e.to_string())?;
            let is_eof = matches!(t.kind, TokenKind::Eof);
            tokens.push(t);
            if is_eof {
                break;
            }
        }

        let expr = Parser::parse_expr_only(tokens).map_err(|e| e.to_string())?;
        self.eval_expr(&expr)?.as_i64()
    }

    fn set_fs_cur_menu(&mut self, menu: Option<ObjRef>) -> Result<(), String> {
        let fs = self
            .env
            .get("Fs")
            .ok_or_else(|| "missing Fs global".to_string())?;
        let v = match menu {
            Some(m) => Value::Obj(m),
            None => Value::Int(0),
        };
        self.set_field(fs, "cur_menu", v)?;
        Ok(())
    }

    fn parse_menu_spec(&mut self, spec: &str) -> Result<MenuState, String> {
        fn skip_ws(bytes: &[u8], idx: &mut usize) {
            while *idx < bytes.len() && bytes[*idx].is_ascii_whitespace() {
                *idx += 1;
            }
        }

        fn parse_ident(bytes: &[u8], idx: &mut usize) -> Result<String, String> {
            skip_ws(bytes, idx);
            let start = *idx;
            while *idx < bytes.len()
                && ((bytes[*idx] as char).is_ascii_alphanumeric() || bytes[*idx] == b'_')
            {
                *idx += 1;
            }
            if *idx == start {
                return Err("expected identifier in menu spec".to_string());
            }
            Ok(String::from_utf8_lossy(&bytes[start..*idx]).to_string())
        }

        fn split_args(arg_src: &str) -> Vec<String> {
            let mut out = Vec::new();
            let mut cur = String::new();
            let mut in_single = false;
            let mut in_double = false;
            let mut escaped = false;
            for ch in arg_src.chars() {
                if escaped {
                    cur.push(ch);
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    cur.push(ch);
                    escaped = true;
                    continue;
                }
                if !in_double && ch == '\'' {
                    in_single = !in_single;
                    cur.push(ch);
                    continue;
                }
                if !in_single && ch == '"' {
                    in_double = !in_double;
                    cur.push(ch);
                    continue;
                }
                if !in_single && !in_double && ch == ',' {
                    out.push(cur.trim().to_string());
                    cur.clear();
                    continue;
                }
                cur.push(ch);
            }
            out.push(cur.trim().to_string());
            out
        }

        let bytes = spec.as_bytes();
        let mut idx = 0usize;
        let mut groups: Vec<MenuGroup> = Vec::new();
        let mut entries_by_path: HashMap<String, ObjRef> = HashMap::new();
        let root = Rc::new(RefCell::new(Obj {
            fields: HashMap::new(),
        }));

        while idx < bytes.len() {
            skip_ws(bytes, &mut idx);
            if idx >= bytes.len() {
                break;
            }

            let group_name = parse_ident(bytes, &mut idx)?;
            skip_ws(bytes, &mut idx);
            if idx >= bytes.len() || bytes[idx] != b'{' {
                return Err(format!("expected '{{' after menu group '{group_name}'"));
            }
            idx += 1; // {

            let mut items: Vec<MenuItem> = Vec::new();
            loop {
                skip_ws(bytes, &mut idx);
                if idx >= bytes.len() {
                    return Err(format!("unterminated menu group '{group_name}'"));
                }
                if bytes[idx] == b'}' {
                    idx += 1; // }
                    break;
                }

                let item_name = parse_ident(bytes, &mut idx)?;
                skip_ws(bytes, &mut idx);
                if idx >= bytes.len() || bytes[idx] != b'(' {
                    return Err(format!("expected '(' after menu item '{item_name}'"));
                }
                idx += 1; // (

                let args_start = idx;
                let mut depth = 1i32;
                let mut in_single = false;
                let mut in_double = false;
                let mut escaped = false;
                while idx < bytes.len() {
                    let b = bytes[idx];
                    if escaped {
                        escaped = false;
                        idx += 1;
                        continue;
                    }
                    if b == b'\\' {
                        escaped = true;
                        idx += 1;
                        continue;
                    }
                    if !in_double && b == b'\'' {
                        in_single = !in_single;
                        idx += 1;
                        continue;
                    }
                    if !in_single && b == b'"' {
                        in_double = !in_double;
                        idx += 1;
                        continue;
                    }
                    if !in_single && !in_double {
                        if b == b'(' {
                            depth += 1;
                        } else if b == b')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    idx += 1;
                }
                if idx >= bytes.len() || bytes[idx] != b')' {
                    return Err(format!("unterminated args for menu item '{item_name}'"));
                }

                let args_src = &spec[args_start..idx];
                idx += 1; // )

                skip_ws(bytes, &mut idx);
                if idx < bytes.len() && bytes[idx] == b';' {
                    idx += 1;
                }

                let args = split_args(args_src);
                let action = self.menu_action_from_args(&args)?;

                let entry = Rc::new(RefCell::new(Obj {
                    fields: HashMap::from([("checked".to_string(), Value::Int(0))]),
                }));

                let path = format!("{group_name}/{item_name}");
                entries_by_path.insert(path.clone(), entry.clone());
                items.push(MenuItem {
                    name: item_name,
                    path,
                    entry,
                    action,
                });
            }

            groups.push(MenuGroup {
                name: group_name,
                items,
            });
        }

        Ok(MenuState {
            root,
            groups,
            entries_by_path,
            open_group: None,
            hover_item: None,
            underlay: None,
        })
    }

    fn menu_action_from_args(&mut self, args: &[String]) -> Result<MenuAction, String> {
        let a0 = args.get(0).map(|s| s.trim()).unwrap_or("");
        let a1 = args.get(1).map(|s| s.trim()).unwrap_or("");
        let a2 = args.get(2).map(|s| s.trim()).unwrap_or("");

        if !a0.is_empty() {
            let code = self.eval_int_expr_str(a0)?;
            if code == 1 {
                let arg1 = if a1.is_empty() {
                    0
                } else {
                    self.eval_int_expr_str(a1)?
                };
                let arg2 = if a2.is_empty() {
                    0
                } else {
                    self.eval_int_expr_str(a2)?
                };
                return Ok(MenuAction::MsgCmd { arg1, arg2 });
            }
        }

        if a0.is_empty() && a1.is_empty() && !a2.is_empty() {
            let arg2 = self.eval_int_expr_str(a2)?;
            return Ok(MenuAction::KeyScan { arg2 });
        }

        if a0.is_empty() && !a1.is_empty() {
            let ascii = self.eval_int_expr_str(a1)?;
            return Ok(MenuAction::KeyAscii { ascii });
        }

        Ok(MenuAction::None)
    }

    pub(super) fn menu_push(&mut self, spec: &str) -> Result<(), String> {
        let state = self.parse_menu_spec(spec)?;
        let root = state.root.clone();
        self.menu_stack.push(state);
        self.set_fs_cur_menu(Some(root))?;
        Ok(())
    }

    pub(super) fn menu_pop(&mut self) -> Result<(), String> {
        if let Some(menu) = self.menu_stack.last_mut() {
            Self::menu_set_open_group(&mut self.rt, menu, None);
        }

        self.menu_stack.pop();
        let next = self.menu_stack.last().map(|m| m.root.clone());
        self.set_fs_cur_menu(next)?;
        Ok(())
    }

    fn menu_item_label(item: &MenuItem) -> String {
        let checked = item
            .entry
            .borrow()
            .fields
            .get("checked")
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(0)
            != 0;
        if checked {
            format!("[x] {}", item.name)
        } else {
            format!("    {}", item.name)
        }
    }

    fn menu_bar_hit(menu: &MenuState, x: i32) -> Option<usize> {
        if x < 0 {
            return None;
        }
        let mut cur_x = 0i32;
        for (i, g) in menu.groups.iter().enumerate() {
            let label = format!(" {} ", g.name);
            let w = label.chars().count() as i32 * 8;
            let x0 = cur_x;
            let x1 = cur_x + w;
            if x >= x0 && x < x1 {
                return Some(i);
            }
            cur_x = x1;
        }
        None
    }

    fn menu_dropdown_rect(menu: &MenuState, group_idx: usize) -> Option<(i32, i32, i32, i32)> {
        let group = menu.groups.get(group_idx)?;
        let mut bar_x = 0i32;
        for (i, g) in menu.groups.iter().enumerate() {
            let label = format!(" {} ", g.name);
            let w = label.chars().count() as i32 * 8;
            if i == group_idx {
                break;
            }
            bar_x += w;
        }

        let max_chars = group
            .items
            .iter()
            .map(|it| Self::menu_item_label(it).chars().count())
            .max()
            .unwrap_or(0) as i32;

        let w = (max_chars.max(1) + 1) * 8;
        let h = group.items.len() as i32 * 8;
        Some((bar_x, 8, w, h))
    }

    fn menu_dropdown_hit(menu: &MenuState, group_idx: usize, x: i32, y: i32) -> Option<usize> {
        let (x0, y0, w, h) = Self::menu_dropdown_rect(menu, group_idx)?;
        if x < x0 || y < y0 || x >= x0 + w || y >= y0 + h {
            return None;
        }
        let idx = ((y - y0) / 8) as usize;
        let group = menu.groups.get(group_idx)?;
        if idx >= group.items.len() {
            return None;
        }
        Some(idx)
    }

    pub(super) fn menu_capture_underlay(
        rt: &mut TempleRt,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Option<MenuUnderlay> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let (sw, sh) = rt.size();
        let sw = sw as i32;
        let sh = sh as i32;
        if x >= sw || y >= sh || x + w <= 0 || y + h <= 0 {
            return None;
        }

        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(sw);
        let y1 = (y + h).min(sh);
        let w = x1 - x0;
        let h = y1 - y0;

        let fb = rt.framebuffer_mut();
        let mut pixels = Vec::with_capacity((w * h) as usize);
        for yy in 0..h {
            let row = (y0 + yy) * sw;
            let start = (row + x0) as usize;
            let end = (row + x0 + w) as usize;
            pixels.extend_from_slice(&fb[start..end]);
        }

        Some(MenuUnderlay {
            x: x0,
            y: y0,
            w,
            h,
            pixels,
        })
    }

    pub(super) fn menu_restore_underlay(rt: &mut TempleRt, underlay: &MenuUnderlay) {
        let (sw, _sh) = rt.size();
        let sw = sw as i32;
        let fb = rt.framebuffer_mut();

        let mut idx = 0usize;
        for yy in 0..underlay.h {
            let row = (underlay.y + yy) * sw;
            let start = (row + underlay.x) as usize;
            let end = (row + underlay.x + underlay.w) as usize;
            if idx >= underlay.pixels.len() {
                break;
            }
            let take = (end - start).min(underlay.pixels.len() - idx);
            fb[start..start + take].copy_from_slice(&underlay.pixels[idx..idx + take]);
            idx += take;
        }
    }

    fn menu_set_open_group(rt: &mut TempleRt, menu: &mut MenuState, new: Option<usize>) {
        if menu.open_group == new {
            return;
        }

        if let Some(old) = menu.underlay.take() {
            Self::menu_restore_underlay(rt, &old);
        }

        menu.open_group = new;
        menu.hover_item = None;

        if let Some(g) = menu.open_group {
            if let Some((x, y, w, h)) = Self::menu_dropdown_rect(menu, g) {
                menu.underlay = Self::menu_capture_underlay(rt, x, y, w, h);
            }
        }
    }

    fn menu_update_hover(&mut self, x: i32, y: i32) {
        let Some(menu) = self.menu_stack.last_mut() else {
            return;
        };

        let desired_open = if y < 8 {
            Self::menu_bar_hit(menu, x)
        } else if let Some(g) = menu.open_group {
            if Self::menu_dropdown_hit(menu, g, x, y).is_some() {
                Some(g)
            } else {
                None
            }
        } else {
            None
        };

        if desired_open != menu.open_group {
            Self::menu_set_open_group(&mut self.rt, menu, desired_open);
        }

        if let Some(g) = menu.open_group {
            menu.hover_item = Self::menu_dropdown_hit(menu, g, x, y);
        } else {
            menu.hover_item = None;
        }
    }

    fn menu_handle_left_click(&mut self, x: i32, y: i32) {
        self.menu_update_hover(x, y);
        let Some(menu) = self.menu_stack.last_mut() else {
            return;
        };
        let Some(g) = menu.open_group else {
            return;
        };
        let Some(i) = menu.hover_item else {
            return;
        };

        if let Some(item) = menu.groups.get(g).and_then(|g| g.items.get(i)) {
            match item.action {
                MenuAction::None => {}
                MenuAction::MsgCmd { arg1, arg2 } => {
                    self.msg_queue.push_back(TempleMsg {
                        code: 1, // MSG_CMD
                        arg1,
                        arg2,
                    });
                }
                MenuAction::KeyAscii { ascii } => {
                    self.msg_queue.push_back(TempleMsg {
                        code: 2, // MSG_KEY_DOWN
                        arg1: ascii,
                        arg2: 0,
                    });
                }
                MenuAction::KeyScan { arg2 } => {
                    self.msg_queue.push_back(TempleMsg {
                        code: 2, // MSG_KEY_DOWN
                        arg1: 0,
                        arg2,
                    });
                }
            }
        }

        Self::menu_set_open_group(&mut self.rt, menu, None);
    }

    fn render_menu_overlay(&mut self) {
        let (mx, my) = {
            let pos = self.ms_pos.borrow();
            let x = pos
                .fields
                .get("x")
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0) as i32;
            let y = pos
                .fields
                .get("y")
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0) as i32;
            (x, y)
        };

        self.menu_update_hover(mx, my);

        let Some(menu) = self.menu_stack.last() else {
            return;
        };

        let (w, _h) = self.rt.size();
        let w = w as i32;

        // Bar.
        let bar_bg = 1u8; // BLUE
        let bar_fg = 15u8; // WHITE
        let active_bg = 3u8; // CYAN
        let active_fg = 0u8; // BLACK

        self.rt.fill_rect(0, 0, w, 8, bar_bg);

        let mut cur_x = 0i32;
        for (i, g) in menu.groups.iter().enumerate() {
            let label = format!(" {} ", g.name);
            let is_active = menu.open_group == Some(i);
            let fg = if is_active { active_fg } else { bar_fg };
            let bg = if is_active { active_bg } else { bar_bg };
            self.rt.draw_text(cur_x, 0, fg, bg, &label);
            cur_x += label.chars().count() as i32 * 8;
        }

        // Drop-down.
        if let Some(g) = menu.open_group {
            if let Some((x0, y0, ww, hh)) = Self::menu_dropdown_rect(menu, g) {
                let bg = 7u8; // LGRAY
                let fg = 0u8; // BLACK
                let hl_bg = 9u8; // LTBLUE
                let hl_fg = 0u8; // BLACK

                self.rt.fill_rect(x0, y0, ww, hh, bg);

                if let Some(group) = menu.groups.get(g) {
                    for (idx, item) in group.items.iter().enumerate() {
                        let row_y = y0 + (idx as i32) * 8;
                        let is_hl = menu.hover_item == Some(idx)
                            && mx >= x0
                            && mx < x0 + ww
                            && my >= y0
                            && my < y0 + hh;
                        let row_bg = if is_hl { hl_bg } else { bg };
                        let row_fg = if is_hl { hl_fg } else { fg };
                        let text = Self::menu_item_label(item);
                        self.rt.draw_text(x0, row_y, row_fg, row_bg, &text);
                    }
                }
            }
        }
    }

    fn maybe_call_draw_it(&mut self) -> Result<(), String> {
        if self.in_draw_it {
            return Ok(());
        }
        let Some(Value::Obj(fs)) = self.env.get("Fs") else {
            return Ok(());
        };
        let draw = fs.borrow().fields.get("draw_it").cloned();
        let Some(Value::FuncRef(name)) = draw else {
            return Ok(());
        };

        let (w, h) = self.rt.size();
        {
            let mut fs = fs.borrow_mut();
            fs.fields
                .insert("pix_width".to_string(), Value::Int(w as i64));
            fs.fields
                .insert("pix_height".to_string(), Value::Int(h as i64));
            fs.fields
                .insert("win_width".to_string(), Value::Int((w / 8) as i64));
            fs.fields
                .insert("win_height".to_string(), Value::Int((h / 8) as i64));
        }

        self.in_draw_it = true;
        let res = (|| {
            let _scope = EnvScopeGuard::new(&mut self.env);
            self.env
                .define("__tl_task".to_string(), Value::Obj(fs.clone()));
            self.env
                .define("__tl_dc".to_string(), Value::Obj(self.dc_alias.clone()));
            let args = [
                Expr::Var("__tl_task".to_string()),
                Expr::Var("__tl_dc".to_string()),
            ];
            let _ = self.call(&name, &args)?;
            Ok(())
        })();
        self.in_draw_it = false;
        res
    }

    fn maybe_draw_ctrls(&mut self) -> Result<(), String> {
        if self.in_draw_it {
            return Ok(());
        }
        let Some(Value::Obj(fs)) = self.env.get("Fs") else {
            return Ok(());
        };
        let head = match fs.borrow().fields.get("last_ctrl").cloned() {
            Some(Value::Obj(head)) => head,
            _ => return Ok(()),
        };

        let mut cur = match head.borrow().fields.get("next").cloned() {
            Some(Value::Obj(cur)) => cur,
            _ => return Ok(()),
        };

        const CTRLF_SHOW: i64 = 1;
        let mut steps = 0usize;
        while !Rc::ptr_eq(&cur, &head) {
            steps += 1;
            if steps > 4096 {
                return Err("control list appears to be looping".to_string());
            }

            let (next, flags, draw) = {
                let cur = cur.borrow();
                let next = cur.fields.get("next").cloned();
                let flags = cur
                    .fields
                    .get("flags")
                    .and_then(|v| v.as_i64().ok())
                    .unwrap_or(0);
                let draw = cur.fields.get("draw_it").cloned();
                (next, flags, draw)
            };

            if (flags & CTRLF_SHOW) != 0 {
                if let Some(Value::FuncRef(name)) = draw {
                    self.in_draw_it = true;
                    let res = (|| {
                        let _scope = EnvScopeGuard::new(&mut self.env);
                        self.env
                            .define("__tl_dc".to_string(), Value::Obj(self.dc_alias.clone()));
                        self.env
                            .define("__tl_ctrl".to_string(), Value::Obj(cur.clone()));
                        let args = [
                            Expr::Var("__tl_dc".to_string()),
                            Expr::Var("__tl_ctrl".to_string()),
                        ];
                        let _ = self.call(&name, &args)?;
                        Ok::<(), String>(())
                    })();
                    self.in_draw_it = false;
                    res?;
                }
            }

            cur = match next {
                Some(Value::Obj(next)) => next,
                _ => break,
            };
        }

        Ok(())
    }

    fn maybe_draw_mouse_overlay(&mut self) -> Result<(), String> {
        if self.in_draw_it {
            return Ok(());
        }
        let Some(Value::Obj(gr)) = self.env.get("gr") else {
            return Ok(());
        };
        let fp = gr.borrow().fields.get("fp_draw_ms").cloned();
        let Some(Value::FuncRef(name)) = fp else {
            return Ok(());
        };

        let (x, y) = {
            let pos = self.ms_pos.borrow();
            let x = pos
                .fields
                .get("x")
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0);
            let y = pos
                .fields
                .get("y")
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0);
            (x, y)
        };

        self.in_draw_it = true;
        let res = (|| {
            let _scope = EnvScopeGuard::new(&mut self.env);
            self.env
                .define("__tl_dc".to_string(), Value::Obj(self.dc_alias.clone()));
            self.env.define("__tl_mx".to_string(), Value::Int(x));
            self.env.define("__tl_my".to_string(), Value::Int(y));
            let args = [
                Expr::Var("__tl_dc".to_string()),
                Expr::Var("__tl_mx".to_string()),
                Expr::Var("__tl_my".to_string()),
            ];
            let _ = self.call(&name, &args)?;
            Ok::<(), String>(())
        })();
        self.in_draw_it = false;
        res
    }

    pub(super) fn present_with_overlays(&mut self) -> Result<(), String> {
        self.maybe_call_draw_it()?;
        self.maybe_draw_ctrls()?;
        self.render_menu_overlay();
        self.maybe_draw_mouse_overlay()?;
        match self.rt.present() {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                Err("Broken pipe".to_string())
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn map_key_code(&self, code: u32) -> u32 {
        const CH_BACKSPACE: u32 = 0x08;
        const CH_ESC: u32 = 0x1B;
        const CH_SHIFT_ESC: u32 = 0x1C;

        match code {
            protocol::KEY_ESCAPE => {
                if self.shift_down {
                    CH_SHIFT_ESC
                } else {
                    CH_ESC
                }
            }
            protocol::KEY_BACKSPACE => CH_BACKSPACE,
            protocol::KEY_ENTER => b'\n' as u32,
            protocol::KEY_TAB => b'\t' as u32,
            _ => {
                if self.ctrl_down {
                    if let Ok(ch) = u8::try_from(code) {
                        if ch.is_ascii_alphabetic() {
                            return (ch.to_ascii_uppercase() & 0x1F) as u32;
                        }
                    }
                }
                code
            }
        }
    }
}
