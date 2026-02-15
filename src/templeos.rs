//! TempleOS-ish compatibility surface for **Rust** code.
//!
//! TempleLinux runs TempleOS apps (HolyC) via `temple-hc`, which provides many TempleOS API
//! functions as *built-ins*. This module exists to make it easier to write small Rust-side demo
//! apps (or adapters) that use familiar TempleOS names and constants.
//!
//! Notes:
//! - This is **not** a complete TempleOS API. It is intentionally a small, practical subset.
//! - Graphics here are palette-indexed (0–15) and render into the TempleShell framebuffer.
#![allow(non_snake_case)]

use std::io;

use crate::rt::TempleRt;

// --- Common constants --------------------------------------------------------

pub const TRUE: i64 = 1;
pub const FALSE: i64 = 0;
pub const ON: i64 = 1;
pub const OFF: i64 = 0;
pub const NULL: i64 = 0;

// TempleOS-ish char constants (from `::/Kernel/KernelA.HH`, subset).
pub const CH_CTRLA: i64 = 0x01;
pub const CH_CTRLB: i64 = 0x02;
pub const CH_CTRLC: i64 = 0x03;
pub const CH_CTRLD: i64 = 0x04;
pub const CH_CTRLE: i64 = 0x05;
pub const CH_CTRLF: i64 = 0x06;
pub const CH_CTRLG: i64 = 0x07;
pub const CH_CTRLH: i64 = 0x08;
pub const CH_CTRLI: i64 = 0x09;
pub const CH_CTRLJ: i64 = 0x0A;
pub const CH_CTRLK: i64 = 0x0B;
pub const CH_CTRLL: i64 = 0x0C;
pub const CH_CTRLM: i64 = 0x0D;
pub const CH_CTRLN: i64 = 0x0E;
pub const CH_CTRLO: i64 = 0x0F;
pub const CH_CTRLP: i64 = 0x10;
pub const CH_CTRLQ: i64 = 0x11;
pub const CH_CTRLR: i64 = 0x12;
pub const CH_CTRLS: i64 = 0x13;
pub const CH_CTRLT: i64 = 0x14;
pub const CH_CTRLU: i64 = 0x15;
pub const CH_CTRLV: i64 = 0x16;
pub const CH_CTRLW: i64 = 0x17;
pub const CH_CTRLX: i64 = 0x18;
pub const CH_CTRLY: i64 = 0x19;
pub const CH_CTRLZ: i64 = 0x1A;
pub const CH_BACKSPACE: i64 = 0x08;
pub const CH_ESC: i64 = 0x1B;
pub const CH_SHIFT_ESC: i64 = 0x1C;
pub const CH_SHIFT_SPACE: i64 = 0x1F;
pub const CH_SPACE: i64 = 0x20;

// TempleOS std palette indices.
pub const BLACK: u8 = 0;
pub const BLUE: u8 = 1;
pub const GREEN: u8 = 2;
pub const CYAN: u8 = 3;
pub const RED: u8 = 4;
pub const MAGENTA: u8 = 5;
pub const PURPLE: u8 = 5;
pub const BROWN: u8 = 6;
pub const LTGRAY: u8 = 7;
pub const LGRAY: u8 = 7;
pub const DKGRAY: u8 = 8;
pub const DGRAY: u8 = 8;
pub const LTBLUE: u8 = 9;
pub const LTGREEN: u8 = 10;
pub const LTCYAN: u8 = 11;
pub const LTRED: u8 = 12;
pub const LTPURPLE: u8 = 13;
pub const LTMAGENTA: u8 = 13;
pub const YELLOW: u8 = 14;
pub const WHITE: u8 = 15;
pub const COLORS_NUM: u8 = 16;

pub const COLOR_INVALID: u8 = 16;
pub const COLOR_MONO: u8 = 0xFF;

pub const FONT_WIDTH: i64 = 8;
pub const FONT_HEIGHT: i64 = 8;

// Messages (subset).
pub const MSG_NULL: i64 = 0;
pub const MSG_CMD: i64 = 1;
pub const MSG_KEY_DOWN: i64 = 2;
pub const MSG_KEY_UP: i64 = 3;
pub const MSG_MS_MOVE: i64 = 4;
pub const MSG_MS_L_DOWN: i64 = 5;
pub const MSG_MS_L_UP: i64 = 6;
pub const MSG_MS_R_DOWN: i64 = 9;
pub const MSG_MS_R_UP: i64 = 10;

// Scan codes (subset).
pub const SC_ESC: i64 = 0x01;
pub const SC_BACKSPACE: i64 = 0x0E;
pub const SC_TAB: i64 = 0x0F;
pub const SC_ENTER: i64 = 0x1C;
pub const SC_SHIFT: i64 = 0x2A;
pub const SC_CTRL: i64 = 0x1D;
pub const SC_ALT: i64 = 0x38;
pub const SC_CAPS: i64 = 0x3A;
pub const SC_NUM: i64 = 0x45;
pub const SC_SCROLL: i64 = 0x46;
pub const SC_CURSOR_UP: i64 = 0x48;
pub const SC_CURSOR_DOWN: i64 = 0x50;
pub const SC_CURSOR_LEFT: i64 = 0x4B;
pub const SC_CURSOR_RIGHT: i64 = 0x4D;
pub const SC_PAGE_UP: i64 = 0x49;
pub const SC_PAGE_DOWN: i64 = 0x51;
pub const SC_HOME: i64 = 0x47;
pub const SC_END: i64 = 0x4F;
pub const SC_INS: i64 = 0x52;
pub const SC_DELETE: i64 = 0x53;
pub const SC_F1: i64 = 0x3B;
pub const SC_F2: i64 = 0x3C;
pub const SC_F3: i64 = 0x3D;
pub const SC_F4: i64 = 0x3E;
pub const SC_F5: i64 = 0x3F;
pub const SC_F6: i64 = 0x40;
pub const SC_F7: i64 = 0x41;
pub const SC_F8: i64 = 0x42;
pub const SC_F9: i64 = 0x43;
pub const SC_F10: i64 = 0x44;
pub const SC_F11: i64 = 0x57;
pub const SC_F12: i64 = 0x58;

// Scan code flags (subset).
pub const SCF_KEY_UP: i64 = 0x100;
pub const SCF_SHIFT: i64 = 0x200;
pub const SCF_CTRL: i64 = 0x400;
pub const SCF_ALT: i64 = 0x800;
pub const SCF_DELETE: i64 = 0x40000;
pub const SCF_INS: i64 = 0x80000;

// Window inhibit flags (subset).
pub const WIF_SELF_MS_L: i64 = 0x0008;
pub const WIF_SELF_MS_R: i64 = 0x0020;
pub const WIF_SELF_KEY_DESC: i64 = 0x1000;
pub const WIF_FOCUS_TASK_MS_L_D: i64 = 0x00100000;
pub const WIF_FOCUS_TASK_MS_R_D: i64 = 0x00400000;
pub const WIG_DBL_CLICK: i64 = 0x00500000;
pub const WIG_USER_TASK_DFT: i64 = 0x1000;

// Device context flags (subset).
pub const DCF_TRANSFORMATION: i64 = 0x100;
pub const DCF_SYMMETRY: i64 = 0x200;
pub const DCF_JUST_MIRROR: i64 = 0x400;

// File utils (subset).
pub const FUF_JUST_DIRS: i64 = 0x0000400;

// GetStr flags (subset).
pub const GSF_WITH_NEW_LINE: i64 = 2;

// Control flags/types (subset).
pub const CTRLT_GENERIC: i64 = 0;
pub const CTRLF_SHOW: i64 = 1;
pub const CTRLF_BORDER: i64 = 2;
pub const CTRLF_CAPTURE_LEFT_MS: i64 = 4;
pub const CTRLF_CAPTURE_RIGHT_MS: i64 = 8;
pub const CTRLF_CLICKED: i64 = 16;

// Graphics (misc).
pub const GR_Z_ALL: i64 = 1073741823;

// Date/time.
pub const CDATE_FREQ: i64 = 49710;

// TempleLinux/HolyC key codes (internal), re-exported for convenience.
pub use crate::protocol::{
    KEY_ALT, KEY_BACKSPACE, KEY_CONTROL, KEY_DELETE, KEY_DOWN, KEY_END, KEY_ENTER, KEY_ESCAPE,
    KEY_F1, KEY_F2, KEY_F3, KEY_F4, KEY_F5, KEY_F6, KEY_F7, KEY_F8, KEY_F9, KEY_F10, KEY_F11,
    KEY_F12, KEY_HOME, KEY_INSERT, KEY_LEFT, KEY_PAGE_DOWN, KEY_PAGE_UP, KEY_RIGHT, KEY_SHIFT,
    KEY_SUPER, KEY_TAB, KEY_UP,
};

pub use crate::protocol::{
    MOUSE_BUTTON_BACK, MOUSE_BUTTON_FORWARD, MOUSE_BUTTON_LEFT, MOUSE_BUTTON_MIDDLE,
    MOUSE_BUTTON_OTHER_BASE, MOUSE_BUTTON_RIGHT,
};

// --- Types -------------------------------------------------------------------

/// Minimal TempleOS-ish device context.
///
/// TempleOS passes `CDC *dc` around; TempleLinux's Rust API is built around `TempleRt` methods.
/// This struct exists mainly so Rust ports can keep using the same "dc.color/dc.thick" pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CDC {
    pub color: u8,
    pub thick: i32,
}

impl Default for CDC {
    fn default() -> Self {
        Self {
            color: WHITE,
            thick: 1,
        }
    }
}

/// TempleOS uses a 48-bit BGR color (`CBGR48`) for palette edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CBGR48 {
    pub b: u16,
    pub g: u16,
    pub r: u16,
}

// --- Helpers -----------------------------------------------------------------

pub fn SCR_W(rt: &TempleRt) -> i64 {
    rt.size().0 as i64
}

pub fn SCR_H(rt: &TempleRt) -> i64 {
    rt.size().1 as i64
}

pub fn GR_WIDTH(rt: &TempleRt) -> i64 {
    SCR_W(rt)
}

pub fn GR_HEIGHT(rt: &TempleRt) -> i64 {
    SCR_H(rt)
}

// --- Graphics shims ----------------------------------------------------------

pub fn GrPlot(rt: &mut TempleRt, dc: &CDC, x: i32, y: i32) {
    rt.set_pixel(x, y, dc.color);
}

pub fn GrLine(rt: &mut TempleRt, dc: &CDC, x1: i32, y1: i32, x2: i32, y2: i32, thick: Option<i32>) {
    let thick = thick.unwrap_or(dc.thick).max(1);
    rt.draw_line_thick(x1, y1, x2, y2, dc.color, thick);
}

pub fn GrLine3(
    rt: &mut TempleRt,
    dc: &CDC,
    x1: i32,
    y1: i32,
    _z1: i32,
    x2: i32,
    y2: i32,
    _z2: i32,
    thick: Option<i32>,
) {
    GrLine(rt, dc, x1, y1, x2, y2, thick);
}

pub fn GrBorder(rt: &mut TempleRt, dc: &CDC, x1: i32, y1: i32, x2: i32, y2: i32) {
    let w = x2 - x1;
    let h = y2 - y1;
    rt.draw_rect_outline_thick(x1, y1, w, h, dc.color, dc.thick.max(1));
}

pub fn GrRect(rt: &mut TempleRt, dc: &CDC, x: i32, y: i32, w: i32, h: i32, color: Option<u8>) {
    let color = color.unwrap_or(dc.color);
    rt.draw_rect_outline_thick(x, y, w, h, color, dc.thick.max(1));
}

pub fn GrCircle(rt: &mut TempleRt, dc: &CDC, x: i32, y: i32, r: i32, thick: Option<i32>) {
    let thick = thick.unwrap_or(dc.thick).max(1);
    if thick == 1 {
        rt.draw_circle(x, y, r, dc.color);
    } else {
        rt.draw_circle_thick(x, y, r, dc.color, thick);
    }
}

pub fn GrClip(rt: &mut TempleRt, x: i32, y: i32, w: i32, h: i32) {
    rt.set_clip_rect(x, y, w, h);
}

pub fn GrUnClip(rt: &mut TempleRt) {
    rt.reset_clip_rect();
}

pub fn GrPaletteColorSet(rt: &mut TempleRt, color_index: u8, bgr48: CBGR48) -> io::Result<()> {
    let rgba = [
        (bgr48.r >> 8) as u8,
        (bgr48.g >> 8) as u8,
        (bgr48.b >> 8) as u8,
        255u8,
    ];
    rt.palette_color_set(color_index, rgba)
}

// --- Sound/settings shims ----------------------------------------------------

pub fn Snd(rt: &mut TempleRt, ona: i8) -> io::Result<()> {
    rt.snd(ona)
}

pub fn Mute(rt: &mut TempleRt, val: bool) -> io::Result<()> {
    rt.mute(val)
}

pub fn SettingsPush(rt: &mut TempleRt) -> io::Result<()> {
    rt.settings_push()
}

pub fn SettingsPop(rt: &mut TempleRt) -> io::Result<()> {
    rt.settings_pop()
}
