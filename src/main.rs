mod audio;

use std::{
    ffi::CString,
    fmt,
    fs::File,
    io,
    os::unix::{io::AsRawFd as _, net::UnixListener},
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};

use arboard::Clipboard;
use std::thread;
use temple_rt::assets;
use temple_rt::protocol;
use wgpu::util::DeviceExt as _;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    keyboard::{Key, NamedKey},
    window::{Fullscreen, Window, WindowBuilder},
};

const INTERNAL_W: u32 = 640;
const INTERNAL_H: u32 = 480;

const FONT_W: u32 = 8;
const FONT_H: u32 = 8;
const TERM_COLS: u32 = INTERNAL_W / FONT_W;
const TERM_ROWS: u32 = INTERNAL_H / FONT_H;
const OUTPUT_ROWS: u32 = TERM_ROWS - 2;
const PROMPT_ROW: u32 = TERM_ROWS - 2;
const STATUS_ROW: u32 = TERM_ROWS - 1;

const COLOR_BG: u8 = 0;
const COLOR_FG: u8 = 15;
const COLOR_STATUS_BG: u8 = 4;
const COLOR_SEL_BG: u8 = 1;
const COLOR_SEL_FG: u8 = 15;

const CURSOR_W: u32 = 8;
const CURSOR_H: u32 = 8;
const CURSOR_BORDER_COLOR: u8 = 0;
const CURSOR_FILL_COLOR: u8 = 15;

include!("templeshell/00_ui_primitives.rs");
include!("templeshell/01_paths_browser.rs");
include!("templeshell/02_shell.rs");
include!("templeshell/03_gfx.rs");
include!("templeshell/04_app.rs");
