use std::{io, thread, time::Duration};

use temple_rt::{
    protocol,
    rt::{Event, TempleRt},
};

const UI_BAR_H: i32 = 16;

fn clamp_i32(v: i32, min_v: i32, max_v: i32) -> i32 {
    v.max(min_v).min(max_v)
}

fn paint_rect(canvas: &mut [u8], w: i32, h: i32, x: i32, y: i32, bw: i32, bh: i32, c: u8) {
    if bw <= 0 || bh <= 0 {
        return;
    }
    let x0 = x.max(0);
    let y0 = y.max(UI_BAR_H);
    let x1 = (x + bw).min(w);
    let y1 = (y + bh).min(h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    for yy in y0..y1 {
        let row = (yy * w) as usize;
        let start = row + x0 as usize;
        let end = row + x1 as usize;
        canvas[start..end].fill(c);
    }
}

fn paint_line(
    canvas: &mut [u8],
    w: i32,
    h: i32,
    from: (i32, i32),
    to: (i32, i32),
    bw: i32,
    bh: i32,
    c: u8,
) {
    let (x0, y0) = from;
    let (x1, y1) = to;

    let dx = x1 - x0;
    let dy = y1 - y0;
    let dist = dx.abs().max(dy.abs());
    if dist == 0 {
        paint_rect(canvas, w, h, x1, y1, bw, bh, c);
        return;
    }

    let stride = (bw.min(bh) / 2).max(1);
    let steps = (dist / stride).max(1);
    for i in 0..=steps {
        let x = x0 + dx * i / steps;
        let y = y0 + dy * i / steps;
        paint_rect(canvas, w, h, x, y, bw, bh, c);
    }
}

fn draw_cursor(rt: &mut TempleRt, x: i32, y: i32, bw: i32, bh: i32) {
    let border = 15u8;
    rt.fill_rect(x - 1, y - 1, bw + 2, 1, border);
    rt.fill_rect(x - 1, y + bh, bw + 2, 1, border);
    rt.fill_rect(x - 1, y, 1, bh, border);
    rt.fill_rect(x + bw, y, 1, bh, border);
}

fn main() -> io::Result<()> {
    match run() {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err),
    }
}

fn run() -> io::Result<()> {
    let mut rt = TempleRt::connect()?;
    let (w_u32, h_u32) = rt.size();
    let (w, h) = (w_u32 as i32, h_u32 as i32);

    let mut canvas = vec![0u8; (w_u32 * h_u32) as usize];

    let mut brush_w: i32 = 8;
    let mut brush_h: i32 = 8;
    let mut color: u8 = 12;

    let mut x: i32 = w / 2 - brush_w / 2;
    let mut y: i32 = (h.max(UI_BAR_H + 1) + UI_BAR_H) / 2;

    let mut mouse_pos: Option<(i32, i32)> = None;
    let mut mouse_left_down = false;
    let mut mouse_right_down = false;
    let mut last_paint_pos: Option<(i32, i32)> = None;

    loop {
        rt.framebuffer_mut().copy_from_slice(&canvas);

        rt.fill_rect(0, 0, w, UI_BAR_H, 4);
        rt.draw_text(
            4,
            4,
            15,
            4,
            "Temple Paint - LMB paint  RMB erase  MMB pick  arrows move  C clear  0-9 color  +/- brush  Esc",
        );

        let status = format!("Color: {color}  Brush: {brush_w}x{brush_h}");
        rt.draw_text(4, UI_BAR_H + 4, 15, 0, &status);

        draw_cursor(&mut rt, x, y, brush_w, brush_h);
        rt.present()?;

        while let Some(ev) = rt.try_next_event() {
            match ev {
                Event::Key { code, down } => {
                    if !down {
                        continue;
                    }

                    match code {
                        protocol::KEY_ESCAPE => return Ok(()),
                        protocol::KEY_LEFT => x -= brush_w,
                        protocol::KEY_RIGHT => x += brush_w,
                        protocol::KEY_UP => y -= brush_h,
                        protocol::KEY_DOWN => y += brush_h,
                        _ if code <= 0xFF => {
                            let ch = char::from_u32(code).unwrap_or('\0');
                            match ch {
                                ' ' => paint_rect(&mut canvas, w, h, x, y, brush_w, brush_h, color),
                                'c' | 'C' => canvas.fill(0),
                                '0'..='9' => color = (ch as u8) - b'0',
                                '+' => {
                                    brush_w = (brush_w + 1).min(64);
                                    brush_h = (brush_h + 1).min(64);
                                }
                                '-' => {
                                    brush_w = (brush_w - 1).max(1);
                                    brush_h = (brush_h - 1).max(1);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                Event::MouseMove { x: mx, y: my } => {
                    let mx = mx as i32;
                    let my = my as i32;
                    mouse_pos = Some((mx, my));

                    let new_x = mx - brush_w / 2;
                    let new_y = my - brush_h / 2;
                    x = new_x;
                    y = new_y;

                    let paint_color = if mouse_right_down {
                        Some(0u8)
                    } else if mouse_left_down {
                        Some(color)
                    } else {
                        None
                    };

                    if let Some(c) = paint_color {
                        let from = last_paint_pos.unwrap_or((new_x, new_y));
                        paint_line(&mut canvas, w, h, from, (new_x, new_y), brush_w, brush_h, c);
                        last_paint_pos = Some((new_x, new_y));
                    } else {
                        last_paint_pos = None;
                    }
                }
                Event::MouseButton { button, down } => {
                    match button {
                        protocol::MOUSE_BUTTON_LEFT => mouse_left_down = down,
                        protocol::MOUSE_BUTTON_RIGHT => mouse_right_down = down,
                        protocol::MOUSE_BUTTON_MIDDLE if down => {
                            if let Some((mx, my)) = mouse_pos {
                                if mx >= 0 && mx < w && my >= 0 && my < h {
                                    color = canvas[(my * w + mx) as usize];
                                }
                            }
                        }
                        _ => {}
                    }

                    if down {
                        if let Some((mx, my)) = mouse_pos {
                            x = mx - brush_w / 2;
                            y = my - brush_h / 2;
                        }

                        let paint_color = if mouse_right_down {
                            Some(0u8)
                        } else if mouse_left_down {
                            Some(color)
                        } else {
                            None
                        };

                        if let Some(c) = paint_color {
                            paint_rect(&mut canvas, w, h, x, y, brush_w, brush_h, c);
                            last_paint_pos = Some((x, y));
                        }
                    } else if !mouse_left_down && !mouse_right_down {
                        last_paint_pos = None;
                    }
                }
                Event::MouseWheel { .. } => {}
                Event::MouseEnter => {}
                Event::MouseLeave => {
                    mouse_pos = None;
                    mouse_left_down = false;
                    mouse_right_down = false;
                    last_paint_pos = None;
                }
            }
        }

        x = clamp_i32(x, 0, w - brush_w);
        y = clamp_i32(y, UI_BAR_H, h - brush_h);

        thread::sleep(Duration::from_millis(16));
    }
}
