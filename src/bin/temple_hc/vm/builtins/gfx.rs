use super::super::prelude::*;
use super::super::{Value, Vm};

impl Vm {
    pub(super) fn call_builtin_gfx(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        match name {
            "GrPlot" => {
                let (dc, x, y) = match args.len() {
                    2 => (Value::Obj(self.dc_alias.clone()), &args[0], &args[1]),
                    3 if matches!(args[0], Expr::DefaultArg) => {
                        (Value::Obj(self.dc_alias.clone()), &args[1], &args[2])
                    }
                    3 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2])
                    }
                    _ => return Err("GrPlot(dc?, x, y) expects 2 or 3 args".to_string()),
                };

                let x = self.eval_expr(x)?.as_i64()? as i32;
                let y = self.eval_expr(y)?.as_i64()? as i32;

                let color = match dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };

                self.rt.set_pixel(x, y, color);
                Ok(Value::Void)
            }
            "GrLine" => {
                let (dc, x1, y1, x2, y2, thick) = match args.len() {
                    4 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[2],
                        &args[3],
                        None,
                    ),
                    5 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        &args[4],
                        None,
                    ),
                    5 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], &args[4], None)
                    }
                    6 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        &args[4],
                        Some(&args[5]),
                    ),
                    6 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], &args[4], Some(&args[5]))
                    }
                    _ => {
                        return Err(
                            "GrLine(dc?, x1, y1, x2, y2, thick?) expects 4-6 args".to_string()
                        );
                    }
                };

                let x1 = self.eval_expr(x1)?.as_i64()? as i32;
                let y1 = self.eval_expr(y1)?.as_i64()? as i32;
                let x2 = self.eval_expr(x2)?.as_i64()? as i32;
                let y2 = self.eval_expr(y2)?.as_i64()? as i32;
                let thick = match thick {
                    Some(e) => self.eval_expr(e)?.as_i64()? as i32,
                    None => match &dc {
                        Value::Obj(dc) => dc
                            .borrow()
                            .fields
                            .get("thick")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(1) as i32,
                        _ => 1,
                    },
                }
                .max(1);

                let color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };

                self.rt.draw_line_thick(x1, y1, x2, y2, color, thick);

                Ok(Value::Void)
            }
            "GrLine3" => {
                // GrLine3(dc?, x1, y1, z1, x2, y2, z2, thick?) — ignore Z.
                let (dc, x1, y1, x2, y2, thick) = match args.len() {
                    6 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[3],
                        &args[4],
                        None,
                    ),
                    7 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[4],
                        &args[5],
                        None,
                    ),
                    7 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[4], &args[5], None)
                    }
                    8 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[4],
                        &args[5],
                        Some(&args[7]),
                    ),
                    8 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[4], &args[5], Some(&args[7]))
                    }
                    _ => {
                        return Err(
                            "GrLine3(dc?, x1, y1, z1, x2, y2, z2, thick?) expects 6-8 args"
                                .to_string(),
                        );
                    }
                };

                let x1 = self.eval_expr(x1)?.as_i64()? as i32;
                let y1 = self.eval_expr(y1)?.as_i64()? as i32;
                let x2 = self.eval_expr(x2)?.as_i64()? as i32;
                let y2 = self.eval_expr(y2)?.as_i64()? as i32;
                let thick = match thick {
                    Some(e) => self.eval_expr(e)?.as_i64()? as i32,
                    None => match &dc {
                        Value::Obj(dc) => dc
                            .borrow()
                            .fields
                            .get("thick")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(1) as i32,
                        _ => 1,
                    },
                }
                .max(1);

                let color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };

                self.rt.draw_line_thick(x1, y1, x2, y2, color, thick);
                Ok(Value::Void)
            }
            "GrBorder" => {
                // GrBorder(dc?, x1, y1, x2, y2) — draw an outline rectangle.
                let (dc, x1, y1, x2, y2) = match args.len() {
                    4 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[2],
                        &args[3],
                    ),
                    5 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        &args[4],
                    ),
                    5 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], &args[4])
                    }
                    _ => {
                        return Err("GrBorder(dc?, x1, y1, x2, y2) expects 4 or 5 args".to_string());
                    }
                };

                let x1 = self.eval_expr(x1)?.as_i64()? as i32;
                let y1 = self.eval_expr(y1)?.as_i64()? as i32;
                let x2 = self.eval_expr(x2)?.as_i64()? as i32;
                let y2 = self.eval_expr(y2)?.as_i64()? as i32;

                let thick = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("thick")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(1) as i32,
                    _ => 1,
                }
                .max(1);

                let color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };

                let w = x2.saturating_sub(x1);
                let h = y2.saturating_sub(y1);
                self.rt.draw_rect_outline_thick(x1, y1, w, h, color, thick);
                Ok(Value::Void)
            }
            "GrEllipse" => {
                // GrEllipse(dc?, x, y, r1, r2) — approximate with a polyline.
                let (dc, x, y, r1, r2) = match args.len() {
                    4 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[2],
                        &args[3],
                    ),
                    5 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        &args[4],
                    ),
                    5 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], &args[4])
                    }
                    _ => return Err("GrEllipse(dc?, x, y, r1, r2) expects 4 or 5 args".to_string()),
                };

                let x = self.eval_expr(x)?.as_f64()? as f64;
                let y = self.eval_expr(y)?.as_f64()? as f64;
                let r1 = self.eval_expr(r1)?.as_f64()?.abs();
                let r2 = self.eval_expr(r2)?.as_f64()?.abs();

                let thick = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("thick")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(1) as i32,
                    _ => 1,
                }
                .max(1);

                let color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };

                if r1 <= 0.0 || r2 <= 0.0 {
                    return Ok(Value::Void);
                }

                let steps = (((r1 + r2) * 0.5).round() as i32).clamp(12, 256) * 4;
                let mut prev: Option<(i32, i32)> = None;
                for i in 0..=steps {
                    let t = (i as f64) / (steps as f64);
                    let a = t * std::f64::consts::TAU;
                    let px = (x + a.cos() * r1).round() as i32;
                    let py = (y + a.sin() * r2).round() as i32;
                    if let Some((ox, oy)) = prev {
                        self.rt.draw_line_thick(ox, oy, px, py, color, thick);
                    }
                    prev = Some((px, py));
                }

                Ok(Value::Void)
            }
            "GrFloodFill" => {
                // GrFloodFill(dc?, x, y, ...) — currently a no-op (sprites cover most cases).
                // Keep it permissive: upstream sources use both 2D and 3D variants.
                if args.len() < 2 {
                    return Err("GrFloodFill(dc?, x, y, ...) expects at least 2 args".to_string());
                }
                for e in args {
                    let _ = self.eval_expr(e)?;
                }
                Ok(Value::Void)
            }
            "GrRect" => {
                let (dc, x, y, w, h, color) = match args.len() {
                    4 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[2],
                        &args[3],
                        None,
                    ),
                    5 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        &args[4],
                        None,
                    ),
                    5 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], &args[4], None)
                    }
                    6 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        &args[4],
                        Some(&args[5]),
                    ),
                    6 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], &args[4], Some(&args[5]))
                    }
                    _ => {
                        return Err("GrRect(dc?, x, y, w, h, color?) expects 4-6 args".to_string());
                    }
                };

                let x = self.eval_expr(x)?.as_i64()? as i32;
                let y = self.eval_expr(y)?.as_i64()? as i32;
                let w = self.eval_expr(w)?.as_i64()? as i32;
                let h = self.eval_expr(h)?.as_i64()? as i32;

                let thick = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("thick")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(1) as i32,
                    _ => 1,
                }
                .max(1);

                let default_color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };
                let color = color
                    .map(|e| self.eval_expr(e))
                    .transpose()?
                    .map(|v| v.as_i64())
                    .transpose()?
                    .map(|v| v as u8)
                    .unwrap_or(default_color);

                self.rt.draw_rect_outline_thick(x, y, w, h, color, thick);
                Ok(Value::Void)
            }
            "GrPrint" => {
                if args.len() < 3 {
                    return Err("GrPrint(dc?, x, y, fmt, ...) expects at least 3 args".to_string());
                }

                let (dc, x, y, fmt, rest_start) = if args.len() >= 4 {
                    if matches!(args[0], Expr::DefaultArg) {
                        (
                            Value::Obj(self.dc_alias.clone()),
                            &args[1],
                            &args[2],
                            &args[3],
                            4usize,
                        )
                    } else {
                        let first = self.eval_expr(&args[0])?;
                        if matches!(first, Value::Obj(_)) {
                            (first, &args[1], &args[2], &args[3], 4usize)
                        } else {
                            (
                                Value::Obj(self.dc_alias.clone()),
                                &args[0],
                                &args[1],
                                &args[2],
                                3usize,
                            )
                        }
                    }
                } else {
                    (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[2],
                        3usize,
                    )
                };

                let x = self.eval_expr(x)?.as_i64()? as i32;
                let y = self.eval_expr(y)?.as_i64()? as i32;

                let fmt_v = self.eval_expr(fmt)?;
                let fmt = match fmt_v {
                    Value::Str(s) => s,
                    Value::Int(0) => String::new(),
                    Value::Int(ptr) => self.read_cstr_lossy(ptr)?,
                    Value::Ptr { addr, .. } => self.read_cstr_lossy(addr)?,
                    other => {
                        return Err(format!(
                            "GrPrint: fmt must be a string or pointer, got {other:?}"
                        ));
                    }
                };

                let mut values: Vec<Value> = Vec::new();
                for expr in args.get(rest_start..).unwrap_or(&[]) {
                    if matches!(expr, Expr::DefaultArg) {
                        values.push(Value::Int(0));
                    } else {
                        values.push(self.eval_expr(expr)?);
                    }
                }

                let rendered = format_temple_fmt_with_cstr(
                    &fmt,
                    &values,
                    |ptr| self.read_cstr_lossy(ptr),
                    |idx, name| self.define_sub(idx, name),
                )?;

                let fg = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };
                let bg = 0u8;
                self.rt.draw_text(x, y, fg, bg, &rendered);
                Ok(Value::Void)
            }
            "GrPaletteColorSet" => {
                if args.len() != 2 {
                    return Err("GrPaletteColorSet(color_num, bgr48) expects 2 args".to_string());
                }

                let color_num = self.eval_expr(&args[0])?.as_i64()?;
                let color_index = color_num.clamp(0, 255) as u8;

                let v = self.eval_expr(&args[1])?;
                let (b, g, r) = match v {
                    Value::Int(bits) => {
                        let u = bits as u64;
                        let b = (u & 0xffff) as u16;
                        let g = ((u >> 16) & 0xffff) as u16;
                        let r = ((u >> 32) & 0xffff) as u16;
                        (b, g, r)
                    }
                    Value::Char(bits) => {
                        let u = bits;
                        let b = (u & 0xffff) as u16;
                        let g = ((u >> 16) & 0xffff) as u16;
                        let r = ((u >> 32) & 0xffff) as u16;
                        (b, g, r)
                    }
                    Value::Obj(obj) => {
                        let obj = obj.borrow();
                        let b = obj
                            .fields
                            .get("b")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(0) as u16;
                        let g = obj
                            .fields
                            .get("g")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(0) as u16;
                        let r = obj
                            .fields
                            .get("r")
                            .and_then(|v| v.as_i64().ok())
                            .unwrap_or(0) as u16;
                        (b, g, r)
                    }
                    other => {
                        return Err(format!(
                            "GrPaletteColorSet: expected CBGR48 obj or int bits, got {other:?}"
                        ));
                    }
                };

                let rgba = [(r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8, 255u8];
                self.rt
                    .palette_color_set(color_index, rgba)
                    .map_err(|e| e.to_string())?;
                Ok(Value::Void)
            }
            "GrCircle" => {
                let (dc, x, y, r, color) = match args.len() {
                    3 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[2],
                        None,
                    ),
                    4 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        None,
                    ),
                    4 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], None)
                    }
                    5 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        Some(&args[4]),
                    ),
                    5 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[3], Some(&args[4]))
                    }
                    // Some upstream programs pass angle ranges; we ignore them for now and draw
                    // a full circle.
                    6 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        if matches!(args[4], Expr::DefaultArg) {
                            None
                        } else {
                            Some(&args[4])
                        },
                    ),
                    6 => {
                        let dc = self.eval_expr(&args[0])?;
                        (
                            dc,
                            &args[1],
                            &args[2],
                            &args[3],
                            if matches!(args[4], Expr::DefaultArg) {
                                None
                            } else {
                                Some(&args[4])
                            },
                        )
                    }
                    7 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[3],
                        if matches!(args[4], Expr::DefaultArg) {
                            None
                        } else {
                            Some(&args[4])
                        },
                    ),
                    7 => {
                        let dc = self.eval_expr(&args[0])?;
                        (
                            dc,
                            &args[1],
                            &args[2],
                            &args[3],
                            if matches!(args[4], Expr::DefaultArg) {
                                None
                            } else {
                                Some(&args[4])
                            },
                        )
                    }
                    _ => {
                        return Err(
                            "GrCircle(dc?, x, y, r, color?, theta1?, theta2?) expects 3-7 args"
                                .to_string(),
                        );
                    }
                };

                let x = self.eval_expr(x)?.as_i64()? as i32;
                let y = self.eval_expr(y)?.as_i64()? as i32;
                let r = self.eval_expr(r)?.as_i64()? as i32;

                let thick = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("thick")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(1) as i32,
                    _ => 1,
                }
                .max(1);

                let default_color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };
                let color = color
                    .map(|e| self.eval_expr(e))
                    .transpose()?
                    .map(|v| v.as_i64())
                    .transpose()?
                    .map(|v| v as u8)
                    .unwrap_or(default_color);

                self.rt.draw_circle_thick(x, y, r, color, thick);
                Ok(Value::Void)
            }
            "GrCircle3" => {
                // GrCircle3(dc?, x, y, z, r, color?) — ignore Z.
                let (dc, x, y, r, color) = match args.len() {
                    4 => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[0],
                        &args[1],
                        &args[3],
                        None,
                    ),
                    5 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[4],
                        None,
                    ),
                    5 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[4], None)
                    }
                    6 if matches!(args[0], Expr::DefaultArg) => (
                        Value::Obj(self.dc_alias.clone()),
                        &args[1],
                        &args[2],
                        &args[4],
                        Some(&args[5]),
                    ),
                    6 => {
                        let dc = self.eval_expr(&args[0])?;
                        (dc, &args[1], &args[2], &args[4], Some(&args[5]))
                    }
                    _ => {
                        return Err(
                            "GrCircle3(dc?, x, y, z, r, color?) expects 4-6 args".to_string()
                        );
                    }
                };

                let x = self.eval_expr(x)?.as_i64()? as i32;
                let y = self.eval_expr(y)?.as_i64()? as i32;
                let r = self.eval_expr(r)?.as_i64()? as i32;

                let thick = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("thick")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(1) as i32,
                    _ => 1,
                }
                .max(1);

                let default_color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                };
                let color = color
                    .map(|e| self.eval_expr(e))
                    .transpose()?
                    .map(|v| v.as_i64())
                    .transpose()?
                    .map(|v| v as u8)
                    .unwrap_or(default_color);

                self.rt.draw_circle_thick(x, y, r, color, thick);
                Ok(Value::Void)
            }
            "DCDepthBufAlloc" => {
                if args.len() > 2 {
                    return Err("DCDepthBufAlloc(dc=gr.dc, flags=0) expects 0-2 args".to_string());
                }
                let _dc = match args.get(0) {
                    None | Some(Expr::DefaultArg) => Value::Obj(self.dc_alias.clone()),
                    Some(e) => self.eval_expr(e)?,
                };
                let _flags = match args.get(1) {
                    None | Some(Expr::DefaultArg) => Value::Int(0),
                    Some(e) => self.eval_expr(e)?,
                };
                Ok(Value::Void)
            }
            "D3I32Norm" => {
                if args.len() != 1 {
                    return Err("D3I32Norm(p) expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                let obj = match v {
                    Value::Obj(obj) => obj,
                    Value::ObjFieldRef { obj, field } => {
                        match obj.borrow().fields.get(&field).cloned() {
                            Some(Value::Obj(inner)) => inner,
                            _ => {
                                return Err("D3I32Norm: expected &obj_field with x/y/z".to_string());
                            }
                        }
                    }
                    Value::VarRef(name) => match self.env.get(&name) {
                        Some(Value::Obj(obj)) => obj,
                        _ => return Err("D3I32Norm: expected object".to_string()),
                    },
                    _ => return Err("D3I32Norm: expected object pointer".to_string()),
                };

                let o = obj.borrow();
                let x = o.fields.get("x").and_then(|v| v.as_i64().ok()).unwrap_or(0) as f64;
                let y = o.fields.get("y").and_then(|v| v.as_i64().ok()).unwrap_or(0) as f64;
                let z = o.fields.get("z").and_then(|v| v.as_i64().ok()).unwrap_or(0) as f64;
                Ok(Value::Float((x * x + y * y + z * z).sqrt()))
            }
            "SpriteInterpolate" => {
                if args.len() != 3 {
                    return Err("SpriteInterpolate(t, elems0, elems1) expects 3 args".to_string());
                }
                let t = self.eval_expr(&args[0])?.as_f64()?;
                let e0 = self.eval_expr(&args[1])?.as_i64()?;
                let e1 = self.eval_expr(&args[2])?.as_i64()?;
                if t < 0.5 {
                    Ok(Value::Int(e0))
                } else {
                    Ok(Value::Int(e1))
                }
            }
            "Sprite3YB" => {
                // Sprite3YB(dc=gr.dc, x, y, z, elems, angle=0) — ignore rotation and Z.
                if !(args.len() == 5 || args.len() == 6) {
                    return Err(
                        "Sprite3YB(dc, x, y, z, elems, angle?) expects 5-6 args".to_string()
                    );
                }

                let dc = self.eval_expr(&args[0])?;
                let x = self.eval_expr(&args[1])?.as_f64()? as i32;
                let y = self.eval_expr(&args[2])?.as_f64()? as i32;
                let _z = self.eval_expr(&args[3])?.as_f64()?;
                let elems = self.eval_expr(&args[4])?.as_i64()?;
                let _angle = if args.len() == 6 {
                    self.eval_expr(&args[5])?
                } else {
                    Value::Float(0.0)
                };

                if elems == 0 {
                    return Ok(Value::Void);
                }

                let initial_color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                } & 0x0f;
                let initial_thick = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("thick")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(1) as i32,
                    _ => 1,
                }
                .max(1);

                let bytes_vec = if let Some(&len) = self.doldoc_bin_len_by_ptr.get(&elems) {
                    self.heap_slice(elems, len)?.to_vec()
                } else {
                    self.heap_tail(elems)?.to_vec()
                };

                temple_rt::sprite::sprite_render_with_state(
                    &mut self.rt,
                    x,
                    y,
                    &bytes_vec,
                    initial_color,
                    initial_thick,
                );
                Ok(Value::Void)
            }
            "Sprite3" => {
                // Sprite3(dc=gr.dc, x, y, z, elems, just_one_elem=FALSE) — ignore Z and just_one_elem.
                if !(args.len() == 5 || args.len() == 6) {
                    return Err(
                        "Sprite3(dc, x, y, z, elems, just_one_elem?) expects 5-6 args".to_string(),
                    );
                }

                let dc = self.eval_expr(&args[0])?;
                let x = self.eval_expr(&args[1])?.as_f64()? as i32;
                let y = self.eval_expr(&args[2])?.as_f64()? as i32;
                let _z = self.eval_expr(&args[3])?.as_f64()?;
                let elems = self.eval_expr(&args[4])?.as_i64()?;
                if elems == 0 {
                    return Ok(Value::Void);
                }

                let initial_color = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("color")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(15) as u8,
                    _ => 15u8,
                } & 0x0f;
                let initial_thick = match &dc {
                    Value::Obj(dc) => dc
                        .borrow()
                        .fields
                        .get("thick")
                        .and_then(|v| v.as_i64().ok())
                        .unwrap_or(1) as i32,
                    _ => 1,
                }
                .max(1);

                let bytes_vec = if let Some(&len) = self.doldoc_bin_len_by_ptr.get(&elems) {
                    self.heap_slice(elems, len)?.to_vec()
                } else {
                    self.heap_tail(elems)?.to_vec()
                };

                temple_rt::sprite::sprite_render_with_state(
                    &mut self.rt,
                    x,
                    y,
                    &bytes_vec,
                    initial_color,
                    initial_thick,
                );
                Ok(Value::Void)
            }
            _ => Err(format!("internal: call_builtin_gfx cannot handle {name}")),
        }
    }
}
