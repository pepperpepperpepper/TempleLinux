use std::ops::Range;

use crate::assets;

const SPG_TYPE_MASK: u8 = 0x7f;

const SPT_END: u8 = 0;
const SPT_COLOR: u8 = 1;
const SPT_DITHER_COLOR: u8 = 2;
const SPT_THICK: u8 = 3;
const SPT_PLANAR_SYMMETRY: u8 = 4;
const SPT_TRANSFORM_ON: u8 = 5;
const SPT_TRANSFORM_OFF: u8 = 6;
const SPT_SHIFT: u8 = 7;
const SPT_PT: u8 = 8;
const SPT_POLYPT: u8 = 9;
const SPT_LINE: u8 = 10;
const SPT_POLYLINE: u8 = 11;
const SPT_RECT: u8 = 12;
const SPT_ROTATED_RECT: u8 = 13;
const SPT_CIRCLE: u8 = 14;
const SPT_ELLIPSE: u8 = 15;
const SPT_POLYGON: u8 = 16;
const SPT_BSPLINE2: u8 = 17;
const SPT_BSPLINE2_CLOSED: u8 = 18;
const SPT_BSPLINE3: u8 = 19;
const SPT_BSPLINE3_CLOSED: u8 = 20;
const SPT_FLOOD_FILL: u8 = 21;
const SPT_FLOOD_FILL_NOT: u8 = 22;
const SPT_BITMAP: u8 = 23;
const SPT_MESH: u8 = 24;
const SPT_SHIFTABLE_MESH: u8 = 25;
const SPT_ARROW: u8 = 26;
const SPT_TEXT: u8 = 27;
const SPT_TEXT_BOX: u8 = 28;
const SPT_TEXT_DIAMOND: u8 = 29;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpriteBounds {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl SpriteBounds {
    pub fn width(&self) -> i32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> i32 {
        self.y1 - self.y0
    }

    fn include_point(&mut self, x: i32, y: i32) {
        self.include_rect(x, y, 1, 1);
    }

    fn include_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }

        if self.x0 == 0 && self.y0 == 0 && self.x1 == 0 && self.y1 == 0 {
            self.x0 = x;
            self.y0 = y;
            self.x1 = x.saturating_add(w);
            self.y1 = y.saturating_add(h);
            return;
        }

        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x.saturating_add(w));
        self.y1 = self.y1.max(y.saturating_add(h));
    }
}

pub trait SpriteTarget {
    fn set_pixel(&mut self, x: i32, y: i32, color: u8);
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8);

    fn draw_line_thick(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8, thick: i32) {
        draw_line_thick_default(self, x1, y1, x2, y2, color, thick);
    }

    fn draw_rect_outline_thick(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8, thick: i32) {
        draw_rect_outline_thick_default(self, x, y, w, h, color, thick);
    }

    fn draw_circle_thick(&mut self, cx: i32, cy: i32, r: i32, color: u8, thick: i32) {
        draw_circle_thick_default(self, cx, cy, r, color, thick);
    }

    fn blit_8bpp(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        src_w: i32,
        src_h: i32,
        stride: i32,
        src: &[u8],
    );
}

fn read_i32_le(bytes: &[u8], off: usize) -> Option<i32> {
    let b = bytes.get(off..off + 4)?;
    Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_u16_le(bytes: &[u8], off: usize) -> Option<u16> {
    let b = bytes.get(off..off + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn ceil_to_multiple(v: i32, step: i32) -> i32 {
    if step <= 0 {
        return v;
    }
    ((v + step - 1) / step) * step
}

fn sprite_text_nul_range(bytes: &[u8], start: usize) -> Option<Range<usize>> {
    let tail = bytes.get(start..)?;
    let len = tail.iter().position(|&b| b == 0)?;
    Some(start..start + len)
}

fn sprite_elem_size(bytes: &[u8], off: usize) -> Option<usize> {
    let t = *bytes.get(off)? & SPG_TYPE_MASK;
    match t {
        SPT_END => Some(1),
        SPT_COLOR => Some(2),
        SPT_DITHER_COLOR => Some(3),
        SPT_THICK => Some(1 + 4),
        SPT_PLANAR_SYMMETRY => Some(1 + 4 * 4),
        SPT_TRANSFORM_ON | SPT_TRANSFORM_OFF => Some(1),
        SPT_SHIFT | SPT_PT | SPT_FLOOD_FILL | SPT_FLOOD_FILL_NOT => Some(1 + 4 + 4),
        SPT_LINE | SPT_RECT | SPT_ARROW => Some(1 + 4 * 4),
        SPT_ROTATED_RECT => Some(1 + 4 * 4 + 8),
        SPT_CIRCLE => Some(1 + 4 * 3),
        SPT_ELLIPSE => Some(1 + 4 * 4 + 8),
        SPT_POLYGON => Some(1 + 4 * 4 + 8 + 4),
        SPT_POLYPT => {
            let num = read_i32_le(bytes, off + 1)? as i64;
            if num < 0 {
                return None;
            }
            let num = usize::try_from(num).ok()?;
            let mask = (num * 3 + 7) / 8;
            let base = 1usize + 4 + 4 + 4;
            Some(base.checked_add(mask)?)
        }
        SPT_POLYLINE => {
            let num = read_i32_le(bytes, off + 1)? as i64;
            if num < 0 {
                return None;
            }
            let num = usize::try_from(num).ok()?;
            let base = 1usize + 4;
            Some(base.checked_add(num.checked_mul(8)?)?)
        }
        SPT_BSPLINE2 | SPT_BSPLINE2_CLOSED | SPT_BSPLINE3 | SPT_BSPLINE3_CLOSED => {
            let num = read_i32_le(bytes, off + 1)? as i64;
            if num < 0 {
                return None;
            }
            let num = usize::try_from(num).ok()?;
            let base = 1usize + 4;
            Some(base.checked_add(num.checked_mul(12)?)?)
        }
        SPT_TEXT | SPT_TEXT_BOX | SPT_TEXT_DIAMOND => {
            // type + x1 + y1 + nul-terminated bytes
            let base = off + 1 + 4 + 4;
            let s = sprite_text_nul_range(bytes, base)?;
            Some((base - off) + s.len() + 1)
        }
        SPT_BITMAP => {
            let w = read_i32_le(bytes, off + 1 + 8)?; // x1,y1,width,height
            let h = read_i32_le(bytes, off + 1 + 12)?;
            if w < 0 || h < 0 {
                return None;
            }
            let stride = ceil_to_multiple(w, 8);
            let data_len = (stride as usize).checked_mul(h as usize)?;
            let base = 1usize + 4 * 4;
            Some(base.checked_add(data_len)?)
        }
        SPT_MESH => {
            let vertex_cnt = read_i32_le(bytes, off + 1)? as i64;
            let tri_cnt = read_i32_le(bytes, off + 5)? as i64;
            if vertex_cnt < 0 || tri_cnt < 0 {
                return None;
            }
            let vertex_cnt = usize::try_from(vertex_cnt).ok()?;
            let tri_cnt = usize::try_from(tri_cnt).ok()?;
            let verts = vertex_cnt.checked_mul(12)?;
            let tris = tri_cnt.checked_mul(16)?;
            let base = 1usize + 4 + 4;
            Some(base.checked_add(verts)?.checked_add(tris)?)
        }
        SPT_SHIFTABLE_MESH => {
            let vertex_cnt = read_i32_le(bytes, off + 1 + 12)? as i64;
            let tri_cnt = read_i32_le(bytes, off + 1 + 16)? as i64;
            if vertex_cnt < 0 || tri_cnt < 0 {
                return None;
            }
            let vertex_cnt = usize::try_from(vertex_cnt).ok()?;
            let tri_cnt = usize::try_from(tri_cnt).ok()?;
            let verts = vertex_cnt.checked_mul(12)?;
            let tris = tri_cnt.checked_mul(16)?;
            let base = 1usize + 4 * 5;
            Some(base.checked_add(verts)?.checked_add(tris)?)
        }
        // Unhandled/unknown.
        _ => None,
    }
}

fn sprite_try_parse_to_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut off = start;
    let mut steps = 0usize;
    while off < bytes.len() {
        steps += 1;
        if steps > 50_000 {
            return None;
        }

        let t = bytes.get(off).copied()? & SPG_TYPE_MASK;
        let sz = sprite_elem_size(bytes, off)?;
        let next = off.checked_add(sz)?;
        if next > bytes.len() {
            return None;
        }
        off = next;

        if t == SPT_END {
            return Some(off);
        }
    }
    None
}

fn sprite_best_start(bytes: &[u8]) -> usize {
    let max = bytes.len().min(256);
    for start in 0..max {
        let t = bytes[start] & SPG_TYPE_MASK;
        if t > SPT_TEXT_DIAMOND {
            continue;
        }
        let Some(end) = sprite_try_parse_to_end(bytes, start) else {
            continue;
        };
        if end == bytes.len()
            || bytes
                .get(end..)
                .is_some_and(|tail| tail.iter().all(|&b| b == 0))
        {
            return start;
        }
    }
    0
}

pub fn sprite_bounds(bytes: &[u8]) -> Option<SpriteBounds> {
    let start = sprite_best_start(bytes);
    sprite_bounds_from(bytes, start)
}

pub fn sprite_is_valid(bytes: &[u8]) -> bool {
    let start = sprite_best_start(bytes);
    let Some(end) = sprite_try_parse_to_end(bytes, start) else {
        return false;
    };
    end == bytes.len() || bytes[end..].iter().all(|&b| b == 0)
}

pub fn sprite_parse_end_at_start(bytes: &[u8]) -> Option<usize> {
    sprite_try_parse_to_end(bytes, 0)
}

pub fn sprite_parse_last_good_prefix_len_at_start(bytes: &[u8]) -> Option<usize> {
    let mut off = 0usize;
    let mut last = 0usize;
    let mut steps = 0usize;

    while off < bytes.len() {
        steps += 1;
        if steps > 50_000 {
            break;
        }

        let Some(sz) = sprite_elem_size(bytes, off) else {
            break;
        };
        let next = off.checked_add(sz)?;
        if next > bytes.len() {
            break;
        }
        off = next;
        last = off;
    }

    if last == 0 { None } else { Some(last) }
}

pub fn sprite_is_valid_at_start(bytes: &[u8]) -> bool {
    let Some(end) = sprite_try_parse_to_end(bytes, 0) else {
        return false;
    };
    end == bytes.len() || bytes[end..].iter().all(|&b| b == 0)
}

fn sprite_bounds_from(bytes: &[u8], start: usize) -> Option<SpriteBounds> {
    let mut off = start;
    let mut dx: i32 = 0;
    let mut dy: i32 = 0;
    let mut bounds = SpriteBounds::default();

    while off < bytes.len() {
        let t = bytes[off] & SPG_TYPE_MASK;
        match t {
            SPT_END => break,
            SPT_SHIFT => {
                let x = read_i32_le(bytes, off + 1)?;
                let y = read_i32_le(bytes, off + 5)?;
                dx = dx.saturating_add(x);
                dy = dy.saturating_add(y);
            }
            SPT_PT => {
                let x = read_i32_le(bytes, off + 1)?;
                let y = read_i32_le(bytes, off + 5)?;
                bounds.include_point(dx.saturating_add(x), dy.saturating_add(y));
            }
            SPT_LINE | SPT_ARROW | SPT_RECT => {
                let x1 = read_i32_le(bytes, off + 1)?;
                let y1 = read_i32_le(bytes, off + 5)?;
                let x2 = read_i32_le(bytes, off + 9)?;
                let y2 = read_i32_le(bytes, off + 13)?;

                let (xa, xb) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
                let (ya, yb) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
                bounds.include_rect(
                    dx.saturating_add(xa),
                    dy.saturating_add(ya),
                    xb.saturating_sub(xa).saturating_add(1),
                    yb.saturating_sub(ya).saturating_add(1),
                );
            }
            SPT_CIRCLE => {
                let x = read_i32_le(bytes, off + 1)?;
                let y = read_i32_le(bytes, off + 5)?;
                let r = read_i32_le(bytes, off + 9)?;
                bounds.include_rect(
                    dx.saturating_add(x).saturating_sub(r),
                    dy.saturating_add(y).saturating_sub(r),
                    r.saturating_mul(2).saturating_add(1),
                    r.saturating_mul(2).saturating_add(1),
                );
            }
            SPT_TEXT | SPT_TEXT_DIAMOND | SPT_TEXT_BOX => {
                let x = read_i32_le(bytes, off + 1)?;
                let y = read_i32_le(bytes, off + 5)?;
                let base = off + 1 + 4 + 4;
                let s = sprite_text_nul_range(bytes, base)?;

                let (w, h) = measure_text_box(&bytes[s.clone()]);
                let (mut bx, mut by) = (dx.saturating_add(x), dy.saturating_add(y));
                let (mut bw, mut bh) = (w, h);
                if t == SPT_TEXT_BOX {
                    let border = 2;
                    bx = bx.saturating_sub(border);
                    by = by.saturating_sub(border);
                    bw = bw.saturating_add(border * 2);
                    bh = bh.saturating_add(border * 2);
                }
                bounds.include_rect(bx, by, bw, bh);
            }
            SPT_BITMAP => {
                let x = read_i32_le(bytes, off + 1)?;
                let y = read_i32_le(bytes, off + 5)?;
                let w = read_i32_le(bytes, off + 9)?;
                let h = read_i32_le(bytes, off + 13)?;
                bounds.include_rect(dx.saturating_add(x), dy.saturating_add(y), w, h);
            }
            SPT_MESH => {
                let vertex_cnt = read_i32_le(bytes, off + 1)? as i64;
                if vertex_cnt > 0 && vertex_cnt <= 65_536 {
                    let vertex_cnt = vertex_cnt as usize;
                    let base = off + 1 + 4 + 4;
                    for i in 0..vertex_cnt {
                        let v_off = base + i * 12;
                        let Some(x) = read_i32_le(bytes, v_off) else {
                            break;
                        };
                        let Some(y) = read_i32_le(bytes, v_off + 4) else {
                            break;
                        };
                        let x = decode_mesh_coord(x);
                        let y = decode_mesh_coord(y);
                        bounds.include_point(dx.saturating_add(x), dy.saturating_add(y));
                    }
                }
            }
            SPT_SHIFTABLE_MESH => {
                let tx = read_i32_le(bytes, off + 1)?;
                let ty = read_i32_le(bytes, off + 5)?;
                let vertex_cnt = read_i32_le(bytes, off + 1 + 12)? as i64;
                if vertex_cnt > 0 && vertex_cnt <= 65_536 {
                    let vertex_cnt = vertex_cnt as usize;
                    let base = off + 1 + 4 * 5;
                    for i in 0..vertex_cnt {
                        let v_off = base + i * 12;
                        let Some(x) = read_i32_le(bytes, v_off) else {
                            break;
                        };
                        let Some(y) = read_i32_le(bytes, v_off + 4) else {
                            break;
                        };
                        let x = decode_mesh_coord(x);
                        let y = decode_mesh_coord(y);
                        bounds.include_point(
                            dx.saturating_add(tx).saturating_add(x),
                            dy.saturating_add(ty).saturating_add(y),
                        );
                    }
                }
            }
            _ => {}
        }

        let Some(sz) = sprite_elem_size(bytes, off) else {
            break;
        };
        off += sz;
    }

    Some(bounds)
}

fn measure_text_box(text: &[u8]) -> (i32, i32) {
    let mut w: i32 = 0;
    let mut w_max: i32 = 0;
    let mut h: i32 = 8;

    for &b in text {
        match b {
            b'\n' => {
                w_max = w_max.max(w);
                w = 0;
                h = h.saturating_add(8);
            }
            b'\t' => {
                w = ceil_to_multiple(w.saturating_add(8), 8 * 8);
            }
            _ => w = w.saturating_add(8),
        }
    }

    w_max = w_max.max(w);
    (w_max, h)
}

fn draw_char_transparent_8x8(target: &mut impl SpriteTarget, x: i32, y: i32, color: u8, ch: u8) {
    for row in 0..8i32 {
        let row_bits = assets::sys_font_std_glyph_row_bits(ch, row as u8);
        for col in 0..8i32 {
            if (row_bits & (1u8 << col as u8)) != 0 {
                target.set_pixel(x + col, y + row, color);
            }
        }
    }
}

fn draw_text_transparent_8x8(
    target: &mut impl SpriteTarget,
    x: i32,
    y: i32,
    color: u8,
    text: &[u8],
) {
    let mut cx = x;
    let mut cy = y;
    let base_x = x;

    for &b in text {
        match b {
            b'\n' => {
                cx = base_x;
                cy = cy.saturating_add(8);
            }
            b'\t' => {
                let rel = cx.saturating_sub(base_x);
                let next = ceil_to_multiple(rel.saturating_add(8), 8 * 8);
                cx = base_x.saturating_add(next);
            }
            b'\r' => cx = base_x,
            _ => {
                draw_char_transparent_8x8(target, cx, cy, color, b);
                cx = cx.saturating_add(8);
            }
        }
    }
}

pub fn sprite_render(target: &mut impl SpriteTarget, base_x: i32, base_y: i32, bytes: &[u8]) {
    sprite_render_with_state(target, base_x, base_y, bytes, 15, 1);
}

pub fn sprite_render_with_state(
    target: &mut impl SpriteTarget,
    base_x: i32,
    base_y: i32,
    bytes: &[u8],
    initial_color: u8,
    initial_thick: i32,
) {
    let start = sprite_best_start(bytes);
    sprite_render_from(
        target,
        base_x,
        base_y,
        bytes,
        start,
        initial_color,
        initial_thick,
    );
}

fn sprite_render_from(
    target: &mut impl SpriteTarget,
    base_x: i32,
    base_y: i32,
    bytes: &[u8],
    start: usize,
    initial_color: u8,
    initial_thick: i32,
) {
    let mut off = start;
    let mut color: u8 = initial_color & 0x0f;
    let mut thick: i32 = initial_thick.max(1);
    let mut dx: i32 = 0;
    let mut dy: i32 = 0;

    while off < bytes.len() {
        let t = bytes[off] & SPG_TYPE_MASK;
        match t {
            SPT_END => break,
            SPT_COLOR => {
                if let Some(&c) = bytes.get(off + 1) {
                    color = c & 0x0f;
                }
            }
            SPT_DITHER_COLOR => {
                if let Some(v) = read_u16_le(bytes, off + 1) {
                    color = (v as u8) & 0x0f;
                }
            }
            SPT_THICK => {
                if let Some(v) = read_i32_le(bytes, off + 1) {
                    thick = v.max(1);
                }
            }
            SPT_SHIFT => {
                let Some(x) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                dx = dx.saturating_add(x);
                dy = dy.saturating_add(y);
            }
            SPT_PT => {
                let Some(x) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                if thick == 1 {
                    target.set_pixel(base_x + dx + x, base_y + dy + y, color);
                } else {
                    let half = thick / 2;
                    target.fill_rect(
                        base_x + dx + x - half,
                        base_y + dy + y - half,
                        thick,
                        thick,
                        color,
                    );
                }
            }
            SPT_LINE => {
                let Some(x1) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y1) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let Some(x2) = read_i32_le(bytes, off + 9) else {
                    break;
                };
                let Some(y2) = read_i32_le(bytes, off + 13) else {
                    break;
                };
                target.draw_line_thick(
                    base_x + dx + x1,
                    base_y + dy + y1,
                    base_x + dx + x2,
                    base_y + dy + y2,
                    color,
                    thick,
                );
            }
            SPT_RECT => {
                let Some(x1) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y1) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let Some(x2) = read_i32_le(bytes, off + 9) else {
                    break;
                };
                let Some(y2) = read_i32_le(bytes, off + 13) else {
                    break;
                };
                target.draw_rect_outline_thick(
                    base_x + dx + x1,
                    base_y + dy + y1,
                    x2 - x1,
                    y2 - y1,
                    color,
                    thick,
                );
            }
            SPT_CIRCLE => {
                let Some(x) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let Some(r) = read_i32_le(bytes, off + 9) else {
                    break;
                };
                target.draw_circle_thick(base_x + dx + x, base_y + dy + y, r, color, thick);
            }
            SPT_ARROW => {
                let Some(x1) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y1) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let Some(x2) = read_i32_le(bytes, off + 9) else {
                    break;
                };
                let Some(y2) = read_i32_le(bytes, off + 13) else {
                    break;
                };
                draw_arrow(
                    target,
                    base_x + dx + x1,
                    base_y + dy + y1,
                    base_x + dx + x2,
                    base_y + dy + y2,
                    color,
                    thick,
                );
            }
            SPT_TEXT | SPT_TEXT_DIAMOND => {
                let Some(x) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let base = off + 1 + 4 + 4;
                let Some(s) = sprite_text_nul_range(bytes, base) else {
                    break;
                };
                draw_text_transparent_8x8(
                    target,
                    base_x + dx + x,
                    base_y + dy + y,
                    color,
                    &bytes[s],
                );
            }
            SPT_TEXT_BOX => {
                let Some(x) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let base = off + 1 + 4 + 4;
                let Some(s) = sprite_text_nul_range(bytes, base) else {
                    break;
                };
                let text = &bytes[s];
                draw_text_transparent_8x8(target, base_x + dx + x, base_y + dy + y, color, text);
                let (w, h) = measure_text_box(text);
                let border = 2;
                target.draw_rect_outline_thick(
                    base_x + dx + x - border,
                    base_y + dy + y - border,
                    w + border * 2,
                    h + border * 2,
                    color,
                    thick,
                );
            }
            SPT_BITMAP => {
                let Some(x) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(y) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let Some(w) = read_i32_le(bytes, off + 9) else {
                    break;
                };
                let Some(h) = read_i32_le(bytes, off + 13) else {
                    break;
                };
                if w <= 0 || h <= 0 {
                    // nothing
                } else {
                    let stride = ceil_to_multiple(w, 8);
                    let data_off = off + 1 + 4 * 4;
                    let data_len = match (stride as usize).checked_mul(h as usize) {
                        Some(v) => v,
                        None => break,
                    };
                    let Some(src) = bytes.get(data_off..data_off + data_len) else {
                        break;
                    };
                    target.blit_8bpp(base_x + dx + x, base_y + dy + y, w, h, stride, src);
                }
            }
            SPT_MESH => {
                let Some(vertex_cnt) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(tri_cnt) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                if vertex_cnt <= 0 || tri_cnt <= 0 {
                    // nothing
                } else {
                    let vertex_cnt = vertex_cnt as usize;
                    let tri_cnt = tri_cnt as usize;
                    if vertex_cnt <= 65_536 && tri_cnt <= 65_536 {
                        let verts_off = off + 1 + 4 + 4;
                        let verts_bytes = vertex_cnt.saturating_mul(12);
                        let tris_off = verts_off.saturating_add(verts_bytes);
                        let tris_bytes = tri_cnt.saturating_mul(16);
                        let Some(verts) = bytes.get(verts_off..tris_off) else {
                            break;
                        };
                        let Some(tris) = bytes.get(tris_off..tris_off + tris_bytes) else {
                            break;
                        };
                        render_mesh(
                            target,
                            base_x.saturating_add(dx),
                            base_y.saturating_add(dy),
                            verts,
                            tris,
                        );
                    }
                }
            }
            SPT_SHIFTABLE_MESH => {
                let Some(tx) = read_i32_le(bytes, off + 1) else {
                    break;
                };
                let Some(ty) = read_i32_le(bytes, off + 5) else {
                    break;
                };
                let Some(vertex_cnt) = read_i32_le(bytes, off + 1 + 12) else {
                    break;
                };
                let Some(tri_cnt) = read_i32_le(bytes, off + 1 + 16) else {
                    break;
                };
                if vertex_cnt <= 0 || tri_cnt <= 0 {
                    // nothing
                } else {
                    let vertex_cnt = vertex_cnt as usize;
                    let tri_cnt = tri_cnt as usize;
                    if vertex_cnt <= 65_536 && tri_cnt <= 65_536 {
                        let verts_off = off + 1 + 4 * 5;
                        let verts_bytes = vertex_cnt.saturating_mul(12);
                        let tris_off = verts_off.saturating_add(verts_bytes);
                        let tris_bytes = tri_cnt.saturating_mul(16);
                        let Some(verts) = bytes.get(verts_off..tris_off) else {
                            break;
                        };
                        let Some(tris) = bytes.get(tris_off..tris_off + tris_bytes) else {
                            break;
                        };
                        render_mesh(
                            target,
                            base_x.saturating_add(dx).saturating_add(tx),
                            base_y.saturating_add(dy).saturating_add(ty),
                            verts,
                            tris,
                        );
                    }
                }
            }
            _ => {}
        }

        let Some(sz) = sprite_elem_size(bytes, off) else {
            break;
        };
        off += sz;
    }
}

fn render_mesh(
    target: &mut impl SpriteTarget,
    base_x: i32,
    base_y: i32,
    verts: &[u8],
    tris: &[u8],
) {
    fn edge(ax: i32, ay: i32, bx: i32, by: i32, cx: i32, cy: i32) -> i64 {
        (cx as i64 - ax as i64) * (by as i64 - ay as i64)
            - (cy as i64 - ay as i64) * (bx as i64 - ax as i64)
    }

    fn fill_tri(
        target: &mut impl SpriteTarget,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u8,
    ) {
        let min_x = x0.min(x1).min(x2);
        let max_x = x0.max(x1).max(x2);
        let min_y = y0.min(y1).min(y2);
        let max_y = y0.max(y1).max(y2);

        let area = edge(x0, y0, x1, y1, x2, y2);
        if area == 0 {
            return;
        }

        // Clamp degenerate / huge triangles to avoid pathological slow paths.
        let w = max_x.saturating_sub(min_x).saturating_add(1);
        let h = max_y.saturating_sub(min_y).saturating_add(1);
        if w as i64 * h as i64 > 500_000 {
            return;
        }

        let wants_pos = area > 0;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let w0 = edge(x1, y1, x2, y2, x, y);
                let w1 = edge(x2, y2, x0, y0, x, y);
                let w2 = edge(x0, y0, x1, y1, x, y);
                let inside = if wants_pos {
                    w0 >= 0 && w1 >= 0 && w2 >= 0
                } else {
                    w0 <= 0 && w1 <= 0 && w2 <= 0
                };
                if inside {
                    target.set_pixel(x, y, color);
                }
            }
        }
    }

    let vertex_cnt = verts.len() / 12;
    let tri_cnt = tris.len() / 16;
    if vertex_cnt == 0 || tri_cnt == 0 {
        return;
    }

    // Decode vertices once. Some vendored TempleOS sprites (notably PersonalMenu mesh icons) store
    // coordinates/indices shifted into higher bytes; decode them into a small signed range so bounds
    // and rendering behave reasonably.
    let mut vxs: Vec<i32> = Vec::with_capacity(vertex_cnt);
    let mut vys: Vec<i32> = Vec::with_capacity(vertex_cnt);
    for i in 0..vertex_cnt {
        let base = i * 12;
        let Some(x_raw) = read_i32_le(verts, base) else {
            break;
        };
        let Some(y_raw) = read_i32_le(verts, base + 4) else {
            break;
        };
        vxs.push(decode_mesh_coord(x_raw));
        vys.push(decode_mesh_coord(y_raw));
    }
    if vxs.len() != vertex_cnt || vys.len() != vertex_cnt {
        return;
    }

    for t in 0..tri_cnt {
        let base = t * 16;
        let Some(tri_color_raw) = read_i32_le(tris, base) else {
            continue;
        };
        let Some(i0_raw) = read_i32_le(tris, base + 4) else {
            continue;
        };
        let Some(i1_raw) = read_i32_le(tris, base + 8) else {
            continue;
        };
        let Some(i2_raw) = read_i32_le(tris, base + 12) else {
            continue;
        };

        let Some(i0) = decode_mesh_index(i0_raw, vertex_cnt) else {
            continue;
        };
        let Some(i1) = decode_mesh_index(i1_raw, vertex_cnt) else {
            continue;
        };
        let Some(i2) = decode_mesh_index(i2_raw, vertex_cnt) else {
            continue;
        };

        let color = decode_mesh_color(tri_color_raw);
        fill_tri(
            target,
            base_x.saturating_add(vxs[i0]),
            base_y.saturating_add(vys[i0]),
            base_x.saturating_add(vxs[i1]),
            base_y.saturating_add(vys[i1]),
            base_x.saturating_add(vxs[i2]),
            base_y.saturating_add(vys[i2]),
            color,
        );
    }
}

fn decode_mesh_coord(raw: i32) -> i32 {
    // Most TempleOS sprites store coordinates directly as small signed I32s.
    //
    // Some vendored mesh icons (notably in `::/PersonalMenu.DD`) appear to store coordinates in a
    // single byte/word, leaving the remaining bytes as 0x00/0xff. When interpreted as a normal
    // little-endian I32, this yields huge magnitudes (or misleading values like 255) which break
    // sprite bounds/layout and can cause large “glitch triangles”.
    //
    // Heuristic:
    // - If the value is already in a reasonable range, keep it, *unless* it looks like a packed
    //   single-byte value shifted into a higher byte (common in PersonalMenu mesh icons).
    // - Otherwise, prefer decoding from an obvious single non-{00,FF} byte (or a single byte that
    //   differs from all-FF or all-00 patterns).
    // - Fall back to the older shift/byteswap trick when needed.
    let bytes = (raw as u32).to_le_bytes();

    if (-4096..=4096).contains(&raw) {
        // Some packed values still fall within our "reasonable range" (e.g. 0x00001000 == 4096),
        // so detect the common "one informative byte + padding" encoding before returning `raw`.
        if bytes[0] == 0x00 || bytes[0] == 0xff {
            let mut informative: Option<u8> = None;
            let mut informative_cnt = 0usize;
            for &b in &bytes {
                if b == 0x00 || b == 0xff {
                    continue;
                }
                informative = Some(b);
                informative_cnt += 1;
            }
            if informative_cnt == 1 {
                return informative.unwrap_or(0) as i8 as i32;
            }

            // All bytes are 0x00/0xff (e.g. 0x000000ff, 0xffffff00). Some vendored mesh icons
            // appear to store the signed coord in the "next" byte, leaving the low byte as 0xff
            // or 0x00 padding.
            if informative_cnt == 0 {
                return bytes[1] as i8 as i32;
            }
        }
        return raw;
    }

    let mut informative: Option<u8> = None;
    let mut informative_cnt = 0usize;
    for &b in &bytes {
        if b == 0x00 || b == 0xff {
            continue;
        }
        informative = Some(b);
        informative_cnt += 1;
    }
    if informative_cnt == 1 {
        return informative.unwrap_or(0) as i8 as i32;
    }

    let mut non_ff: Option<u8> = None;
    let mut non_ff_cnt = 0usize;
    for &b in &bytes {
        if b == 0xff {
            continue;
        }
        non_ff = Some(b);
        non_ff_cnt += 1;
    }
    if non_ff_cnt == 1 {
        return non_ff.unwrap_or(0) as i8 as i32;
    }

    let mut non_00: Option<u8> = None;
    let mut non_00_cnt = 0usize;
    for &b in &bytes {
        if b == 0x00 {
            continue;
        }
        non_00 = Some(b);
        non_00_cnt += 1;
    }
    if non_00_cnt == 1 {
        return non_00.unwrap_or(0) as i8 as i32;
    }

    const LIMIT: i32 = 4096;
    let bs = raw.swap_bytes();
    let cands = [
        raw,
        raw >> 8,
        raw >> 16,
        raw >> 24,
        bs,
        bs >> 8,
        bs >> 16,
        bs >> 24,
    ];
    let mut best: Option<i32> = None;
    for &v in &cands {
        if (-LIMIT..=LIMIT).contains(&v) {
            best = Some(match best {
                None => v,
                Some(prev) if v.abs() < prev.abs() => v,
                Some(prev) => prev,
            });
        }
    }
    best.unwrap_or(raw)
}

fn decode_mesh_index(raw: i32, vertex_cnt: usize) -> Option<usize> {
    let vc = u32::try_from(vertex_cnt).ok()?;
    let raw = raw as u32;
    let bs = raw.swap_bytes();
    let cands = [
        raw,
        raw >> 8,
        raw >> 16,
        raw >> 24,
        bs,
        bs >> 8,
        bs >> 16,
        bs >> 24,
    ];
    for &v in &cands {
        if v < vc {
            return Some(v as usize);
        }
    }
    None
}

fn decode_mesh_color(raw: i32) -> u8 {
    let raw = raw as u32;
    let bs = raw.swap_bytes();
    let cands = [
        raw,
        raw >> 8,
        raw >> 16,
        raw >> 24,
        bs,
        bs >> 8,
        bs >> 16,
        bs >> 24,
    ];
    for &v in &cands {
        if v <= 15 {
            return (v & 0x0f) as u8;
        }
    }
    (raw & 0x0f) as u8
}

fn draw_arrow(
    target: &mut impl SpriteTarget,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u8,
    thick: i32,
) {
    target.draw_line_thick(x1, y1, x2, y2, color, thick);

    let dx = (x2 - x1) as f64;
    let dy = (y2 - y1) as f64;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0001 {
        return;
    }

    let ux = dx / len;
    let uy = dy / len;
    let px = -uy;
    let py = ux;

    let arrow_len = (6 * thick.max(1)) as f64;
    let arrow_w = (4 * thick.max(1)) as f64;

    let ax1 = (x2 as f64 - ux * arrow_len + px * arrow_w).round() as i32;
    let ay1 = (y2 as f64 - uy * arrow_len + py * arrow_w).round() as i32;
    let ax2 = (x2 as f64 - ux * arrow_len - px * arrow_w).round() as i32;
    let ay2 = (y2 as f64 - uy * arrow_len - py * arrow_w).round() as i32;

    target.draw_line_thick(ax1, ay1, x2, y2, color, thick);
    target.draw_line_thick(ax2, ay2, x2, y2, color, thick);
}

fn draw_line_thick_default<T: SpriteTarget + ?Sized>(
    target: &mut T,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u8,
    thick: i32,
) {
    let thick = thick.max(1);
    let half = thick / 2;

    let mut x = x1;
    let mut y = y1;
    let dx = (x2 - x1).abs();
    let sx = if x1 < x2 { 1 } else { -1 };
    let dy = -(y2 - y1).abs();
    let sy = if y1 < y2 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if thick == 1 {
            target.set_pixel(x, y, color);
        } else {
            target.fill_rect(x - half, y - half, thick, thick, color);
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

fn draw_rect_outline_thick_default<T: SpriteTarget + ?Sized>(
    target: &mut T,
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
        target.fill_rect(x, y, w, h, color);
        return;
    }

    target.fill_rect(x, y, w, thick, color);
    target.fill_rect(x, y + h - thick, w, thick, color);
    target.fill_rect(x, y + thick, thick, h - 2 * thick, color);
    target.fill_rect(x + w - thick, y + thick, thick, h - 2 * thick, color);
}

fn draw_circle_thick_default<T: SpriteTarget + ?Sized>(
    target: &mut T,
    cx: i32,
    cy: i32,
    r: i32,
    color: u8,
    thick: i32,
) {
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
                target.set_pixel(px, py, color);
            } else {
                target.fill_rect(px - half, py - half, thick, thick, color);
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

impl SpriteTarget for crate::rt::TempleRt {
    fn set_pixel(&mut self, x: i32, y: i32, color: u8) {
        crate::rt::TempleRt::set_pixel(self, x, y, color);
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        crate::rt::TempleRt::fill_rect(self, x, y, w, h, color);
    }

    fn draw_line_thick(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, color: u8, thick: i32) {
        crate::rt::TempleRt::draw_line_thick(self, x1, y1, x2, y2, color, thick);
    }

    fn draw_rect_outline_thick(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8, thick: i32) {
        crate::rt::TempleRt::draw_rect_outline_thick(self, x, y, w, h, color, thick);
    }

    fn draw_circle_thick(&mut self, cx: i32, cy: i32, r: i32, color: u8, thick: i32) {
        crate::rt::TempleRt::draw_circle_thick(self, cx, cy, r, color, thick);
    }

    fn blit_8bpp(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        src_w: i32,
        src_h: i32,
        stride: i32,
        src: &[u8],
    ) {
        if src_w <= 0 || src_h <= 0 || stride <= 0 {
            return;
        }
        let w = src_w as usize;
        let stride = stride as usize;
        for row in 0..(src_h as usize) {
            let start = match row.checked_mul(stride).and_then(|v| v.checked_add(0)) {
                Some(v) => v,
                None => return,
            };
            let Some(row_src) = src.get(start..start + w) else {
                return;
            };
            crate::rt::TempleRt::blit_8bpp_transparent(
                self,
                dst_x,
                dst_y + row as i32,
                src_w,
                1,
                row_src,
                0xFF,
            );
        }
    }
}
