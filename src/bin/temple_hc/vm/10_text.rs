use super::prelude::*;
use super::{Value, Vm};

impl Vm {
    fn newline(&mut self) {
        self.text_x = 0;
        self.text_y += 8;

        let (w, h) = self.rt.size();
        let h = h as i32;
        if self.text_y + 8 <= h {
            return;
        }

        // Scroll the framebuffer up by one text row (8 pixels).
        let w = w as usize;
        let shift_rows = 8usize;
        let shift = w * shift_rows;
        let fb = self.rt.framebuffer_mut();
        if shift < fb.len() {
            let len = fb.len();
            fb.copy_within(shift.., 0);
            fb[len - shift..].fill(self.text_bg);
        }
        self.text_y = (h - 8).max(0);
    }

    pub(super) fn put_char(&mut self, ch: char) {
        match ch {
            '\0' => {}
            '\n' => {
                self.capture_push('\n');
                self.newline();
            }
            '\r' => {
                self.capture_push('\r');
                self.text_x = 0;
            }
            '\t' => {
                for _ in 0..4 {
                    self.put_char(' ');
                }
            }
            _ => {
                self.capture_push(ch);
                let (w, _) = self.rt.size();
                if self.text_x + 8 > w as i32 {
                    self.newline();
                }
                self.rt
                    .draw_char_8x8(self.text_x, self.text_y, self.text_fg, self.text_bg, ch);
                self.text_x += 8;
            }
        }
    }

    fn print_str(&mut self, text: &str) {
        // TempleOS frequently uses DolDoc-style inline markup in strings, especially for color:
        //   "$$RED$$Hello$$FG$$"
        //
        // For compatibility and a more TempleOS-like look, interpret a small subset here.
        // Unknown sequences are preserved literally.
        let mut i = 0usize;
        while i < text.len() {
            if text[i..].starts_with("$$") {
                let after_open = i + 2;
                if after_open <= text.len() {
                    if let Some(end_rel) = text[after_open..].find("$$") {
                        let end = after_open + end_rel;
                        let code = &text[after_open..end];
                        if self.try_apply_doldoc_code(code) {
                            i = end + 2;
                            continue;
                        }

                        // Unknown code: emit literally.
                        self.put_char('$');
                        self.put_char('$');
                        for ch in code.chars() {
                            self.put_char(ch);
                        }
                        self.put_char('$');
                        self.put_char('$');
                        i = end + 2;
                        continue;
                    }
                }
            }

            let ch = text[i..].chars().next().unwrap_or('\0');
            self.put_char(ch);
            i = i.saturating_add(ch.len_utf8().max(1));
        }
    }

    fn try_apply_doldoc_code(&mut self, code: &str) -> bool {
        let code = code.trim();
        if code.is_empty() {
            return false;
        }

        let upper = code.to_ascii_uppercase();

        // Reset to default foreground/background.
        if upper == "FG" {
            self.text_fg = 15;
            return true;
        }
        if upper == "BG" {
            self.text_bg = 0;
            return true;
        }

        // Background set: $$BK,<idx>$$
        if let Some(rest) = upper.strip_prefix("BK,") {
            if let Ok(v) = rest.trim().parse::<i64>() {
                self.text_bg = v.clamp(0, 15) as u8;
                return true;
            }
        }

        // Foreground/background numeric set (nonstandard but harmless).
        if let Some((name, rest)) = upper.split_once(',') {
            if let Ok(v) = rest.trim().parse::<i64>() {
                let idx = v.clamp(0, 15) as u8;
                match name.trim() {
                    "FG" => {
                        self.text_fg = idx;
                        return true;
                    }
                    "BG" => {
                        self.text_bg = idx;
                        return true;
                    }
                    _ => {}
                }
            }
        }

        if let Some(idx) = Self::doldoc_color_name_to_idx(&upper) {
            self.text_fg = idx;
            return true;
        }

        false
    }

    fn doldoc_color_name_to_idx(name: &str) -> Option<u8> {
        match name {
            "BLACK" => Some(0),
            "BLUE" => Some(1),
            "GREEN" => Some(2),
            "CYAN" => Some(3),
            "RED" => Some(4),
            "PURPLE" | "MAGENTA" => Some(5),
            "BROWN" => Some(6),
            "LTGRAY" | "LGRAY" => Some(7),
            "DKGRAY" | "DGRAY" => Some(8),
            "LTBLUE" => Some(9),
            "LTGREEN" => Some(10),
            "LTCYAN" => Some(11),
            "LTRED" => Some(12),
            "LTPURPLE" | "LTMAGENTA" => Some(13),
            "YELLOW" => Some(14),
            "WHITE" => Some(15),
            _ => None,
        }
    }

    fn print_putchars(&mut self, v: u64) {
        for i in 0..8usize {
            let b = ((v >> (i * 8)) & 0xff) as u8;
            if b == 0 {
                break;
            }
            self.put_char(b as char);
        }
    }

    pub(super) fn exec_print(&mut self, parts: &[Expr]) -> Result<(), String> {
        let mut values = Vec::with_capacity(parts.len());
        for expr in parts {
            values.push(self.eval_expr(expr)?);
        }

        if values.is_empty() {
            return Ok(());
        }

        match (values.first().cloned().unwrap(), values.len()) {
            (Value::Str(s), 1) => self.print_str(&s),
            (Value::Char(v), 1) => self.print_putchars(v),
            (Value::Int(v), 1) => self.print_str(&v.to_string()),
            (Value::Float(v), 1) => self.print_str(&v.to_string()),
            (Value::Str(fmt), _) => {
                let rendered = format_temple_fmt_with_cstr(
                    &fmt,
                    &values[1..],
                    |ptr| self.read_cstr_lossy(ptr),
                    |idx, name| self.define_sub(idx, name),
                )?;
                self.print_str(&rendered);
            }
            (other, _) => {
                return Err(format!(
                    "print statement expects a leading string/char literal, got {other:?}"
                ));
            }
        }

        self.present_with_overlays()?;
        Ok(())
    }
}
