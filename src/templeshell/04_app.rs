const WIN_TITLE_H: i32 = 16;
const WIN_CLOSE_W: i32 = 16;
const WIN_DEFAULT_W: i32 = 320;
const WIN_DEFAULT_H: i32 = 256;

#[derive(Clone, Copy, Debug)]
struct RectI32 {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl RectI32 {
    fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowHit {
    None,
    TitleBar,
    Close,
    Client,
}

#[derive(Clone, Debug)]
struct AppWindow {
    id: AppId,
    title: String,
    rect: RectI32,
    closing: bool,
}

impl AppWindow {
    fn title_rect(&self) -> RectI32 {
        RectI32 {
            x: self.rect.x,
            y: self.rect.y,
            w: self.rect.w,
            h: WIN_TITLE_H.min(self.rect.h),
        }
    }

    fn close_rect(&self) -> RectI32 {
        RectI32 {
            x: self.rect.x + self.rect.w - WIN_CLOSE_W,
            y: self.rect.y,
            w: WIN_CLOSE_W,
            h: WIN_TITLE_H.min(self.rect.h),
        }
    }

    fn client_rect(&self) -> RectI32 {
        RectI32 {
            x: self.rect.x,
            y: self.rect.y + WIN_TITLE_H,
            w: self.rect.w,
            h: (self.rect.h - WIN_TITLE_H).max(0),
        }
    }

    fn hit_test(&self, px: i32, py: i32) -> WindowHit {
        if !self.rect.contains(px, py) {
            return WindowHit::None;
        }
        if self.close_rect().contains(px, py) {
            return WindowHit::Close;
        }
        if self.title_rect().contains(px, py) {
            return WindowHit::TitleBar;
        }
        if self.client_rect().contains(px, py) {
            return WindowHit::Client;
        }
        WindowHit::None
    }
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    window_id: AppId,
    grab_dx: i32,
    grab_dy: i32,
}

struct FbSpriteTarget<'a> {
    fb: &'a mut Framebuffer,
    clip: Option<(i32, i32, i32, i32)>,
}

impl temple_rt::sprite::SpriteTarget for FbSpriteTarget<'_> {
    fn set_pixel(&mut self, x: i32, y: i32, color: u8) {
        if x < 0 || y < 0 || x >= INTERNAL_W as i32 || y >= INTERNAL_H as i32 {
            return;
        }
        if let Some((x0, y0, x1, y1)) = self.clip {
            if x < x0 || y < y0 || x >= x1 || y >= y1 {
                return;
            }
        }
        self.fb.put_pixel(x as u32, y as u32, color);
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        if w <= 0 || h <= 0 {
            return;
        }

        let mut x0 = x;
        let mut y0 = y;
        let mut x1 = x.saturating_add(w);
        let mut y1 = y.saturating_add(h);

        x0 = x0.clamp(0, INTERNAL_W as i32);
        y0 = y0.clamp(0, INTERNAL_H as i32);
        x1 = x1.clamp(0, INTERNAL_W as i32);
        y1 = y1.clamp(0, INTERNAL_H as i32);

        if let Some((cx0, cy0, cx1, cy1)) = self.clip {
            x0 = x0.max(cx0);
            y0 = y0.max(cy0);
            x1 = x1.min(cx1);
            y1 = y1.min(cy1);
        }

        let w = x1 - x0;
        let h = y1 - y0;
        if w <= 0 || h <= 0 {
            return;
        }
        fill_rect_i32(self.fb, x0, y0, w, h, color);
    }

    fn blit_8bpp(&mut self, dst_x: i32, dst_y: i32, src_w: i32, src_h: i32, stride: i32, src: &[u8]) {
        if src_w <= 0 || src_h <= 0 || stride <= 0 {
            return;
        }
        let w = src_w as usize;
        let stride = stride as usize;
        for row in 0..(src_h as usize) {
            let start = match row.checked_mul(stride) {
                Some(v) => v,
                None => return,
            };
            let Some(row_src) = src.get(start..start + w) else {
                return;
            };
            for (col, &px) in row_src.iter().enumerate() {
                if px == 0xFF {
                    continue;
                }
                self.set_pixel(dst_x + col as i32, dst_y + row as i32, px);
            }
        }
    }
}

struct App {
    gfx: Gfx,
    fb: Framebuffer,
    fb_rgba: Vec<u8>,
    palette: [[u8; 4]; 256],
    palette_stack: Vec<[[u8; 4]; 256]>,
    terminal: Terminal,
    shell: Shell,
    temple_sock_path: Option<PathBuf>,
    wallpaper_app: Option<AppId>,
    wallpaper_title: Option<String>,
    mods: ModState,
    browser_last_click: Option<(std::time::Instant, usize)>,
    cursor_internal: Option<(u32, u32)>,
    mouse_left_down: bool,
    output_size: PhysicalSize<u32>,
    temple_apps: std::collections::BTreeMap<AppId, TempleAppSession>,
    focused_app: Option<AppId>,
    hovered_app: Option<AppId>,
    mouse_capture_app: Option<AppId>,
    drag: Option<DragState>,
    windows: Vec<AppWindow>,
    window_focused: bool,
    shutdown_started: bool,
    test: Option<TestState>,
}

impl App {
    fn new(
        gfx: Gfx,
        output_size: PhysicalSize<u32>,
        temple_sock: Option<PathBuf>,
        test: Option<TestState>,
    ) -> Self {
        let palette = assets::TEMPLEOS_GR_PALETTE_STD_RGBA256;
        let fb = Framebuffer::new();
        let fb_rgba = vec![0; (INTERNAL_W * INTERNAL_H * 4) as usize];
        let mut terminal = Terminal::new(COLOR_FG, COLOR_BG, OUTPUT_ROWS);
        let shell = Shell::new(test.is_some());

        use fmt::Write as _;
        writeln!(&mut terminal, "TempleShell").ok();
        writeln!(
            &mut terminal,
            "Internal framebuffer: {INTERNAL_W}x{INTERNAL_H}"
        )
        .ok();
        writeln!(
            &mut terminal,
            "Font: {FONT_W}x{FONT_H} ({TERM_COLS}x{TERM_ROWS} cells)"
        )
        .ok();
        if test.is_some() {
            writeln!(&mut terminal, "Temple root: (redacted)").ok();
        } else {
            writeln!(&mut terminal, "Temple root: {}", shell.root_dir.display()).ok();
        }
        let templeos_root = discover_templeos_root();
        if test.is_some() {
            writeln!(&mut terminal, "TempleOS tree: (redacted)").ok();
        } else if let Some(root) = &templeos_root {
            writeln!(&mut terminal, "TempleOS tree: {}", root.display()).ok();
        } else {
            writeln!(
                &mut terminal,
                "TempleOS tree: (not found; set TEMPLEOS_ROOT to run upstream demos/games)"
            )
            .ok();
        }
        if let Some(sock) = &temple_sock {
            if test.is_some() {
                writeln!(&mut terminal, "Temple IPC socket: (redacted)").ok();
            } else {
                writeln!(&mut terminal, "Temple IPC socket: {}", sock.display()).ok();
            }
            writeln!(&mut terminal, "Try: tapp demo").ok();
        }
        writeln!(&mut terminal, "Try: menu (PersonalMenu icons) or apps (F2)").ok();
        writeln!(&mut terminal, "").ok();
        writeln!(&mut terminal, "Type 'help' for commands.").ok();
        writeln!(&mut terminal, "Move the mouse; Esc quits (in shell).").ok();

        let mut app = Self {
            gfx,
            fb,
            fb_rgba,
            palette,
            palette_stack: Vec::new(),
            terminal,
            shell,
            temple_sock_path: temple_sock,
            wallpaper_app: None,
            wallpaper_title: None,
            mods: ModState::default(),
            browser_last_click: None,
            cursor_internal: None,
            mouse_left_down: false,
            output_size,
            temple_apps: std::collections::BTreeMap::new(),
            focused_app: None,
            hovered_app: None,
            mouse_capture_app: None,
            drag: None,
            windows: Vec::new(),
            window_focused: true,
            shutdown_started: false,
            test,
        };
        app.update_status_line();
        if app.test.is_none() {
            app.shell.run_autostart(&mut app.terminal);
        }
        app.shell.draw_prompt(&mut app.terminal);
        app
    }

    fn letterbox(&self) -> Letterbox {
        Letterbox::new(
            self.output_size.width.max(1),
            self.output_size.height.max(1),
        )
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.output_size = new_size;
        self.gfx.resize(new_size);
        self.update_status_line();
    }

    fn set_cursor_output_pos(&mut self, pos: PhysicalPosition<f64>) {
        self.cursor_internal = self.letterbox().map_point_to_internal(pos);
        self.update_status_line();
    }

    fn graceful_shutdown(&mut self) {
        if self.shutdown_started {
            return;
        }
        self.shutdown_started = true;

        // Best-effort persistence: state is usually flushed incrementally, but ensure we write
        // whatever we can before exiting.
        if self.test.is_none() {
            self.shell.save_history();
            self.shell.save_vars();
        }

        // Notify all connected apps.
        let ids: Vec<AppId> = self.temple_apps.keys().copied().collect();
        for id in ids {
            let _ = self.send_app_msg(id, protocol::Msg::shutdown());
        }

        // Avoid leaving clients blocked on `Present()` when `TEMPLE_SYNC_PRESENT=1`.
        self.flush_present_acks();

        // Try to kill the last spawned `tapp` child (if any) to avoid leaving a stray process.
        if let Some(mut child) = self.shell.tapp_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Cleanup the socket path so subsequent sessions don't trip over a stale file.
        if let Some(sock) = self.temple_sock_path.as_ref() {
            let _ = std::fs::remove_file(sock);
        }

        // Give clients a short chance to observe MSG_SHUTDOWN and disconnect.
        if self.test.is_none() {
            thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn default_window_rect(&self, idx: usize) -> RectI32 {
        let (mut x, mut y) = match idx {
            0 => (0, 0),
            1 => (WIN_DEFAULT_W, 0),
            _ => {
                let step = 24;
                let off = ((idx as i32 - 1) * step) % 200;
                (40 + off, 40 + off)
            }
        };

        let max_x = (INTERNAL_W as i32 - WIN_DEFAULT_W).max(0);
        let max_y = (INTERNAL_H as i32 - WIN_DEFAULT_H).max(0);
        x = x.clamp(0, max_x);
        y = y.clamp(0, max_y);

        RectI32 {
            x,
            y,
            w: WIN_DEFAULT_W,
            h: WIN_DEFAULT_H,
        }
    }

    fn open_app_window(&mut self, id: AppId, title: String) {
        let rect = self.default_window_rect(self.windows.len());
        self.windows.push(AppWindow {
            id,
            title,
            rect,
            closing: false,
        });
        self.focused_app = Some(id);
        self.hovered_app = None;
        self.mouse_capture_app = None;
        self.drag = None;
    }

    fn set_wallpaper_app(&mut self, id: AppId, title: String) {
        if let Some(old) = self.wallpaper_app {
            if old != id {
                let _ = self.send_app_msg(old, protocol::Msg::shutdown());
                self.temple_apps.remove(&old);
                self.windows.retain(|w| w.id != old);
            }
        }

        self.wallpaper_app = Some(id);
        self.wallpaper_title = Some(title);

        // Wallpaper should never capture focus or mouse capture.
        if self.focused_app == Some(id) {
            self.focused_app = self.windows.last().map(|w| w.id);
        }
        if self.hovered_app == Some(id) {
            self.hovered_app = None;
        }
        if self.mouse_capture_app == Some(id) {
            self.mouse_capture_app = None;
        }
        self.drag = None;
    }

    fn drop_app(&mut self, id: AppId) {
        self.temple_apps.remove(&id);
        self.windows.retain(|w| w.id != id);
        if self.wallpaper_app == Some(id) {
            self.wallpaper_app = None;
            self.wallpaper_title = None;
        }
        if self.focused_app == Some(id) {
            self.focused_app = self.windows.last().map(|w| w.id);
        }
        if self.hovered_app == Some(id) {
            self.hovered_app = None;
        }
        if self.mouse_capture_app == Some(id) {
            self.mouse_capture_app = None;
        }
        if self.drag.is_some_and(|d| d.window_id == id) {
            self.drag = None;
        }
        self.shell.tapp_connected = !self.temple_apps.is_empty();
        self.update_status_line();
    }

    fn bring_window_to_front(&mut self, idx: usize) {
        if idx >= self.windows.len() {
            return;
        }
        let win = self.windows.remove(idx);
        let id = win.id;
        self.windows.push(win);
        self.focused_app = Some(id);
    }

    fn focus_next_window(&mut self) {
        if self.windows.len() <= 1 {
            return;
        }
        if let Some(win) = self.windows.pop() {
            self.windows.insert(0, win);
        }
        self.focused_app = self.windows.last().map(|w| w.id);
        self.update_status_line();
    }

    fn hit_test_windows(&self, x: i32, y: i32) -> Option<(usize, WindowHit)> {
        for (idx, win) in self.windows.iter().enumerate().rev() {
            let hit = win.hit_test(x, y);
            if hit != WindowHit::None {
                return Some((idx, hit));
            }
        }
        None
    }

    fn client_window_at_point(&self, x: i32, y: i32) -> Option<AppId> {
        for win in self.windows.iter().rev() {
            if win.closing {
                continue;
            }
            if win.hit_test(x, y) == WindowHit::Client {
                return Some(win.id);
            }
        }
        None
    }

    fn send_app_msg(&mut self, id: AppId, msg: protocol::Msg) -> bool {
        let failed = match self.temple_apps.get(&id) {
            Some(sess) => sess.cmd_tx.send(msg).is_err(),
            None => true,
        };
        if failed {
            self.drop_app(id);
        }
        failed
    }

    fn flush_present_acks(&mut self) {
        let mut drop_ids = Vec::new();
        for (&id, sess) in self.temple_apps.iter_mut() {
            let Some(seq) = sess.pending_present_ack_seq.take() else {
                continue;
            };
            if sess.cmd_tx.send(protocol::Msg::present_ack(seq)).is_err() {
                drop_ids.push(id);
            }
        }
        for id in drop_ids {
            self.drop_app(id);
        }
    }

    fn map_point_to_app_coords(
        &self,
        win: &AppWindow,
        sess: &TempleAppSession,
        x: i32,
        y: i32,
    ) -> Option<(u32, u32)> {
        let client = win.client_rect();
        if client.w <= 0 || client.h <= 0 {
            return None;
        }
        if !client.contains(x, y) {
            return None;
        }
        let lx = (x - client.x) as i64;
        let ly = (y - client.y) as i64;
        let sx = ((lx * sess.width as i64) / client.w as i64)
            .clamp(0, sess.width.saturating_sub(1) as i64) as u32;
        let sy = ((ly * sess.height as i64) / client.h as i64)
            .clamp(0, sess.height.saturating_sub(1) as i64) as u32;
        Some((sx, sy))
    }

    fn map_point_to_app_coords_clamped(
        &self,
        win: &AppWindow,
        sess: &TempleAppSession,
        x: i32,
        y: i32,
    ) -> (u32, u32) {
        let client = win.client_rect();
        let clamped_x = x.clamp(client.x, client.x + client.w.saturating_sub(1));
        let clamped_y = y.clamp(client.y, client.y + client.h.saturating_sub(1));
        self.map_point_to_app_coords(win, sess, clamped_x, clamped_y)
            .unwrap_or((0, 0))
    }

    fn close_window(&mut self, id: AppId) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            if !win.closing {
                win.closing = true;
                let _ = self.send_app_msg(id, protocol::Msg::shutdown());
            }
        }
    }

    fn draw_windows(&mut self) {
        for win in &self.windows {
            let focused = self.focused_app == Some(win.id);
            let title_bg = if focused { 1 } else { 8 };
            let border = if focused { 15 } else { 7 };
            let close_bg = if focused { 4 } else { 6 };

            fill_rect_i32(
                &mut self.fb,
                win.rect.x,
                win.rect.y,
                win.rect.w,
                WIN_TITLE_H,
                title_bg,
            );
            fill_rect_i32(
                &mut self.fb,
                win.rect.x,
                win.rect.y + WIN_TITLE_H,
                win.rect.w,
                win.rect.h - WIN_TITLE_H,
                0,
            );

            let close = win.close_rect();
            fill_rect_i32(&mut self.fb, close.x, close.y, close.w, close.h, close_bg);

            if win.closing {
                let client = win.client_rect();
                fill_rect_i32(&mut self.fb, client.x, client.y, client.w, client.h, 8);
                draw_text_8x8(
                    &mut self.fb,
                    client.x + 8,
                    client.y + 8,
                    15,
                    8,
                    "Closing...",
                );
            } else if let Some(sess) = self.temple_apps.get(&win.id) {
                let client = win.client_rect();
                blit_scaled_indices(&mut self.fb, client, &sess.fb, sess.width, sess.height);
            }

            // Border + title separator.
            fill_rect_i32(&mut self.fb, win.rect.x, win.rect.y, win.rect.w, 1, border);
            fill_rect_i32(
                &mut self.fb,
                win.rect.x,
                win.rect.y + win.rect.h - 1,
                win.rect.w,
                1,
                border,
            );
            fill_rect_i32(&mut self.fb, win.rect.x, win.rect.y, 1, win.rect.h, border);
            fill_rect_i32(
                &mut self.fb,
                win.rect.x + win.rect.w - 1,
                win.rect.y,
                1,
                win.rect.h,
                border,
            );
            fill_rect_i32(
                &mut self.fb,
                win.rect.x,
                win.rect.y + WIN_TITLE_H - 1,
                win.rect.w,
                1,
                border,
            );

            let max_chars = ((win.rect.w - WIN_CLOSE_W - 8).max(0) as usize).saturating_div(8);
            let mut title = win.title.clone();
            if title.chars().count() > max_chars {
                title = title.chars().take(max_chars.max(1)).collect();
            }
            draw_text_8x8(
                &mut self.fb,
                win.rect.x + 4,
                win.rect.y + 4,
                15,
                title_bg,
                &title,
            );
            draw_text_8x8(&mut self.fb, close.x + 4, close.y + 4, 15, close_bg, "X");
        }
    }

    fn update_status_line(&mut self) {
        let row = STATUS_ROW;
        self.terminal.fill_row(row, COLOR_FG, COLOR_STATUS_BG);

        let app_state = if !self.window_focused && self.test.is_none() {
            "Paused (focus lost)"
        } else if self.focused_app.is_some() {
            "App focused (Alt+Tab switch, Ctrl+W close; Esc forwarded)"
        } else if !self.windows.is_empty() {
            "Shell focused (click a window to focus)"
        } else if self.shell.in_browser() {
            "File browser (Esc back)"
        } else {
            "Esc quits"
        };

        let mut line = String::new();
        use fmt::Write as _;
        if self.test.is_some() {
            let _ = write!(
                line,
                "Mouse: (redacted)  Output: (redacted)  Scale: (redacted)  State: (redacted)  WS: Super+1 Temple  Super+2 Linux"
            );
        } else {
            let scrollback = if self.focused_app.is_none() && self.terminal.view_offset() > 0 {
                format!("  SB:-{}", self.terminal.view_offset())
            } else {
                String::new()
            };

            match self.cursor_internal {
                Some((x, y)) => {
                    let _ = write!(line, "Mouse: {x:>3},{y:>3}  ");
                }
                None => {
                    let _ = write!(line, "Mouse: (outside)  ");
                }
            }
            let _ = write!(
                line,
                "Output: {}x{}  Scale: {:.3}x{:.3}  {}  WS: Super+1 Temple  Super+2 Linux",
                self.output_size.width,
                self.output_size.height,
                self.letterbox().scale_x,
                self.letterbox().scale_y,
                format!("{app_state}{scrollback}")
            );
        }
        self.terminal
            .write_at(0, row, COLOR_FG, COLOR_STATUS_BG, &line);
    }

    fn paste_text_into_running_app(&mut self, text: &str) {
        const MAX_PASTE_CHARS: usize = 4096;

        let Some(id) = self.focused_app else { return };
        let Some(sess) = self.temple_apps.get(&id) else {
            self.drop_app(id);
            return;
        };
        let cmd_tx = sess.cmd_tx.clone();

        let mut failed = false;
        for ch in text.chars().take(MAX_PASTE_CHARS) {
            let (code, down_up) = match ch {
                '\n' | '\r' => (protocol::KEY_ENTER, true),
                '\t' => (protocol::KEY_TAB, true),
                ch if ch.is_ascii_graphic() || ch == ' ' => (ch as u32, true),
                _ => (0, false),
            };
            if !down_up {
                continue;
            }
            if cmd_tx.send(protocol::Msg::key(code, true)).is_err()
                || cmd_tx.send(protocol::Msg::key(code, false)).is_err()
            {
                failed = true;
                break;
            }
        }

        if failed {
            self.drop_app(id);
        }
    }

    fn draw(&mut self) {
        if let Some(wall_id) = self.wallpaper_app {
            if let Some(sess) = self.temple_apps.get(&wall_id) {
                let dst = RectI32 {
                    x: 0,
                    y: 0,
                    w: INTERNAL_W as i32,
                    h: INTERNAL_H as i32,
                };
                blit_scaled_indices(&mut self.fb, dst, &sess.fb, sess.width, sess.height);
            } else {
                self.fb.indices.fill(0);
            }
            self.terminal
                .render(&mut self.fb, TerminalRenderMode::OverWallpaper);
        } else {
            self.terminal
                .render(&mut self.fb, TerminalRenderMode::Opaque);
        }
        if let Some(state) = self.shell.doc_viewer.as_ref() {
            let content_rows = Shell::doc_view_rows().max(1);
            let view_start = state.scroll;
            let view_end = view_start + content_rows;

            let clip = Some((
                0,
                FONT_H as i32,
                INTERNAL_W as i32,
                (PROMPT_ROW * FONT_H) as i32,
            ));
            let mut target = FbSpriteTarget {
                fb: &mut self.fb,
                clip,
            };

            let view_start_i32 = view_start as i32;
            let view_end_i32 = view_end as i32;
            for sp in &state.sprites {
                if sp.bbox_line1 <= view_start_i32 || sp.bbox_line0 >= view_end_i32 {
                    continue;
                }
                let Some(data) = state.bins.get(&sp.bin_num) else {
                    continue;
                };

                let base_x = sp.anchor_col as i32 * FONT_W as i32;
                let row_off = sp.anchor_line as i32 - view_start_i32;
                let base_y = (1 + row_off) as i32 * FONT_H as i32;
                temple_rt::sprite::sprite_render(&mut target, base_x, base_y, data);
            }
        }
        if !self.windows.is_empty() {
            self.draw_windows();
        }
        if self.test.is_none() {
            if let Some((x, y)) = self.cursor_internal {
                draw_software_cursor(&mut self.fb, x, y);
            }
        }
        self.fb.to_rgba(&self.palette, &mut self.fb_rgba);
    }

    fn dump_screenshot_png(&mut self, path: &Path) -> io::Result<()> {
        self.draw();
        write_png_rgba(path, INTERNAL_W, INTERNAL_H, &self.fb_rgba)
    }

    fn present(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.draw();
        if let Some(test) = self.test.as_mut() {
            let focused = self.focused_app.and_then(|id| self.temple_apps.get(&id));
            if let Err(err) = test.maybe_dump(&self.fb_rgba, focused) {
                eprintln!("templeshell: failed to dump PNG: {err}");
                test.exit_now = true;
            }
        }
        self.flush_present_acks();
        self.gfx.write_framebuffer(&self.fb_rgba);
        self.gfx.render(self.letterbox())
    }

    fn test_run_shell_command_spawns_app(cmd: &str) -> bool {
        let mut parts = cmd.split_whitespace();
        let Some(first) = parts.next() else {
            return false;
        };
        if first != "tapp" {
            return false;
        }
        let sub = parts.next().unwrap_or("demo");
        !matches!(sub, "list" | "tree" | "search" | "kill" | "status")
    }

    fn step_test_run_shell(&mut self) -> bool {
        let (run_shell_done, wait_target, run_shell_idx, run_shell_len) = match self.test.as_ref()
        {
            None => return false,
            Some(test) => (
                test.run_shell_done(),
                test.run_shell_wait_for_app_count,
                test.run_shell_idx,
                test.run_shell.len(),
            ),
        };

        if run_shell_done {
            return false;
        }

        let mut did_anything = false;

        if let Some(target) = wait_target {
            if self.temple_apps.len() < target {
                return false;
            }
            if let Some(test) = self.test.as_mut() {
                test.run_shell_wait_for_app_count = None;
            }
            did_anything = true;
        }

        if run_shell_idx >= run_shell_len {
            return did_anything;
        }

        let cmd = {
            let test = self.test.as_mut().expect("test state");
            let cmd = test.run_shell[test.run_shell_idx].clone();
            test.run_shell_idx += 1;
            cmd
        };

        self.shell.exec_line(&cmd, &mut self.terminal);
        self.shell.draw_prompt(&mut self.terminal);
        self.update_status_line();
        did_anything = true;

        if Self::test_run_shell_command_spawns_app(&cmd) {
            if let Some(test) = self.test.as_mut() {
                test.run_shell_wait_for_app_count = Some(self.temple_apps.len() + 1);
            }
        }

        did_anything
    }

    fn test_should_exit(&self) -> bool {
        self.test.as_ref().is_some_and(|t| t.exit_now)
    }
}

fn default_temple_sock_path(root_dir: &Path, test_mode: bool) -> PathBuf {
    if !test_mode {
        if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            let runtime_dir = PathBuf::from(runtime_dir);
            if !runtime_dir.as_os_str().is_empty() {
                return runtime_dir.join("temple.sock");
            }
        }
    }

    root_dir.join("temple.sock")
}

fn print_usage() {
    eprintln!("templeshell");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  templeshell [OPTIONS]");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("  --no-fullscreen                  Do not enter fullscreen");
    eprintln!("  --config <path>                  Set TEMPLE_ROOT (default: $HOME/.templelinux)");
    eprintln!("  --os-root <path>                 Set TEMPLEOS_ROOT (TempleOS tree root)");
    eprintln!("  --sock <path>                    Set TEMPLE_SOCK (IPC socket path)");
    eprintln!("  --test-dump-initial-png <path>   Dump initial framebuffer to PNG and exit");
    eprintln!(
        "  --test-dump-app-png <path>       Wait for first app Present(), dump PNG, then exit after app disconnect"
    );
    eprintln!(
        "  --test-dump-after-n-apps-present-png <n> <path>  Wait for N distinct apps Present(), dump PNG, then exit"
    );
    eprintln!(
        "  --test-dump-after-n-presents-png <n> <path>  Wait for N total app Present() messages, dump PNG, then exit"
    );
    eprintln!(
        "  --test-app-exit <enter|ctrlq|esc|mouseleft|none>  Exit gesture after dumping app PNG (default: enter)"
    );
    eprintln!(
        "  --test-send-after-first-app-present <event>  Send an app input event after the first Present() (repeatable)"
    );
    eprintln!("  --test-run-shell <cmd>           Run a shell command at startup (repeatable)");
    eprintln!("  -h, --help                       Print this help");
}

fn parse_cli_args() -> Result<CliArgs, String> {
    let mut out = CliArgs::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--no-fullscreen" => out.no_fullscreen = true,
            "--config" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--config expects a path".to_string())?;
                out.temple_root = Some(PathBuf::from(path));
            }
            "--os-root" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--os-root expects a path".to_string())?;
                out.templeos_root = Some(PathBuf::from(path));
            }
            "--sock" => {
                let path = args.next().ok_or_else(|| "--sock expects a path".to_string())?;
                out.temple_sock = Some(PathBuf::from(path));
            }
            "--test-dump-initial-png" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--test-dump-initial-png expects a path".to_string())?;
                out.test_dump_initial_png = Some(PathBuf::from(path));
            }
            "--test-dump-app-png" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--test-dump-app-png expects a path".to_string())?;
                out.test_dump_after_first_app_present_png = Some(PathBuf::from(path));
            }
            "--test-dump-after-n-apps-present-png" => {
                let n = args
                    .next()
                    .ok_or_else(|| {
                        "--test-dump-after-n-apps-present-png expects N and a path".to_string()
                    })?
                    .parse::<usize>()
                    .map_err(|_| {
                        "--test-dump-after-n-apps-present-png expects N to be an integer"
                            .to_string()
                    })?;
                let path = args.next().ok_or_else(|| {
                    "--test-dump-after-n-apps-present-png expects N and a path".to_string()
                })?;
                out.test_dump_after_n_apps_present_png = Some((n, PathBuf::from(path)));
            }
            "--test-dump-after-n-presents-png" => {
                let n = args
                    .next()
                    .ok_or_else(|| {
                        "--test-dump-after-n-presents-png expects N and a path".to_string()
                    })?
                    .parse::<usize>()
                    .map_err(|_| {
                        "--test-dump-after-n-presents-png expects N to be an integer".to_string()
                    })?;
                let path = args.next().ok_or_else(|| {
                    "--test-dump-after-n-presents-png expects N and a path".to_string()
                })?;
                out.test_dump_after_n_presents_png = Some((n, PathBuf::from(path)));
            }
            "--test-app-exit" => {
                let v = args
                    .next()
                    .ok_or_else(|| "--test-app-exit expects: enter|ctrlq|esc|mouseleft|none".to_string())?;
                let exit = match v.as_str() {
                    "enter" => TestAppExit::Enter,
                    "ctrlq" => TestAppExit::CtrlQ,
                    "esc" => TestAppExit::Esc,
                    "mouseleft" | "mouse_left" => TestAppExit::MouseLeft,
                    "none" => TestAppExit::None,
                    other => {
                        return Err(format!(
                            "--test-app-exit expects: enter|ctrlq|esc|mouseleft|none (got: {other})"
                        ));
                    }
                };
                out.test_app_exit = Some(exit);
            }
            "--test-send-after-first-app-present" => {
                let spec = args.next().ok_or_else(|| {
                    "--test-send-after-first-app-present expects an event spec".to_string()
                })?;
                out.test_send_after_first_app_present
                    .push(parse_test_event_spec(&spec)?);
            }
            "--test-run-shell" => {
                let cmd = args
                    .next()
                    .ok_or_else(|| "--test-run-shell expects a command line".to_string())?;
                out.test_run_shell.push(cmd);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }

    let mut test_flags = 0;
    if out.test_dump_initial_png.is_some() {
        test_flags += 1;
    }
    if out.test_dump_after_first_app_present_png.is_some() {
        test_flags += 1;
    }
    if out.test_dump_after_n_apps_present_png.is_some() {
        test_flags += 1;
    }
    if out.test_dump_after_n_presents_png.is_some() {
        test_flags += 1;
    }
    if test_flags > 1 {
        return Err("choose only one of the --test-dump-* flags".to_string());
    }

    Ok(out)
}

fn parse_test_event_spec(spec: &str) -> Result<protocol::Msg, String> {
    fn parse_u32(s: &str) -> Result<u32, String> {
        if let Some(hex) = s.strip_prefix("0x") {
            u32::from_str_radix(hex, 16).map_err(|_| format!("invalid number: {s}"))
        } else {
            s.parse::<u32>().map_err(|_| format!("invalid number: {s}"))
        }
    }

    let (kind, rest) = spec
        .split_once(':')
        .ok_or_else(|| format!("invalid event spec {spec:?} (expected kind:args)"))?;

    match kind {
        "mouse_move" => {
            let (xs, ys) = rest.split_once(',').ok_or_else(|| {
                format!("invalid mouse_move spec {spec:?} (expected mouse_move:x,y)")
            })?;
            let x = parse_u32(xs.trim())?;
            let y = parse_u32(ys.trim())?;
            Ok(protocol::Msg::mouse_move(x, y))
        }
        "mouse_button" => {
            let (btn_s, state_s) = rest.split_once(',').ok_or_else(|| {
                format!("invalid mouse_button spec {spec:?} (expected mouse_button:btn,down|up)")
            })?;
            let btn = match btn_s.trim() {
                "left" => protocol::MOUSE_BUTTON_LEFT,
                "right" => protocol::MOUSE_BUTTON_RIGHT,
                "middle" => protocol::MOUSE_BUTTON_MIDDLE,
                other => parse_u32(other)?,
            };
            let down = match state_s.trim() {
                "down" => true,
                "up" => false,
                other => {
                    return Err(format!(
                        "invalid mouse_button state {other:?} (expected down|up)"
                    ));
                }
            };
            Ok(protocol::Msg::mouse_button(btn, down))
        }
        "key" => {
            let (key_s, state_s) = rest
                .split_once(',')
                .ok_or_else(|| format!("invalid key spec {spec:?} (expected key:code,down|up)"))?;
            let code = match key_s.trim() {
                "esc" | "escape" => protocol::KEY_ESCAPE,
                "enter" => protocol::KEY_ENTER,
                "tab" => protocol::KEY_TAB,
                "backspace" => protocol::KEY_BACKSPACE,
                "delete" => protocol::KEY_DELETE,
                "home" => protocol::KEY_HOME,
                "end" => protocol::KEY_END,
                "pageup" | "pgup" => protocol::KEY_PAGE_UP,
                "pagedown" | "pgdn" => protocol::KEY_PAGE_DOWN,
                "insert" | "ins" => protocol::KEY_INSERT,
                "left" => protocol::KEY_LEFT,
                "right" => protocol::KEY_RIGHT,
                "up" => protocol::KEY_UP,
                "down" => protocol::KEY_DOWN,
                "ctrl" | "control" => protocol::KEY_CONTROL,
                "shift" => protocol::KEY_SHIFT,
                "alt" => protocol::KEY_ALT,
                "super" | "meta" | "win" => protocol::KEY_SUPER,
                "f1" => protocol::KEY_F1,
                "f2" => protocol::KEY_F2,
                "f3" => protocol::KEY_F3,
                "f4" => protocol::KEY_F4,
                "f5" => protocol::KEY_F5,
                "f6" => protocol::KEY_F6,
                "f7" => protocol::KEY_F7,
                "f8" => protocol::KEY_F8,
                "f9" => protocol::KEY_F9,
                "f10" => protocol::KEY_F10,
                "f11" => protocol::KEY_F11,
                "f12" => protocol::KEY_F12,
                other if other.len() == 1 => other.as_bytes()[0] as u32,
                other => parse_u32(other)?,
            };
            let down = match state_s.trim() {
                "down" => true,
                "up" => false,
                other => return Err(format!("invalid key state {other:?} (expected down|up)")),
            };
            Ok(protocol::Msg::key(code, down))
        }
        other => Err(format!(
            "unknown event kind {other:?} (expected key|mouse_move|mouse_button)"
        )),
    }
}

fn ensure_temple_sock_from_env_or_default(root_dir: &Path, test_mode: bool) -> PathBuf {
    if let Some(path) = std::env::var_os("TEMPLE_SOCK") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    let temple_sock = default_temple_sock_path(root_dir, test_mode);
    if let Some(parent) = temple_sock.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    unsafe {
        std::env::set_var("TEMPLE_SOCK", temple_sock.as_os_str());
    }
    temple_sock
}

fn is_valid_templeos_root(path: &Path) -> bool {
    path.join("Kernel/FontStd.HC").exists() && path.join("Adam/Gr/GrPalette.HC").exists()
}

fn discover_templeos_root() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("TEMPLEOS_ROOT") {
        if !v.trim().is_empty() {
            let p = PathBuf::from(v);
            if is_valid_templeos_root(&p) {
                return Some(p);
            }
        }
    }

    let mut bases = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            bases.push(dir.to_path_buf());
        }
    }

    for base in bases {
        let mut dir = base.clone();
        for _ in 0..8usize {
            let candidate = dir.join("third_party/TempleOS");
            if is_valid_templeos_root(&candidate) {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    let sys = PathBuf::from("/usr/share/templelinux/TempleOS");
    if is_valid_templeos_root(&sys) {
        return Some(sys);
    }

    None
}

fn walk_dir_collect_hc_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk_dir_collect_hc_files(&path, out);
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("HC"))
        {
            out.push(path);
        }
    }
}

#[derive(Clone, Debug)]
struct TempleOsProgram {
    alias: String,
    rel_no_ext: String,
    spec: String,
}

fn discover_templeos_programs(templeos_root: &Path) -> Vec<TempleOsProgram> {
    let mut files: Vec<PathBuf> = Vec::new();
    for sub in ["Demo", "Apps"] {
        let dir = templeos_root.join(sub);
        walk_dir_collect_hc_files(&dir, &mut files);
    }

    let mut programs: Vec<TempleOsProgram> = Vec::new();
    for file in files {
        let Ok(rel) = file.strip_prefix(templeos_root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let rel_no_ext = rel
            .strip_suffix(".HC")
            .or_else(|| rel.strip_suffix(".hc"))
            .unwrap_or(rel.as_str())
            .to_string();
        let alias = Path::new(&rel)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("(unknown)")
            .to_string();
        let spec = format!("::/{rel}");
        programs.push(TempleOsProgram {
            alias,
            rel_no_ext,
            spec,
        });
    }

    programs.sort_by(|a, b| a.rel_no_ext.cmp(&b.rel_no_ext));
    programs
}

fn resolve_templeos_program_spec(
    programs: &[TempleOsProgram],
    query: &str,
) -> Result<String, String> {
    let q_in = query.trim();
    if q_in.is_empty() {
        return Err(
            "missing program (expected path like ::/Demo/Print.HC or an alias)".to_string(),
        );
    }
    if q_in.starts_with("::/") {
        return Ok(q_in.to_string());
    }

    let q_owned;
    let q = if q_in.len() >= 3 && q_in[q_in.len() - 3..].eq_ignore_ascii_case(".hc") {
        q_owned = q_in[..q_in.len() - 3].to_string();
        q_owned.as_str()
    } else {
        q_in
    };

    let q_lower = q.to_ascii_lowercase();

    let mut exact_alias: Vec<&TempleOsProgram> = programs
        .iter()
        .filter(|p| p.alias.eq_ignore_ascii_case(q))
        .collect();
    if exact_alias.len() == 1 {
        return Ok(exact_alias.remove(0).spec.clone());
    }

    let mut exact_rel: Vec<&TempleOsProgram> = programs
        .iter()
        .filter(|p| p.rel_no_ext.eq_ignore_ascii_case(q))
        .collect();
    if exact_rel.len() == 1 {
        return Ok(exact_rel.remove(0).spec.clone());
    }

    let mut contains: Vec<&TempleOsProgram> = programs
        .iter()
        .filter(|p| {
            p.alias.to_ascii_lowercase().contains(&q_lower)
                || p.rel_no_ext.to_ascii_lowercase().contains(&q_lower)
        })
        .collect();
    contains.sort_by(|a, b| a.rel_no_ext.cmp(&b.rel_no_ext));
    contains.truncate(10);

    if exact_alias.len() > 1 {
        let mut msg = format!("ambiguous alias {q_in:?}; matches:");
        for p in exact_alias.iter().take(10) {
            msg.push_str("\n  ");
            msg.push_str(&p.rel_no_ext);
        }
        return Err(msg);
    }
    if exact_rel.len() > 1 {
        let mut msg = format!("ambiguous path {q_in:?}; matches:");
        for p in exact_rel.iter().take(10) {
            msg.push_str("\n  ");
            msg.push_str(&p.rel_no_ext);
        }
        return Err(msg);
    }

    if let Some(p) = programs
        .iter()
        .find(|p| p.alias.eq_ignore_ascii_case(q) || p.rel_no_ext.eq_ignore_ascii_case(q))
    {
        return Ok(p.spec.clone());
    }

    if contains.len() == 1 {
        return Ok(contains[0].spec.clone());
    }

    if contains.is_empty() {
        return Err(format!(
            "no TempleOS program found matching {q_in:?} (try: tapp list or tapp search <text>)"
        ));
    }

    let mut msg = format!("no exact match for {q_in:?}; closest matches:");
    for p in contains {
        msg.push_str("\n  ");
        msg.push_str(&p.rel_no_ext);
    }
    Err(msg)
}

fn map_winit_key_to_temple_code(key: &Key) -> Option<u32> {
    match key {
        Key::Named(NamedKey::Escape) => Some(protocol::KEY_ESCAPE),
        Key::Named(NamedKey::Enter) => Some(protocol::KEY_ENTER),
        Key::Named(NamedKey::Backspace) => Some(protocol::KEY_BACKSPACE),
        Key::Named(NamedKey::Delete) => Some(protocol::KEY_DELETE),
        Key::Named(NamedKey::Tab) => Some(protocol::KEY_TAB),
        Key::Named(NamedKey::Home) => Some(protocol::KEY_HOME),
        Key::Named(NamedKey::End) => Some(protocol::KEY_END),
        Key::Named(NamedKey::PageUp) => Some(protocol::KEY_PAGE_UP),
        Key::Named(NamedKey::PageDown) => Some(protocol::KEY_PAGE_DOWN),
        Key::Named(NamedKey::Insert) => Some(protocol::KEY_INSERT),
        Key::Named(NamedKey::ArrowLeft) => Some(protocol::KEY_LEFT),
        Key::Named(NamedKey::ArrowRight) => Some(protocol::KEY_RIGHT),
        Key::Named(NamedKey::ArrowUp) => Some(protocol::KEY_UP),
        Key::Named(NamedKey::ArrowDown) => Some(protocol::KEY_DOWN),
        Key::Named(NamedKey::Shift) => Some(protocol::KEY_SHIFT),
        Key::Named(NamedKey::Control) => Some(protocol::KEY_CONTROL),
        Key::Named(NamedKey::Alt) => Some(protocol::KEY_ALT),
        Key::Named(NamedKey::Super) => Some(protocol::KEY_SUPER),
        Key::Named(NamedKey::F1) => Some(protocol::KEY_F1),
        Key::Named(NamedKey::F2) => Some(protocol::KEY_F2),
        Key::Named(NamedKey::F3) => Some(protocol::KEY_F3),
        Key::Named(NamedKey::F4) => Some(protocol::KEY_F4),
        Key::Named(NamedKey::F5) => Some(protocol::KEY_F5),
        Key::Named(NamedKey::F6) => Some(protocol::KEY_F6),
        Key::Named(NamedKey::F7) => Some(protocol::KEY_F7),
        Key::Named(NamedKey::F8) => Some(protocol::KEY_F8),
        Key::Named(NamedKey::F9) => Some(protocol::KEY_F9),
        Key::Named(NamedKey::F10) => Some(protocol::KEY_F10),
        Key::Named(NamedKey::F11) => Some(protocol::KEY_F11),
        Key::Named(NamedKey::F12) => Some(protocol::KEY_F12),
        Key::Named(NamedKey::Space) => Some(b' ' as u32),
        Key::Character(s) => {
            let mut it = s.chars();
            let ch = it.next()?;
            if it.next().is_some() || !ch.is_ascii() {
                return None;
            }
            Some(ch as u32)
        }
        _ => None,
    }
}

fn map_winit_mouse_button_to_temple_button(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => protocol::MOUSE_BUTTON_LEFT,
        MouseButton::Right => protocol::MOUSE_BUTTON_RIGHT,
        MouseButton::Middle => protocol::MOUSE_BUTTON_MIDDLE,
        MouseButton::Back => protocol::MOUSE_BUTTON_BACK,
        MouseButton::Forward => protocol::MOUSE_BUTTON_FORWARD,
        MouseButton::Other(n) => protocol::MOUSE_BUTTON_OTHER_BASE + n as u32,
    }
}

fn spawn_temple_ipc_server(
    proxy: EventLoopProxy<UserEvent>,
    socket_path: PathBuf,
    audio: audio::Audio,
) {
    thread::spawn(move || {
        let _ = std::fs::remove_file(&socket_path);
        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => l,
            Err(err) => {
                let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(format!(
                    "ipc: bind {}: {err}",
                    socket_path.display()
                ))));
                return;
            }
        };

        let mut next_id: AppId = 1;
        loop {
            let audio = audio.clone();
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(err) => {
                    let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(format!(
                        "ipc: accept: {err}"
                    ))));
                    continue;
                }
            };

            let id = next_id;
            next_id = next_id.wrapping_add(1).max(1);

            let hello = match protocol::read_msg(&mut stream) {
                Ok(m) => m,
                Err(err) => {
                    let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(format!(
                        "ipc[{id}]: read hello: {err}"
                    ))));
                    continue;
                }
            };
            if hello.kind != protocol::MSG_HELLO {
                let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(format!(
                    "ipc[{id}]: bad hello"
                ))));
                continue;
            }

            let shm = match (|| -> std::io::Result<File> {
                use nix::sys::memfd::{MemFdCreateFlag, memfd_create};
                let name = CString::new(format!("temple-fb-{id}")).expect("CString");
                let fd = memfd_create(name.as_c_str(), MemFdCreateFlag::MFD_CLOEXEC)
                    .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
                let file: File = fd.into();
                file.set_len((INTERNAL_W * INTERNAL_H) as u64)?;
                Ok(file)
            })() {
                Ok(file) => file,
                Err(err) => {
                    let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(format!(
                        "ipc[{id}]: shm: {err}"
                    ))));
                    continue;
                }
            };

            if let Err(err) = protocol::send_msg_with_fd(
                &stream,
                protocol::Msg::hello_ack(INTERNAL_W, INTERNAL_H),
                shm.as_raw_fd(),
            ) {
                let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(format!(
                    "ipc[{id}]: send hello_ack: {err}"
                ))));
                continue;
            }

            let (cmd_tx, cmd_rx) = mpsc::channel::<protocol::Msg>();
            let writer = match stream.try_clone() {
                Ok(s) => s,
                Err(err) => {
                    let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(format!(
                        "ipc[{id}]: clone stream: {err}"
                    ))));
                    continue;
                }
            };

            thread::spawn(move || {
                let mut stream = writer;
                while let Ok(msg) = cmd_rx.recv() {
                    if protocol::write_msg(&mut stream, msg).is_err() {
                        break;
                    }
                }
            });

            {
                let mut reader = stream;
                let proxy = proxy.clone();
                let audio = audio.clone();
                thread::spawn(move || {
                    use std::io::Read as _;

                    loop {
                        match protocol::read_msg(&mut reader) {
                            Ok(msg) => match msg.kind {
                                protocol::MSG_PRESENT => {
                                    let _ = proxy.send_event(UserEvent::Ipc(
                                        TempleIpcEvent::AppPresent { id, seq: msg.a },
                                    ));
                                }
                                protocol::MSG_PALETTE_COLOR_SET => {
                                    let color_index = msg.a.min(255) as u8;
                                    let packed = msg.b;
                                    let rgba = [
                                        ((packed >> 24) & 0xff) as u8,
                                        ((packed >> 16) & 0xff) as u8,
                                        ((packed >> 8) & 0xff) as u8,
                                        (packed & 0xff) as u8,
                                    ];
                                    let _ = proxy.send_event(UserEvent::Ipc(
                                        TempleIpcEvent::PaletteColorSet {
                                            id,
                                            color_index,
                                            rgba,
                                        },
                                    ));
                                }
                                protocol::MSG_SETTINGS_PUSH => {
                                    let _ = proxy.send_event(UserEvent::Ipc(
                                        TempleIpcEvent::SettingsPush { id },
                                    ));
                                }
                                protocol::MSG_SETTINGS_POP => {
                                    let _ = proxy.send_event(UserEvent::Ipc(
                                        TempleIpcEvent::SettingsPop { id },
                                    ));
                                }
                                protocol::MSG_CLIPBOARD_SET => {
                                    const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

                                    let len = msg.a as usize;
                                    if len > MAX_CLIPBOARD_BYTES {
                                        let mut remaining = len;
                                        let mut buf = [0u8; 4096];
                                        while remaining > 0 {
                                            let to_read = remaining.min(buf.len());
                                            match reader.read(&mut buf[..to_read]) {
                                                Ok(0) => break,
                                                Ok(n) => remaining = remaining.saturating_sub(n),
                                                Err(_) => break,
                                            }
                                        }
                                        let _ = proxy.send_event(UserEvent::Ipc(
                                            TempleIpcEvent::Log(format!(
                                                "ipc[{id}]: clipboard payload too large: {len} bytes"
                                            )),
                                        ));
                                        continue;
                                    }

                                    let mut buf = vec![0u8; len];
                                    if let Err(err) = reader.read_exact(&mut buf) {
                                        let _ =
                                            proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(
                                                format!("ipc[{id}]: read clipboard payload: {err}"),
                                            )));
                                        break;
                                    }
                                    let text = String::from_utf8_lossy(&buf).to_string();
                                    let _ = proxy.send_event(UserEvent::Ipc(
                                        TempleIpcEvent::ClipboardSet { id, text },
                                    ));
                                }
                                protocol::MSG_SND => {
                                    audio.snd(msg.a as u8);
                                }
                                protocol::MSG_MUTE => {
                                    audio.mute(msg.a != 0);
                                }
                                _ => {}
                            },
                            Err(_) => break,
                        }
                    }

                    let _ =
                        proxy.send_event(UserEvent::Ipc(TempleIpcEvent::AppDisconnected { id }));
                });
            }

            let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::AppConnected {
                id,
                shm,
                width: INTERNAL_W,
                height: INTERNAL_H,
                cmd_tx,
            }));
        }
    });
}

fn main() {
    let mut args = match parse_cli_args() {
        Ok(v) => v,
        Err(err) => {
            eprintln!("templeshell: {err}\n");
            print_usage();
            std::process::exit(2);
        }
    };

    let test_mode = args.test_dump_initial_png.is_some()
        || args.test_dump_after_first_app_present_png.is_some()
        || args.test_dump_after_n_apps_present_png.is_some()
        || args.test_dump_after_n_presents_png.is_some();

    if let Some(root) = args.temple_root.take() {
        unsafe {
            std::env::set_var("TEMPLE_ROOT", root.as_os_str());
        }
    }
    if let Some(root) = args.templeos_root.take() {
        unsafe {
            std::env::set_var("TEMPLEOS_ROOT", root.as_os_str());
        }
    }
    if let Some(sock) = args.temple_sock.take() {
        unsafe {
            std::env::set_var("TEMPLE_SOCK", sock.as_os_str());
        }
    }

    // Environment discovery phase (pre-graphics init).
    let temple_root = pick_temple_root(test_mode);
    unsafe {
        std::env::set_var("TEMPLE_ROOT", temple_root.as_os_str());
    }
    let _ = std::fs::create_dir_all(&temple_root);
    for dir in ["Home", "Doc", "Cfg", "Apps"] {
        let _ = std::fs::create_dir_all(temple_root.join(dir));
    }

    if let Ok(v) = std::env::var("TEMPLEOS_ROOT") {
        let v = v.trim();
        if !v.is_empty() {
            let p = PathBuf::from(v);
            if !is_valid_templeos_root(&p) {
                eprintln!("templeshell: TEMPLEOS_ROOT is set but invalid: {}", p.display());
                eprintln!("templeshell: expected Kernel/FontStd.HC and Adam/Gr/GrPalette.HC");
                eprintln!();
                eprintln!("Fix:");
                eprintln!("  - set TEMPLEOS_ROOT (or pass --os-root) to a valid TempleOS tree root, or");
                eprintln!("  - ensure the TempleOS submodule is present (git submodule update --init --recursive), or");
                eprintln!("  - install templelinux-templeos-data (system path: /usr/share/templelinux/TempleOS)");
                std::process::exit(2);
            }
        }
    }

    let Some(templeos_root) = discover_templeos_root() else {
        eprintln!("templeshell: TempleOS tree not found.");
        eprintln!("templeshell: expected Kernel/FontStd.HC and Adam/Gr/GrPalette.HC.");
        eprintln!();
        eprintln!("Fix:");
        eprintln!("  - set TEMPLEOS_ROOT (or pass --os-root), or");
        eprintln!("  - ensure third_party/TempleOS is present (git submodule update --init --recursive), or");
        eprintln!("  - install templelinux-templeos-data (system path: /usr/share/templelinux/TempleOS)");
        std::process::exit(2);
    };
    unsafe {
        std::env::set_var("TEMPLEOS_ROOT", templeos_root.as_os_str());
    }

    let mut test = if let Some(path) = args.test_dump_initial_png {
        Some(TestState::dump_initial_frame(path))
    } else if let Some(path) = args.test_dump_after_first_app_present_png {
        let exit = args.test_app_exit.unwrap_or(TestAppExit::Enter);
        Some(TestState::dump_after_first_app_present(path, exit))
    } else if let Some((n, path)) = args.test_dump_after_n_apps_present_png {
        Some(TestState::dump_after_n_apps_present(path, n))
    } else if let Some((n, path)) = args.test_dump_after_n_presents_png {
        let exit = args.test_app_exit.unwrap_or(TestAppExit::Enter);
        Some(TestState::dump_after_n_presents(path, n, exit))
    } else {
        None
    };
    if let Some(test) = test.as_mut() {
        test.send_after_first_app_present =
            std::mem::take(&mut args.test_send_after_first_app_present);
        test.run_shell = std::mem::take(&mut args.test_run_shell);
    }

    let temple_sock = ensure_temple_sock_from_env_or_default(&temple_root, test_mode);

    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();
    let event_loop = builder.build().expect("event loop");
    let proxy = event_loop.create_proxy();

    let audio = audio::Audio::spawn({
        let proxy = proxy.clone();
        move |line| {
            let _ = proxy.send_event(UserEvent::Ipc(TempleIpcEvent::Log(line)));
        }
    });
    spawn_temple_ipc_server(proxy, temple_sock.clone(), audio);

    let window = Arc::new({
        let mut builder = WindowBuilder::new()
            .with_title("TempleShell")
            .with_inner_size(PhysicalSize::new(1280, 720));
        #[cfg(target_os = "linux")]
        {
            use winit::platform::wayland::WindowBuilderExtWayland as _;
            builder = builder.with_name("templeshell", "templeshell");
        }
        builder.build(&event_loop).expect("create window")
    });
    window.set_cursor_visible(false);

    let no_fullscreen = args.no_fullscreen || test.is_some();
    if !no_fullscreen {
        window.set_fullscreen(Some(Fullscreen::Borderless(None)));
    }

    let main_window_id = window.id();
    let mut app = App::new(
        pollster::block_on(Gfx::new(window.clone())),
        window.inner_size(),
        Some(temple_sock),
        test,
    );

    window.request_redraw();

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::UserEvent(UserEvent::Ipc(ev)) => match ev {
                    TempleIpcEvent::PaletteColorSet {
                        id: _,
                        color_index,
                        rgba,
                    } => {
                        if let Some(slot) = app.palette.get_mut(color_index as usize) {
                            *slot = rgba;
                            window.request_redraw();
                        }
                    }
                    TempleIpcEvent::SettingsPush { id: _ } => {
                        const MAX_STACK: usize = 64;
                        if app.palette_stack.len() < MAX_STACK {
                            app.palette_stack.push(app.palette);
                        }
                    }
                    TempleIpcEvent::SettingsPop { id: _ } => {
                        if let Some(p) = app.palette_stack.pop() {
                            app.palette = p;
                            window.request_redraw();
                        }
                    }
                    TempleIpcEvent::Log(line) => {
                        use fmt::Write as _;
                        let _ = writeln!(&mut app.terminal, "{line}");
                        window.request_redraw();
                    }
                    TempleIpcEvent::ClipboardSet { id, text } => {
                        use fmt::Write as _;
                        match app.shell.clipboard.set_text(&text) {
                            Ok(()) => {
                                let _ = writeln!(
                                    &mut app.terminal,
                                    "[clipboard set from app {id}: {} bytes]",
                                    text.len()
                                );
                            }
                            Err(err) => {
                                let _ = writeln!(&mut app.terminal, "clipboard: set: {err}");
                            }
                        }
                        window.request_redraw();
                    }
                    TempleIpcEvent::AppConnected {
                        id,
                        shm,
                        width,
                        height,
                        cmd_tx,
                    } => {
                        let len = (width * height) as usize;
                        match unsafe { memmap2::MmapOptions::new().len(len).map(&shm) } {
                            Ok(map) => {
                                use fmt::Write as _;
                                let _ = writeln!(&mut app.terminal, "[temple app connected: {id}]");
                                app.temple_apps.insert(
                                    id,
                                    TempleAppSession {
                                        fb: map,
                                        width,
                                        height,
                                        cmd_tx,
                                        pending_present_ack_seq: None,
                                    },
                                );
                                let title = app
                                    .shell
                                    .take_queued_window_title()
                                    .unwrap_or_else(|| format!("App {id}"));
                                let kind = app.shell.take_queued_window_kind();
                                match kind {
                                    PendingWindowKind::Normal => app.open_app_window(id, title),
                                    PendingWindowKind::Wallpaper => app.set_wallpaper_app(id, title),
                                }
                                app.shell.tapp_connected = !app.temple_apps.is_empty();
                                app.update_status_line();
                                window.request_redraw();
                            }
                            Err(err) => {
                                use fmt::Write as _;
                                let _ =
                                    writeln!(&mut app.terminal, "ipc: failed to map shm: {err}");
                            }
                        }
                    }
                    TempleIpcEvent::AppPresent { id, seq } => {
                        if app.test.is_none() && !app.window_focused {
                            // If we're unfocused, avoid rendering churn but ACK promptly so clients
                            // don't stall when `TEMPLE_SYNC_PRESENT=1`.
                            let _ = app.send_app_msg(id, protocol::Msg::present_ack(seq));
                        } else if let Some(sess) = app.temple_apps.get_mut(&id) {
                            sess.pending_present_ack_seq = Some(seq);
                            window.request_redraw();
                        }
                        if let Some(test) = app.test.as_mut() {
                            test.on_app_present(id);
                        }
                    }
                    TempleIpcEvent::AppDisconnected { id } => {
                        if app.temple_apps.contains_key(&id)
                            || app.windows.iter().any(|w| w.id == id)
                        {
                            use fmt::Write as _;
                            let _ = writeln!(&mut app.terminal, "[temple app disconnected: {id}]");
                            app.drop_app(id);
                            window.request_redraw();
                        }
                        if let Some(test) = app.test.as_mut() {
                            test.on_app_disconnected();
                            if test.exit_now {
                                elwt.exit();
                            }
                        }
                    }
                },
                Event::WindowEvent { event, window_id } if window_id == main_window_id => {
                    match event {
                        WindowEvent::CloseRequested => {
                            app.graceful_shutdown();
                            elwt.exit();
                        }
                        WindowEvent::Focused(focused) => {
                            if app.test.is_some() {
                                // In golden-test mode, ignore focus transitions.
                            } else {
                                app.window_focused = focused;

                                // Flush input state on focus changes to avoid stuck modifiers after Alt+Tab.
                                app.mods = ModState::default();

                                if !focused {
                                    app.cursor_internal = None;
                                    app.drag = None;
                                    app.mouse_left_down = false;
                                    if let Some(id) = app.hovered_app.take() {
                                        let _ = app.send_app_msg(id, protocol::Msg::mouse_leave());
                                    }
                                    app.mouse_capture_app = None;
                                }

                                app.update_status_line();
                                window.request_redraw();
                            }
                        }
                        WindowEvent::Resized(new_size) => {
                            app.resize(new_size);
                            window.request_redraw();
                        }
                        WindowEvent::RedrawRequested => match app.present() {
                            Ok(()) => {}
                            Err(wgpu::SurfaceError::Lost) => app.resize(app.output_size),
                            Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                            Err(wgpu::SurfaceError::Timeout) => {}
                            Err(wgpu::SurfaceError::Outdated) => {}
                        },
                        WindowEvent::KeyboardInput { event, .. } => {
                            if app.test.is_some() {
                                // In golden-test mode, ignore real keyboard input.
                            } else if !app.window_focused {
                                // Paused (focus lost): ignore host input.
                            } else {
                                let down = event.state == ElementState::Pressed;
                                app.mods.update(&event.logical_key, down);

                            if down
                                && app.mods.alt
                                && event.logical_key == Key::Named(NamedKey::Tab)
                            {
                                app.focus_next_window();
                                window.request_redraw();
                                return;
                            }
                            if down && app.mods.ctrl {
                                if let Key::Character(s) = &event.logical_key {
                                    if s.eq_ignore_ascii_case("w") {
                                        if let Some(id) = app.focused_app {
                                            app.close_window(id);
                                            window.request_redraw();
                                        }
                                        return;
                                    }
                                }
                            }

                            if down && app.mods.is_paste(&event.logical_key) {
                                use fmt::Write as _;
                                match app.shell.clipboard.get_text() {
                                    Ok(text) => {
                                        if app.focused_app.is_some() {
                                            app.paste_text_into_running_app(&text);
                                        } else if app.shell.paste_text(&text, &mut app.terminal) {
                                            window.request_redraw();
                                        }
                                    }
                                    Err(err) => {
                                        let _ =
                                            writeln!(&mut app.terminal, "clipboard: get: {err}");
                                        window.request_redraw();
                                    }
                                }
                                return;
                            }

                            if let Some(id) = app.focused_app {
                                if let Some(code) = map_winit_key_to_temple_code(&event.logical_key)
                                {
                                    let _ = app.send_app_msg(id, protocol::Msg::key(code, down));
                                }
                                return;
                            }

                            if down {
                                if !app.shell.in_browser() {
                                    match &event.logical_key {
                                        Key::Named(NamedKey::PageUp) => {
                                            let page =
                                                app.terminal.scroll_rows.saturating_sub(1).max(1)
                                                    as usize;
                                            app.terminal.scroll_view_up(page);
                                            app.update_status_line();
                                            window.request_redraw();
                                            return;
                                        }
                                        Key::Named(NamedKey::PageDown) => {
                                            let page =
                                                app.terminal.scroll_rows.saturating_sub(1).max(1)
                                                    as usize;
                                            app.terminal.scroll_view_down(page);
                                            app.update_status_line();
                                            window.request_redraw();
                                            return;
                                        }
                                        Key::Named(NamedKey::Home) if app.mods.ctrl => {
                                            app.terminal.scroll_view_to_top();
                                            app.update_status_line();
                                            window.request_redraw();
                                            return;
                                        }
                                        Key::Named(NamedKey::End) if app.mods.ctrl => {
                                            app.terminal.scroll_view_to_bottom();
                                            app.update_status_line();
                                            window.request_redraw();
                                            return;
                                        }
                                        _ => {}
                                    }
                                }

                                if event.logical_key == Key::Named(NamedKey::Escape)
                                    && !app.shell.in_browser()
                                {
                                    app.graceful_shutdown();
                                    elwt.exit();
                                    return;
                                }
                                if app.shell.handle_key(&event.logical_key, &mut app.terminal) {
                                    app.update_status_line();
                                    window.request_redraw();
                                }

                                if let Some((spec, host_path)) = app.shell.take_pending_screenshot()
                                {
                                    use fmt::Write as _;
                                    match app.dump_screenshot_png(&host_path) {
                                        Ok(()) => {
                                            let _ = writeln!(
                                                &mut app.terminal,
                                                "screenshot: saved {spec}"
                                            );
                                        }
                                        Err(err) => {
                                            let _ = writeln!(
                                                &mut app.terminal,
                                                "screenshot: {spec}: {err}"
                                            );
                                        }
                                    }
                                    app.update_status_line();
                                    window.request_redraw();
                                }

                                if app.shell.take_exit_requested() {
                                    app.graceful_shutdown();
                                    elwt.exit();
                                    return;
                                }
                            }
                            }
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            if app.test.is_some() {
                                // In golden-test mode we avoid forwarding real pointer events.
                                // Xvfb pointer state can be nondeterministic, and Temple apps
                                // (demo/paint/etc.) may render the pointer position.
                            } else if !app.window_focused {
                                // Paused (focus lost): ignore host input.
                            } else {
                            app.set_cursor_output_pos(position);
                            let Some((x_u, y_u)) = app.cursor_internal else {
                                if let Some(id) = app.hovered_app.take() {
                                    let _ = app.send_app_msg(id, protocol::Msg::mouse_leave());
                                }
                                app.drag = None;
                                window.request_redraw();
                                return;
                            };
                            let (x, y) = (x_u as i32, y_u as i32);

                            if let Some(drag) = app.drag {
                                if app.mouse_left_down {
                                    if let Some(win) =
                                        app.windows.iter_mut().find(|w| w.id == drag.window_id)
                                    {
                                        let max_x = (INTERNAL_W as i32 - win.rect.w).max(0);
                                        let max_y = (INTERNAL_H as i32 - win.rect.h).max(0);
                                        win.rect.x = (x - drag.grab_dx).clamp(0, max_x);
                                        win.rect.y = (y - drag.grab_dy).clamp(0, max_y);
                                    } else {
                                        app.drag = None;
                                    }
                                } else {
                                    app.drag = None;
                                }
                            }

                            let new_hover = app.client_window_at_point(x, y);
                            if new_hover != app.hovered_app {
                                if let Some(old) = app.hovered_app {
                                    let _ = app.send_app_msg(old, protocol::Msg::mouse_leave());
                                }
                                if let Some(new_id) = new_hover {
                                    let _ = app.send_app_msg(new_id, protocol::Msg::mouse_enter());
                                }
                                app.hovered_app = new_hover;
                            }

                            let target = app.mouse_capture_app.or(app.hovered_app);
                            if let Some(id) = target {
                                if let (Some(win), Some(sess)) = (
                                    app.windows.iter().find(|w| w.id == id),
                                    app.temple_apps.get(&id),
                                ) {
                                    let (mx, my) = if app.mouse_capture_app == Some(id) {
                                        app.map_point_to_app_coords_clamped(win, sess, x, y)
                                    } else {
                                        match app.map_point_to_app_coords(win, sess, x, y) {
                                            Some(v) => v,
                                            None => {
                                                window.request_redraw();
                                                return;
                                            }
                                        }
                                    };
                                    let _ = app.send_app_msg(id, protocol::Msg::mouse_move(mx, my));
                                }
                            }

                            window.request_redraw();
                            }
                        }
                        WindowEvent::CursorLeft { .. } => {
                            if app.test.is_some() {
                                // In golden-test mode, ignore real pointer events.
                            } else if !app.window_focused {
                                // Paused (focus lost): ignore host input.
                            } else {
                            app.cursor_internal = None;
                            app.drag = None;
                            app.mouse_left_down = false;
                            app.update_status_line();
                            if let Some(id) = app.hovered_app.take() {
                                let _ = app.send_app_msg(id, protocol::Msg::mouse_leave());
                            }
                            app.mouse_capture_app = None;
                            window.request_redraw();
                            }
                        }
                        WindowEvent::MouseInput { state, button, .. } => {
                            if app.test.is_some() {
                                // In golden-test mode, ignore real pointer events.
                            } else if !app.window_focused {
                                // Paused (focus lost): ignore host input.
                            } else {
                            let down = state == ElementState::Pressed;
                            if button == MouseButton::Left {
                                app.mouse_left_down = down;
                                if !down {
                                    app.drag = None;
                                }
                            }

                            let Some((x_u, y_u)) = app.cursor_internal else {
                                if !down {
                                    app.mouse_capture_app = None;
                                }
                                return;
                            };
                            let (x, y) = (x_u as i32, y_u as i32);

                            let temple_button = map_winit_mouse_button_to_temple_button(button);

                            if down {
                                if let Some((idx, hit)) = app.hit_test_windows(x, y) {
                                    let id = app.windows[idx].id;
                                    app.bring_window_to_front(idx);
                                    app.update_status_line();

                                    match hit {
                                        WindowHit::Close if button == MouseButton::Left => {
                                            app.close_window(id);
                                        }
                                        WindowHit::TitleBar if button == MouseButton::Left => {
                                            if let Some(win) =
                                                app.windows.iter().find(|w| w.id == id)
                                            {
                                                app.drag = Some(DragState {
                                                    window_id: id,
                                                    grab_dx: x - win.rect.x,
                                                    grab_dy: y - win.rect.y,
                                                });
                                            }
                                        }
                                        WindowHit::Client => {
                                            app.mouse_capture_app = Some(id);

                                            if app.hovered_app != Some(id) {
                                                if let Some(old) = app.hovered_app {
                                                    let _ = app.send_app_msg(
                                                        old,
                                                        protocol::Msg::mouse_leave(),
                                                    );
                                                }
                                                let _ = app
                                                    .send_app_msg(id, protocol::Msg::mouse_enter());
                                                app.hovered_app = Some(id);
                                            }

                                            if let (Some(win), Some(sess)) = (
                                                app.windows.iter().find(|w| w.id == id),
                                                app.temple_apps.get(&id),
                                            ) {
                                                let (mx, my) = app.map_point_to_app_coords_clamped(
                                                    win, sess, x, y,
                                                );
                                                let _ = app.send_app_msg(
                                                    id,
                                                    protocol::Msg::mouse_move(mx, my),
                                                );
                                                let _ = app.send_app_msg(
                                                    id,
                                                    protocol::Msg::mouse_button(
                                                        temple_button,
                                                        true,
                                                    ),
                                                );
                                            }
                                        }
                                        _ => {}
                                    }

                                    window.request_redraw();
                                    return;
                                }

                                // Background click: focus shell.
                                app.focused_app = None;
                                app.drag = None;
                                app.mouse_capture_app = None;
                                app.update_status_line();

                                if button == MouseButton::Left {
                                    if app.shell.in_browser() {
                                        let row = y_u / FONT_H;
                                        if let Some(idx) =
                                            app.shell.browser_click_row(row, &mut app.terminal)
                                        {
                                            let now = std::time::Instant::now();
                                            let is_double = app
                                                .browser_last_click
                                                .as_ref()
                                                .is_some_and(|(t, last_idx)| {
                                                    *last_idx == idx
                                                        && now.duration_since(*t)
                                                            <= std::time::Duration::from_millis(400)
                                                });

                                            if is_double {
                                                app.browser_last_click = None;
                                                let _ = app.shell.handle_key(
                                                    &Key::Named(NamedKey::Enter),
                                                    &mut app.terminal,
                                                );
                                            } else {
                                                app.browser_last_click = Some((now, idx));
                                            }
                                        }
                                    } else if app.shell.in_doc_viewer() {
                                        let col = x_u / FONT_W;
                                        let row = y_u / FONT_H;
                                        if let Some(idx) =
                                            app.shell.doc_click_cell(col, row, &mut app.terminal)
                                        {
                                            let now = std::time::Instant::now();
                                            let is_double = app
                                                .browser_last_click
                                                .as_ref()
                                                .is_some_and(|(t, last_idx)| {
                                                    *last_idx == idx
                                                        && now.duration_since(*t)
                                                            <= std::time::Duration::from_millis(400)
                                                });

                                            if is_double {
                                                app.browser_last_click = None;
                                                let _ = app.shell.handle_key(
                                                    &Key::Named(NamedKey::Enter),
                                                    &mut app.terminal,
                                                );
                                            } else {
                                                app.browser_last_click = Some((now, idx));
                                            }
                                        }
                                    }
                                }

                                window.request_redraw();
                                return;
                            }

                            // Button release: send to capture app (if any).
                            if let Some(id) = app.mouse_capture_app {
                                if let (Some(win), Some(sess)) = (
                                    app.windows.iter().find(|w| w.id == id),
                                    app.temple_apps.get(&id),
                                ) {
                                    let (mx, my) =
                                        app.map_point_to_app_coords_clamped(win, sess, x, y);
                                    let _ = app.send_app_msg(id, protocol::Msg::mouse_move(mx, my));
                                    let _ = app.send_app_msg(
                                        id,
                                        protocol::Msg::mouse_button(temple_button, false),
                                    );
                                }
                                app.mouse_capture_app = None;
                                window.request_redraw();
                            }
                            }
                        }
                        WindowEvent::MouseWheel { delta, .. } => {
                            if app.test.is_some() {
                                // In golden-test mode, ignore real pointer events.
                            } else if !app.window_focused {
                                // Paused (focus lost): ignore host input.
                            } else {
                            let mut sent_to_app = false;
                            if let Some((x_u, y_u)) = app.cursor_internal {
                                let x = x_u as i32;
                                let y = y_u as i32;
                                if let Some(id) = app.client_window_at_point(x, y) {
                                    let (dx, dy) = match delta {
                                        MouseScrollDelta::LineDelta(x, y) => {
                                            ((x * 8.0).round() as i32, (y * 8.0).round() as i32)
                                        }
                                        MouseScrollDelta::PixelDelta(pos) => {
                                            let lb = app.letterbox();
                                            (
                                                (pos.x / lb.scale_x).round() as i32,
                                                (pos.y / lb.scale_y).round() as i32,
                                            )
                                        }
                                    };
                                    let _ =
                                        app.send_app_msg(id, protocol::Msg::mouse_wheel(dx, dy));
                                    sent_to_app = true;
                                    window.request_redraw();
                                }
                            }

                            if sent_to_app {
                                return;
                            }

                            let (dy, lines) = match delta {
                                MouseScrollDelta::LineDelta(_, y) => {
                                    let steps = y.abs().round() as usize;
                                    (y, steps.saturating_mul(3).max(1))
                                }
                                MouseScrollDelta::PixelDelta(pos) => {
                                    let steps = (pos.y.abs() / 32.0).round() as usize;
                                    (pos.y as f32, steps.max(1))
                                }
                            };

                            if app.shell.in_doc_viewer() {
                                if dy > 0.0 {
                                    app.shell.doc_wheel(lines as isize, &mut app.terminal);
                                } else if dy < 0.0 {
                                    app.shell.doc_wheel(-(lines as isize), &mut app.terminal);
                                }
                            } else if app.shell.in_browser() {
                                if dy > 0.0 {
                                    app.shell.browser_wheel(lines as isize, &mut app.terminal);
                                } else if dy < 0.0 {
                                    app.shell
                                        .browser_wheel(-(lines as isize), &mut app.terminal);
                                }
                            } else if dy > 0.0 {
                                app.terminal.scroll_view_up(lines);
                            } else if dy < 0.0 {
                                app.terminal.scroll_view_down(lines);
                            }
                            app.update_status_line();
                            window.request_redraw();
                            }
                        }
                        _ => {}
                    }

                    if app.test_should_exit() {
                        elwt.exit();
                    }
                }
                _ => {}
            }

            if app.step_test_run_shell() {
                window.request_redraw();
            }
        })
        .expect("run event loop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_integer_scale_with_letterbox() {
        let lb = Letterbox::new(1920, 1080);
        assert_eq!(lb.dest_x, 320);
        assert_eq!(lb.dest_y, 60);
        assert_eq!(lb.dest_w, 1280);
        assert_eq!(lb.dest_h, 960);
        assert_eq!(lb.scale_x, 2.0);
        assert_eq!(lb.scale_y, 2.0);

        assert_eq!(
            lb.map_point_to_internal(PhysicalPosition::new(320.0, 60.0)),
            Some((0, 0))
        );
        assert_eq!(
            lb.map_point_to_internal(PhysicalPosition::new(1599.0, 1019.0)),
            Some((639, 479))
        );
        assert_eq!(
            lb.map_point_to_internal(PhysicalPosition::new(0.0, 0.0)),
            None
        );
    }

    #[test]
    fn letterbox_integer_scale_no_letterbox() {
        let lb = Letterbox::new(1280, 960);
        assert_eq!(lb.dest_x, 0);
        assert_eq!(lb.dest_y, 0);
        assert_eq!(lb.dest_w, 1280);
        assert_eq!(lb.dest_h, 960);
        assert_eq!(lb.scale_x, 2.0);
        assert_eq!(lb.scale_y, 2.0);

        assert_eq!(
            lb.map_point_to_internal(PhysicalPosition::new(0.0, 0.0)),
            Some((0, 0))
        );
        assert_eq!(
            lb.map_point_to_internal(PhysicalPosition::new(1279.0, 959.0)),
            Some((639, 479))
        );
    }

    #[test]
    fn letterbox_downscale_when_output_smaller_than_internal() {
        let lb = Letterbox::new(320, 240);
        assert_eq!(lb.dest_x, 0);
        assert_eq!(lb.dest_y, 0);
        assert_eq!(lb.dest_w, 320);
        assert_eq!(lb.dest_h, 240);
        assert!((lb.scale_x - 0.5).abs() < f64::EPSILON);
        assert!((lb.scale_y - 0.5).abs() < f64::EPSILON);

        assert_eq!(
            lb.map_point_to_internal(PhysicalPosition::new(0.0, 0.0)),
            Some((0, 0))
        );
        assert_eq!(
            lb.map_point_to_internal(PhysicalPosition::new(319.0, 239.0)),
            Some((638, 478))
        );
    }

    #[test]
    fn templeos_root_is_discoverable_in_repo() {
        let Some(root) = discover_templeos_root() else {
            panic!("expected TempleOS tree to be discoverable (third_party/TempleOS)");
        };
        assert!(root.join("Kernel/FontStd.HC").exists());
        assert!(root.join("Demo/Print.HC").exists());
    }

    #[test]
    fn templeos_program_index_includes_print_and_resolves_alias() {
        let root = discover_templeos_root().expect("TempleOS tree");
        let programs = discover_templeos_programs(&root);
        assert!(
            programs
                .iter()
                .any(|p| p.rel_no_ext.eq("Demo/Print") && p.alias.eq("Print")),
            "expected Demo/Print.HC to be indexed"
        );

        let spec = resolve_templeos_program_spec(&programs, "Print").expect("resolve Print");
        assert_eq!(spec, "::/Demo/Print.HC");

        let spec = resolve_templeos_program_spec(&programs, "Demo/Print.HC").expect("resolve");
        assert_eq!(spec, "::/Demo/Print.HC");
    }
}
