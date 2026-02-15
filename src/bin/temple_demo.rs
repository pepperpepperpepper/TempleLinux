use std::{io, thread, time::Duration};

use temple_rt::{
    protocol,
    rt::{Event, TempleRt},
};

fn clamp_i32(v: i32, min_v: i32, max_v: i32) -> i32 {
    v.max(min_v).min(max_v)
}

const KEY_C_LOWER: u32 = b'c' as u32;
const KEY_C_UPPER: u32 = b'C' as u32;

fn main() -> io::Result<()> {
    match run() {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err),
    }
}

fn run() -> io::Result<()> {
    let mut rt = TempleRt::connect()?;
    let (w, h) = rt.size();

    let mut rect_x: i32 = (w as i32) / 2 - 20;
    let mut rect_y: i32 = (h as i32) / 2 - 15;
    let rect_w: i32 = 40;
    let rect_h: i32 = 30;

    let mut mouse_x: i32 = 0;
    let mut mouse_y: i32 = 0;

    let mut last_key = String::from("(none)");

    loop {
        rt.clear(0);
        rt.fill_rect(0, 0, w as i32, 16, 4);
        rt.draw_text(
            4,
            4,
            15,
            4,
            "Temple demo - arrows move - LMB teleports - C copies coords - Esc exits",
        );

        let coord = format!(
            "Rect: ({rect_x},{rect_y})  Mouse: ({mouse_x},{mouse_y})  Last key: {last_key}"
        );
        rt.draw_text(4, 24, 15, 0, &coord);

        rt.fill_rect(rect_x, rect_y, rect_w, rect_h, 10);
        rt.fill_rect(rect_x + 2, rect_y + 2, rect_w - 4, rect_h - 4, 12);
        rt.draw_text(rect_x + 6, rect_y + 10, 0, 12, "APP");

        rt.present()?;

        let mut did_something = false;
        while let Some(ev) = rt.try_next_event() {
            did_something = true;
            match ev {
                Event::Key { code, down } => {
                    if !down {
                        continue;
                    }
                    last_key = if code <= 0xFF {
                        let ch = char::from_u32(code).unwrap_or('?');
                        format!("'{ch}'")
                    } else {
                        format!("0x{code:X}")
                    };

                    match code {
                        protocol::KEY_ESCAPE => return Ok(()),
                        protocol::KEY_LEFT => rect_x -= 4,
                        protocol::KEY_RIGHT => rect_x += 4,
                        protocol::KEY_UP => rect_y -= 4,
                        protocol::KEY_DOWN => rect_y += 4,
                        KEY_C_LOWER | KEY_C_UPPER => {
                            if let Err(err) = rt.clipboard_set_text(&coord) {
                                last_key = format!("clipboard error: {err}");
                            } else {
                                last_key = "copied coords".to_string();
                            }
                        }
                        _ => {}
                    }
                }
                Event::MouseMove { x, y } => {
                    mouse_x = x as i32;
                    mouse_y = y as i32;
                }
                Event::MouseButton { button, down } => {
                    if button == protocol::MOUSE_BUTTON_LEFT && down {
                        rect_x = mouse_x - rect_w / 2;
                        rect_y = mouse_y - rect_h / 2;
                    }
                }
                Event::MouseWheel { .. } => {}
                Event::MouseEnter => {}
                Event::MouseLeave => {}
            }
        }

        rect_x = clamp_i32(rect_x, 0, w as i32 - rect_w);
        rect_y = clamp_i32(rect_y, 16, h as i32 - rect_h);

        if !did_something {
            thread::sleep(Duration::from_millis(16));
        }
    }
}
