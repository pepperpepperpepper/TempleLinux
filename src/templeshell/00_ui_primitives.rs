#[derive(Clone, Copy, Debug, Default)]
struct ModState {
    ctrl: bool,
    shift: bool,
    alt: bool,
    super_key: bool,
}

impl ModState {
    fn update(&mut self, key: &Key, down: bool) {
        match key {
            Key::Named(NamedKey::Control) => self.ctrl = down,
            Key::Named(NamedKey::Shift) => self.shift = down,
            Key::Named(NamedKey::Alt) => self.alt = down,
            Key::Named(NamedKey::Super) => self.super_key = down,
            _ => {}
        }
    }

    fn is_paste(&self, key: &Key) -> bool {
        if self.ctrl {
            if let Key::Character(s) = key {
                if s.eq_ignore_ascii_case("v") {
                    return true;
                }
            }
        }

        self.shift && matches!(key, Key::Named(NamedKey::Insert))
    }
}

#[derive(Default)]
struct HostClipboard {
    inner: Option<Clipboard>,
}

impl HostClipboard {
    fn ensure(&mut self) -> Result<&mut Clipboard, String> {
        if self.inner.is_none() {
            self.inner = Some(Clipboard::new().map_err(|err| err.to_string())?);
        }
        self.inner
            .as_mut()
            .ok_or_else(|| "clipboard: unavailable".to_string())
    }

    fn get_text(&mut self) -> Result<String, String> {
        self.ensure()?.get_text().map_err(|err| err.to_string())
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        self.ensure()?
            .set_text(text.to_string())
            .map_err(|err| err.to_string())
    }
}

#[derive(Debug, Default)]
struct CliArgs {
    no_fullscreen: bool,
    test_dump_initial_png: Option<PathBuf>,
    test_dump_after_first_app_present_png: Option<PathBuf>,
    test_dump_after_n_apps_present_png: Option<(usize, PathBuf)>,
    test_dump_after_n_presents_png: Option<(usize, PathBuf)>,
    test_app_exit: Option<TestAppExit>,
    test_send_after_first_app_present: Vec<protocol::Msg>,
    test_run_shell: Vec<String>,
}

#[derive(Debug)]
enum UserEvent {
    Ipc(TempleIpcEvent),
}

type AppId = u32;

#[derive(Debug)]
enum TempleIpcEvent {
    AppConnected {
        id: AppId,
        shm: File,
        width: u32,
        height: u32,
        cmd_tx: mpsc::Sender<protocol::Msg>,
    },
    AppPresent {
        id: AppId,
        seq: u32,
    },
    AppDisconnected {
        id: AppId,
    },
    PaletteColorSet {
        id: AppId,
        color_index: u8,
        rgba: [u8; 4],
    },
    SettingsPush {
        id: AppId,
    },
    SettingsPop {
        id: AppId,
    },
    Log(String),
    ClipboardSet {
        id: AppId,
        text: String,
    },
}

struct Framebuffer {
    indices: Vec<u8>,
}

impl Framebuffer {
    fn new() -> Self {
        Self {
            indices: vec![0; (INTERNAL_W * INTERNAL_H) as usize],
        }
    }

    fn put_pixel(&mut self, x: u32, y: u32, color: u8) {
        if x >= INTERNAL_W || y >= INTERNAL_H {
            return;
        }
        let idx = (y * INTERNAL_W + x) as usize;
        self.indices[idx] = color;
    }

    #[allow(dead_code)]
    fn put_pixel_i32(&mut self, x: i32, y: i32, color: u8) {
        if x < 0 || y < 0 {
            return;
        }
        self.put_pixel(x as u32, y as u32, color);
    }

    #[allow(dead_code)]
    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u8) {
        let x1 = (x + w).min(INTERNAL_W);
        let y1 = (y + h).min(INTERNAL_H);
        for yy in y..y1 {
            let row_start = (yy * INTERNAL_W) as usize;
            let start = row_start + x as usize;
            let end = row_start + x1 as usize;
            self.indices[start..end].fill(color);
        }
    }

    fn to_rgba(&self, palette: &[[u8; 4]; 256], out: &mut [u8]) {
        assert_eq!(out.len(), (INTERNAL_W * INTERNAL_H * 4) as usize);
        for (i, &px) in self.indices.iter().enumerate() {
            let dst = i * 4;
            out[dst..dst + 4].copy_from_slice(&palette[px as usize]);
        }
    }
}

fn write_png_rgba(path: &Path, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|err| io::Error::other(err.to_string()))?;
    writer
        .write_image_data(rgba)
        .map_err(|err| io::Error::other(err.to_string()))?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct Cell {
    ch: u8,
    fg: u8,
    bg: u8,
}

struct Terminal {
    cells: Vec<Cell>,
    cursor_col: u32,
    cursor_row: u32,
    fg: u8,
    bg: u8,
    scroll_rows: u32,
    scrollback: std::collections::VecDeque<Vec<Cell>>,
    view_offset: usize,
    scrollback_max: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalRenderMode {
    Opaque,
    OverWallpaper,
}

impl Terminal {
    fn new(fg: u8, bg: u8, scroll_rows: u32) -> Self {
        let scroll_rows = scroll_rows.clamp(1, TERM_ROWS);
        let blank = Cell { ch: b' ', fg, bg };
        Self {
            cells: vec![blank; (TERM_COLS * TERM_ROWS) as usize],
            cursor_col: 0,
            cursor_row: 0,
            fg,
            bg,
            scroll_rows,
            scrollback: std::collections::VecDeque::new(),
            view_offset: 0,
            scrollback_max: 2000,
        }
    }

    fn clear_output(&mut self) {
        let blank = Cell {
            ch: b' ',
            fg: self.fg,
            bg: self.bg,
        };
        let region_cells = (self.scroll_rows * TERM_COLS) as usize;
        self.cells[..region_cells].fill(blank);
        self.cursor_col = 0;
        self.cursor_row = 0;
        self.scrollback.clear();
        self.view_offset = 0;
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        let blank = Cell {
            ch: b' ',
            fg: self.fg,
            bg: self.bg,
        };
        self.cells.fill(blank);
        self.cursor_col = 0;
        self.cursor_row = 0;
    }

    #[allow(dead_code)]
    fn set_colors(&mut self, fg: u8, bg: u8) {
        self.fg = fg;
        self.bg = bg;
    }

    fn write_at(&mut self, col: u32, row: u32, fg: u8, bg: u8, text: &str) {
        if row >= TERM_ROWS {
            return;
        }
        let mut c = col.min(TERM_COLS);
        for ch in text.chars() {
            if c >= TERM_COLS {
                break;
            }
            let b = sanitize_ascii(ch);
            let idx = self.idx(c, row);
            self.cells[idx] = Cell { ch: b, fg, bg };
            c += 1;
        }
    }

    fn fill_row(&mut self, row: u32, fg: u8, bg: u8) {
        if row >= TERM_ROWS {
            return;
        }
        for col in 0..TERM_COLS {
            let idx = self.idx(col, row);
            self.cells[idx] = Cell { ch: b' ', fg, bg };
        }
    }

    fn newline(&mut self) {
        self.cursor_col = 0;
        self.cursor_row += 1;
        if self.cursor_row >= self.scroll_rows {
            self.scroll_up(1);
            self.cursor_row = self.scroll_rows - 1;
        }
    }

    fn scroll_up(&mut self, lines: u32) {
        let lines = lines.min(self.scroll_rows);
        let region_cells = (self.scroll_rows * TERM_COLS) as usize;
        let shift = (lines * TERM_COLS) as usize;
        if shift >= region_cells {
            let blank = Cell {
                ch: b' ',
                fg: self.fg,
                bg: self.bg,
            };
            self.cells[..region_cells].fill(blank);
            self.cursor_row = 0;
            self.cursor_col = 0;
            self.scrollback.clear();
            self.view_offset = 0;
            return;
        }

        // Save the scrolled-off lines into scrollback.
        for row in 0..lines {
            let row = row as usize;
            let start = row * TERM_COLS as usize;
            let end = start + TERM_COLS as usize;
            self.scrollback.push_back(self.cells[start..end].to_vec());
            while self.scrollback.len() > self.scrollback_max {
                self.scrollback.pop_front();
            }
        }
        if self.view_offset > 0 {
            self.view_offset = (self.view_offset + lines as usize).min(self.scrollback.len());
        }

        self.cells.copy_within(shift..region_cells, 0);
        let blank = Cell {
            ch: b' ',
            fg: self.fg,
            bg: self.bg,
        };
        self.cells[region_cells - shift..region_cells].fill(blank);
        self.cursor_row = self.cursor_row.saturating_sub(lines);
    }

    fn put_char(&mut self, ch: char) {
        match ch {
            '\n' => self.newline(),
            '\r' => self.cursor_col = 0,
            '\t' => {
                let tab = 4;
                let next = ((self.cursor_col / tab) + 1) * tab;
                while self.cursor_col < next {
                    self.put_char(' ');
                }
            }
            _ => {
                let b = sanitize_ascii(ch);
                let idx = self.idx(self.cursor_col, self.cursor_row);
                self.cells[idx] = Cell {
                    ch: b,
                    fg: self.fg,
                    bg: self.bg,
                };
                self.cursor_col += 1;
                if self.cursor_col >= TERM_COLS {
                    self.newline();
                }
            }
        }
    }

    fn render(&self, fb: &mut Framebuffer, mode: TerminalRenderMode) {
        let scroll_rows = self.scroll_rows.min(TERM_ROWS);
        let history_total = self.scrollback.len() + scroll_rows as usize;
        let start = history_total.saturating_sub(scroll_rows as usize + self.view_offset);

        for row in 0..scroll_rows {
            let row_usize = row as usize;
            let line_idx = start + row_usize;
            let src = if line_idx < self.scrollback.len() {
                Some(&self.scrollback[line_idx][..])
            } else {
                let cur_row = line_idx.saturating_sub(self.scrollback.len());
                if cur_row < scroll_rows as usize {
                    let start = cur_row * TERM_COLS as usize;
                    let end = start + TERM_COLS as usize;
                    Some(&self.cells[start..end])
                } else {
                    None
                }
            };

            for col in 0..TERM_COLS {
                let cell = src
                    .and_then(|row| row.get(col as usize))
                    .copied()
                    .unwrap_or(Cell {
                        ch: b' ',
                        fg: self.fg,
                        bg: self.bg,
                    });
                draw_cell_8x8(fb, col, row, cell, mode);
            }
        }

        for row in scroll_rows..TERM_ROWS {
            for col in 0..TERM_COLS {
                let cell = self.cells[self.idx(col, row)];
                draw_cell_8x8(fb, col, row, cell, mode);
            }
        }
    }

    fn idx(&self, col: u32, row: u32) -> usize {
        (row * TERM_COLS + col) as usize
    }

    fn invert_cell(&mut self, col: u32, row: u32) {
        if col >= TERM_COLS || row >= TERM_ROWS {
            return;
        }
        let idx = self.idx(col, row);
        let mut cell = self.cells[idx];
        std::mem::swap(&mut cell.fg, &mut cell.bg);
        self.cells[idx] = cell;
    }

    fn scroll_view_up(&mut self, lines: usize) {
        self.view_offset = (self.view_offset + lines).min(self.scrollback.len());
    }

    fn scroll_view_down(&mut self, lines: usize) {
        self.view_offset = self.view_offset.saturating_sub(lines);
    }

    fn scroll_view_to_top(&mut self) {
        self.view_offset = self.scrollback.len();
    }

    fn scroll_view_to_bottom(&mut self) {
        self.view_offset = 0;
    }

    fn view_offset(&self) -> usize {
        self.view_offset
    }
}

impl fmt::Write for Terminal {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.chars() {
            self.put_char(ch);
        }
        Ok(())
    }
}

fn sanitize_ascii(ch: char) -> u8 {
    assets::encode_cp437(ch)
}

fn draw_cell_8x8(fb: &mut Framebuffer, col: u32, row: u32, cell: Cell, mode: TerminalRenderMode) {
    let x = col * FONT_W;
    let y = row * FONT_H;
    match mode {
        TerminalRenderMode::Opaque => draw_char_8x8(fb, x, y, cell.ch, cell.fg, cell.bg),
        TerminalRenderMode::OverWallpaper => {
            draw_char_8x8_over_wallpaper(fb, x, y, cell.ch, cell.fg, cell.bg)
        }
    }
}

fn draw_char_8x8(fb: &mut Framebuffer, x: u32, y: u32, ch: u8, fg: u8, bg: u8) {
    for row in 0..8u32 {
        let row_bits = assets::sys_font_std_glyph_row_bits(ch, row as u8);
        for col in 0..8u32 {
            let on = (row_bits & (1u8 << col as u8)) != 0;
            fb.put_pixel(x + col, y + row, if on { fg } else { bg });
        }
    }
}

fn draw_char_8x8_over_wallpaper(fb: &mut Framebuffer, x: u32, y: u32, ch: u8, fg: u8, bg: u8) {
    // Dither black background pixels so a wallpaper can still show through while keeping text readable.
    // Only applies to black background (bg==0); other backgrounds remain fully opaque.
    if bg != 0 {
        draw_char_8x8(fb, x, y, ch, fg, bg);
        return;
    }

    for row in 0..8u32 {
        let row_bits = assets::sys_font_std_glyph_row_bits(ch, row as u8);
        for col in 0..8u32 {
            let px = x + col;
            let py = y + row;
            let on = (row_bits & (1u8 << col as u8)) != 0;
            if on {
                fb.put_pixel(px, py, fg);
            } else if ((px + py) & 1) == 0 {
                fb.put_pixel(px, py, 0);
            }
        }
    }
}

fn fill_rect_i32(fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, color: u8) {
    if w <= 0 || h <= 0 {
        return;
    }
    let x0 = x.max(0).min(INTERNAL_W as i32) as u32;
    let y0 = y.max(0).min(INTERNAL_H as i32) as u32;
    let x1 = (x + w).max(0).min(INTERNAL_W as i32) as u32;
    let y1 = (y + h).max(0).min(INTERNAL_H as i32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for yy in y0..y1 {
        let row_start = (yy * INTERNAL_W) as usize;
        let start = row_start + x0 as usize;
        let end = row_start + x1 as usize;
        fb.indices[start..end].fill(color);
    }
}

fn draw_text_8x8(fb: &mut Framebuffer, x: i32, y: i32, fg: u8, bg: u8, text: &str) {
    let mut cx = x;
    for ch in text.chars() {
        if ch == '\n' {
            cx = x;
            continue;
        }
        if cx >= -7 && y >= -7 {
            let b = sanitize_ascii(ch);
            if cx >= 0 && y >= 0 {
                draw_char_8x8(fb, cx as u32, y as u32, b, fg, bg);
            }
        }
        cx += 8;
    }
}

fn blit_scaled_indices(fb: &mut Framebuffer, dst: RectI32, src: &[u8], src_w: u32, src_h: u32) {
    if dst.w <= 0 || dst.h <= 0 {
        return;
    }

    let dst_x0 = dst.x.max(0).min(INTERNAL_W as i32);
    let dst_y0 = dst.y.max(0).min(INTERNAL_H as i32);
    let dst_x1 = (dst.x + dst.w).max(0).min(INTERNAL_W as i32);
    let dst_y1 = (dst.y + dst.h).max(0).min(INTERNAL_H as i32);

    if dst_x0 >= dst_x1 || dst_y0 >= dst_y1 {
        return;
    }

    let dst_w = dst.w as i64;
    let dst_h = dst.h as i64;
    if dst_w <= 0 || dst_h <= 0 {
        return;
    }

    for y in dst_y0..dst_y1 {
        let local_y = (y - dst.y) as i64;
        let src_y =
            ((local_y * src_h as i64) / dst_h).clamp(0, src_h.saturating_sub(1) as i64) as u32;

        let dst_row = (y as u32 * INTERNAL_W) as usize;
        let src_row = (src_y * src_w) as usize;
        for x in dst_x0..dst_x1 {
            let local_x = (x - dst.x) as i64;
            let src_x =
                ((local_x * src_w as i64) / dst_w).clamp(0, src_w.saturating_sub(1) as i64) as u32;
            let dst_idx = dst_row + x as usize;
            let src_idx = src_row + src_x as usize;
            if let Some(&px) = src.get(src_idx) {
                fb.indices[dst_idx] = px;
            }
        }
    }
}

fn draw_software_cursor(fb: &mut Framebuffer, x: u32, y: u32) {
    const BORDER: [u8; CURSOR_H as usize] = [
        0b00000001, 0b00000011, 0b00000101, 0b00001001, 0b00010001, 0b00100001, 0b01000001,
        0b11111111,
    ];
    const FILL: [u8; CURSOR_H as usize] = [
        0b00000000, 0b00000000, 0b00000010, 0b00000110, 0b00001110, 0b00011110, 0b00111110,
        0b00000000,
    ];

    for row in 0..CURSOR_H {
        let border = BORDER[row as usize];
        let fill = FILL[row as usize];
        for col in 0..CURSOR_W {
            let bit = 1u8 << col;
            if (border & bit) != 0 {
                fb.put_pixel(x + col, y + row, CURSOR_BORDER_COLOR);
            } else if (fill & bit) != 0 {
                fb.put_pixel(x + col, y + row, CURSOR_FILL_COLOR);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Letterbox {
    dest_x: u32,
    dest_y: u32,
    dest_w: u32,
    dest_h: u32,
    scale_x: f64,
    scale_y: f64,
}

impl Letterbox {
    fn new(output_w: u32, output_h: u32) -> Self {
        let scale_x = output_w / INTERNAL_W;
        let scale_y = output_h / INTERNAL_H;
        let scale = scale_x.min(scale_y);

        if scale >= 1 {
            let dest_w = INTERNAL_W * scale;
            let dest_h = INTERNAL_H * scale;

            let dest_x = (output_w - dest_w) / 2;
            let dest_y = (output_h - dest_h) / 2;

            return Self {
                dest_x,
                dest_y,
                dest_w,
                dest_h,
                scale_x: scale as f64,
                scale_y: scale as f64,
            };
        }

        // If the output is smaller than the internal size, fall back to nearest-neighbor
        // downscaling to fit.
        Self {
            dest_x: 0,
            dest_y: 0,
            dest_w: output_w,
            dest_h: output_h,
            scale_x: output_w as f64 / INTERNAL_W as f64,
            scale_y: output_h as f64 / INTERNAL_H as f64,
        }
    }

    fn map_point_to_internal(self, output_pos: PhysicalPosition<f64>) -> Option<(u32, u32)> {
        let output_x = output_pos.x.floor();
        let output_y = output_pos.y.floor();
        if !output_x.is_finite() || !output_y.is_finite() {
            return None;
        }

        let output_x = output_x as i32;
        let output_y = output_y as i32;

        let x0 = self.dest_x as i32;
        let y0 = self.dest_y as i32;
        let x1 = x0 + self.dest_w as i32;
        let y1 = y0 + self.dest_h as i32;

        if output_x < x0 || output_x >= x1 || output_y < y0 || output_y >= y1 {
            return None;
        }

        let local_x = (output_x - x0) as u32;
        let local_y = (output_y - y0) as u32;

        let internal_x =
            ((local_x as f64 / self.scale_x).floor() as i32).clamp(0, INTERNAL_W as i32 - 1) as u32;
        let internal_y =
            ((local_y as f64 / self.scale_y).floor() as i32).clamp(0, INTERNAL_H as i32 - 1) as u32;
        Some((internal_x, internal_y))
    }
}
