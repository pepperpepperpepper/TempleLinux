use std::{
    fs::File,
    io::{self, Write as _},
    os::{
        fd::IntoRawFd as _,
        unix::{io::FromRawFd as _, net::UnixStream},
    },
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crate::assets;
use crate::protocol::{self, Msg};

pub struct TempleRt {
    width: u32,
    height: u32,
    fb: memmap2::MmapMut,
    stream: UnixStream,
    events: mpsc::Receiver<Event>,
    present_acks: mpsc::Receiver<u32>,
    present_seq: u32,
    clip: ClipRect,
    font_u64: [u64; 256],
    sync_present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Key { code: u32, down: bool },
    MouseMove { x: u32, y: u32 },
    MouseButton { button: u32, down: bool },
    MouseWheel { dx: i32, dy: i32 },
    MouseEnter,
    MouseLeave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClipRect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl ClipRect {
    fn full(width: u32, height: u32) -> Self {
        Self {
            x0: 0,
            y0: 0,
            x1: width,
            y1: height,
        }
    }
}

impl TempleRt {
    pub fn connect() -> io::Result<Self> {
        let sock = std::env::var("TEMPLE_SOCK").map_err(|_| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "TEMPLE_SOCK is not set (expected TempleShell to provide it)",
            )
        })?;

        let mut stream = UnixStream::connect(sock)?;
        protocol::write_msg(&mut stream, Msg::hello())?;

        let (ack, shm_fd) = protocol::recv_msg_with_fd(&mut stream)?;
        if ack.kind != protocol::MSG_HELLO_ACK {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "temple-rt: expected HELLO_ACK",
            ));
        }
        let width = ack.a;
        let height = ack.b;

        let Some(shm_fd) = shm_fd else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "temple-rt: missing shm fd in HELLO_ACK",
            ));
        };

        let shm_len = width
            .checked_mul(height)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "temple-rt: bad size"))?
            as usize;

        let file = unsafe { File::from_raw_fd(shm_fd.into_raw_fd()) };
        let fb = unsafe { memmap2::MmapOptions::new().len(shm_len).map_mut(&file)? };

        let mut reader = stream.try_clone()?;
        let (tx, rx) = mpsc::channel();
        let (ack_tx, ack_rx) = mpsc::channel::<u32>();
        thread::spawn(move || {
            loop {
                let msg = match protocol::read_msg(&mut reader) {
                    Ok(m) => m,
                    Err(_) => break,
                };
                match msg.kind {
                    protocol::MSG_PRESENT_ACK => {
                        let _ = ack_tx.send(msg.a);
                    }
                    protocol::MSG_KEY => {
                        let _ = tx.send(Event::Key {
                            code: msg.a,
                            down: msg.b == protocol::KEY_STATE_DOWN,
                        });
                    }
                    protocol::MSG_MOUSE_MOVE => {
                        let _ = tx.send(Event::MouseMove { x: msg.a, y: msg.b });
                    }
                    protocol::MSG_MOUSE_BUTTON => {
                        let _ = tx.send(Event::MouseButton {
                            button: msg.a,
                            down: msg.b == protocol::KEY_STATE_DOWN,
                        });
                    }
                    protocol::MSG_MOUSE_WHEEL => {
                        let _ = tx.send(Event::MouseWheel {
                            dx: msg.a as i32,
                            dy: msg.b as i32,
                        });
                    }
                    protocol::MSG_MOUSE_ENTER => {
                        let _ = tx.send(Event::MouseEnter);
                    }
                    protocol::MSG_MOUSE_LEAVE => {
                        let _ = tx.send(Event::MouseLeave);
                    }
                    protocol::MSG_SHUTDOWN => break,
                    _ => {}
                }
            }
        });

        Ok(Self {
            width,
            height,
            fb,
            stream,
            events: rx,
            present_acks: ack_rx,
            present_seq: 0,
            clip: ClipRect::full(width, height),
            font_u64: assets::TEMPLEOS_SYS_FONT_STD_U64,
            sync_present: env_truthy("TEMPLE_SYNC_PRESENT"),
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn reset_clip_rect(&mut self) {
        self.clip = ClipRect::full(self.width, self.height);
    }

    pub fn set_clip_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            self.clip = ClipRect {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            };
            return;
        }

        let x0 = x.max(0) as i64;
        let y0 = y.max(0) as i64;
        let x1 = (x as i64 + w as i64).max(0);
        let y1 = (y as i64 + h as i64).max(0);

        let x0 = (x0 as u32).min(self.width);
        let y0 = (y0 as u32).min(self.height);
        let x1 = (x1 as u32).min(self.width);
        let y1 = (y1 as u32).min(self.height);

        self.clip = ClipRect { x0, y0, x1, y1 };
    }

    pub fn framebuffer_mut(&mut self) -> &mut [u8] {
        &mut self.fb
    }

    pub fn clear(&mut self, color: u8) {
        self.fb.fill(color);
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, color: u8) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if x < self.clip.x0 || x >= self.clip.x1 || y < self.clip.y0 || y >= self.clip.y1 {
            return;
        }
        let idx = (y * self.width + x) as usize;
        self.fb[idx] = color;
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        if w <= 0 || h <= 0 {
            return;
        }
        let x0 = x.max(self.clip.x0 as i32).max(0) as i64;
        let y0 = y.max(self.clip.y0 as i32).max(0) as i64;
        let x1 = (x as i64 + w as i64)
            .max(0)
            .min(self.clip.x1 as i64)
            .min(self.width as i64);
        let y1 = (y as i64 + h as i64)
            .max(0)
            .min(self.clip.y1 as i64)
            .min(self.height as i64);

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let x0 = x0 as u32;
        let y0 = y0 as u32;
        let x1 = x1 as u32;
        let y1 = y1 as u32;

        for yy in y0..y1 {
            let row = (yy * self.width) as usize;
            let start = row + x0 as usize;
            let end = row + x1 as usize;
            self.fb[start..end].fill(color);
        }
    }

    pub fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) {
        self.draw_line_thick(x1, y1, x2, y2, color, 1);
    }

    pub fn draw_line_thick(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8, thick: i32) {
        let thick = thick.max(1);

        let mut x = x1;
        let mut y = y1;
        let dx = (x2 - x1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let dy = -(y2 - y1).abs();
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if thick == 1 {
                self.set_pixel(x, y, color);
            } else {
                let half = thick / 2;
                self.fill_rect(x - half, y - half, thick, thick, color);
            }

            if x == x2 && y == y2 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn draw_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        self.draw_rect_outline_thick(x, y, w, h, color, 1);
    }

    pub fn draw_rect_outline_thick(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: u8,
        thick: i32,
    ) {
        if w <= 0 || h <= 0 {
            return;
        }
        let thick = thick.max(1);
        if thick * 2 >= w || thick * 2 >= h {
            self.fill_rect(x, y, w, h, color);
            return;
        }

        self.fill_rect(x, y, w, thick, color);
        self.fill_rect(x, y + h - thick, w, thick, color);
        self.fill_rect(x, y + thick, thick, h - 2 * thick, color);
        self.fill_rect(x + w - thick, y + thick, thick, h - 2 * thick, color);
    }

    pub fn draw_circle(&mut self, cx: i32, cy: i32, r: i32, color: u8) {
        self.draw_circle_thick(cx, cy, r, color, 1);
    }

    pub fn draw_circle_thick(&mut self, cx: i32, cy: i32, r: i32, color: u8, thick: i32) {
        if r <= 0 {
            return;
        }
        let thick = thick.max(1);
        let half = thick / 2;

        let mut x = r;
        let mut y = 0;
        let mut err = 0;

        while x >= y {
            let pts = [
                (cx + x, cy + y),
                (cx + y, cy + x),
                (cx - y, cy + x),
                (cx - x, cy + y),
                (cx - x, cy - y),
                (cx - y, cy - x),
                (cx + y, cy - x),
                (cx + x, cy - y),
            ];

            for (px, py) in pts {
                if thick == 1 {
                    self.set_pixel(px, py, color);
                } else {
                    self.fill_rect(px - half, py - half, thick, thick, color);
                }
            }

            y += 1;
            if err <= 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err -= 2 * x + 1;
            }
        }
    }

    pub fn blit_8bpp(&mut self, dst_x: i32, dst_y: i32, src_w: i32, src_h: i32, src: &[u8]) {
        self.blit_8bpp_impl(dst_x, dst_y, src_w, src_h, src, None);
    }

    pub fn blit_8bpp_transparent(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        src_w: i32,
        src_h: i32,
        src: &[u8],
        transparent: u8,
    ) {
        self.blit_8bpp_impl(dst_x, dst_y, src_w, src_h, src, Some(transparent));
    }

    fn blit_8bpp_impl(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        src_w: i32,
        src_h: i32,
        src: &[u8],
        transparent: Option<u8>,
    ) {
        if src_w <= 0 || src_h <= 0 {
            return;
        }

        let Some(expected) = (src_w as usize).checked_mul(src_h as usize) else {
            return;
        };
        if src.len() < expected {
            return;
        }

        let clip = self.clip;
        let dst_x0 = dst_x.max(0).max(clip.x0 as i32);
        let dst_y0 = dst_y.max(0).max(clip.y0 as i32);

        let dst_x1 = (dst_x as i64 + src_w as i64)
            .max(0)
            .min(self.width as i64)
            .min(clip.x1 as i64) as i32;
        let dst_y1 = (dst_y as i64 + src_h as i64)
            .max(0)
            .min(self.height as i64)
            .min(clip.y1 as i64) as i32;

        if dst_x0 >= dst_x1 || dst_y0 >= dst_y1 {
            return;
        }

        let dst_w = self.width as usize;
        let src_w = src_w as usize;

        let copy_w = (dst_x1 - dst_x0) as usize;
        let src_x0 = (dst_x0 - dst_x) as usize;

        match transparent {
            None => {
                for dy in dst_y0..dst_y1 {
                    let sy = (dy - dst_y) as usize;
                    let src_start = sy * src_w + src_x0;
                    let dst_start = dy as usize * dst_w + dst_x0 as usize;
                    self.fb[dst_start..dst_start + copy_w]
                        .copy_from_slice(&src[src_start..src_start + copy_w]);
                }
            }
            Some(t) => {
                for dy in dst_y0..dst_y1 {
                    let sy = (dy - dst_y) as usize;
                    let src_start = sy * src_w + src_x0;
                    let dst_start = dy as usize * dst_w + dst_x0 as usize;
                    let dst_row = &mut self.fb[dst_start..dst_start + copy_w];
                    let src_row = &src[src_start..src_start + copy_w];
                    for (dst_px, &src_px) in dst_row.iter_mut().zip(src_row.iter()) {
                        if src_px != t {
                            *dst_px = src_px;
                        }
                    }
                }
            }
        }
    }

    pub fn draw_text(&mut self, x: i32, y: i32, fg: u8, bg: u8, text: &str) {
        let mut cx = x;
        for ch in text.chars() {
            if ch == '\n' {
                cx = x;
                continue;
            }
            self.draw_char_8x8(cx, y, fg, bg, ch);
            cx += 8;
        }
    }

    pub fn draw_char_8x8(&mut self, x: i32, y: i32, fg: u8, bg: u8, ch: char) {
        let code = assets::encode_cp437(ch);
        for row in 0..8i32 {
            let bits = self.font_u64[code as usize] >> ((row as u32) * 8);
            let row_bits = (bits & 0xFF) as u8;
            for col in 0..8i32 {
                let on = (row_bits & (1u8 << col as u8)) != 0;
                self.set_pixel(x + col, y + row, if on { fg } else { bg });
            }
        }
    }

    pub fn set_font_glyph_u64(&mut self, glyph: u8, bits: u64) {
        self.font_u64[glyph as usize] = bits;
    }

    pub fn present(&mut self) -> io::Result<()> {
        self.present_seq = self.present_seq.wrapping_add(1);
        let seq = self.present_seq;
        protocol::write_msg(&mut self.stream, Msg::present(seq))?;
        if self.sync_present {
            self.wait_for_present_ack(seq)?;
        }
        Ok(())
    }

    fn wait_for_present_ack(&mut self, seq: u32) -> io::Result<()> {
        const DEFAULT_TIMEOUT_MS: u64 = 500;
        let timeout_ms = std::env::var("TEMPLE_SYNC_PRESENT_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("temple-rt: present ack timeout (seq={seq})"),
                ));
            }

            match self
                .present_acks
                .recv_timeout(remaining.min(Duration::from_millis(25)))
            {
                Ok(ack) if ack == seq => return Ok(()),
                Ok(_other) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "temple-rt: present ack channel disconnected",
                    ));
                }
            }
        }
    }

    pub fn snd(&mut self, ona: i8) -> io::Result<()> {
        protocol::write_msg(&mut self.stream, Msg::snd(ona as u8 as u32))
    }

    pub fn mute(&mut self, val: bool) -> io::Result<()> {
        protocol::write_msg(&mut self.stream, Msg::mute(val))
    }

    pub fn palette_color_set(&mut self, color_index: u8, rgba: [u8; 4]) -> io::Result<()> {
        let packed = ((rgba[0] as u32) << 24)
            | ((rgba[1] as u32) << 16)
            | ((rgba[2] as u32) << 8)
            | (rgba[3] as u32);
        protocol::write_msg(
            &mut self.stream,
            Msg::palette_color_set(color_index as u32, packed),
        )
    }

    pub fn settings_push(&mut self) -> io::Result<()> {
        protocol::write_msg(&mut self.stream, Msg::settings_push())
    }

    pub fn settings_pop(&mut self) -> io::Result<()> {
        protocol::write_msg(&mut self.stream, Msg::settings_pop())
    }

    pub fn clipboard_set_text(&mut self, text: &str) -> io::Result<()> {
        const MAX_BYTES: usize = 1024 * 1024;

        let bytes = text.as_bytes();
        if bytes.len() > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "temple-rt: clipboard text too large",
            ));
        }

        protocol::write_msg(&mut self.stream, Msg::clipboard_set(bytes.len() as u32))?;
        self.stream.write_all(bytes)?;
        Ok(())
    }

    pub fn try_next_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}

fn env_truthy(name: &str) -> bool {
    let Some(val) = std::env::var_os(name) else {
        return false;
    };
    let v = val.to_string_lossy();
    !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("no"))
}
