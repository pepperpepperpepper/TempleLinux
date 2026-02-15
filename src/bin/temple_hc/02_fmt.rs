#[derive(Clone, Copy, Debug)]
enum AuxFmt {
    Question,
    Number(i32),
    FromArg,
}

fn group_dec_digits(digits: &str) -> String {
    let mut out_rev = String::new();
    let mut n = 0usize;
    for ch in digits.chars().rev() {
        if n == 3 {
            out_rev.push(',');
            n = 0;
        }
        out_rev.push(ch);
        n += 1;
    }
    out_rev.chars().rev().collect()
}

fn apply_width(mut s: String, width: Option<usize>, left_align: bool, zero_pad: bool) -> String {
    let Some(width) = width else {
        return s;
    };

    if s.len() >= width {
        return s;
    }

    let pad_len = width - s.len();
    let pad_ch = if zero_pad && !left_align { '0' } else { ' ' };

    if left_align {
        s.extend(std::iter::repeat(pad_ch).take(pad_len));
        return s;
    }

    if pad_ch == '0' && s.starts_with('-') {
        let mut out = String::with_capacity(width);
        out.push('-');
        out.extend(std::iter::repeat('0').take(pad_len));
        out.push_str(&s[1..]);
        return out;
    }

    let mut out = String::with_capacity(width);
    out.extend(std::iter::repeat(pad_ch).take(pad_len));
    out.push_str(&s);
    out
}

fn si_suffix(exp: i32) -> Option<&'static str> {
    match exp {
        -12 => Some("p"),
        -9 => Some("n"),
        -6 => Some("u"),
        -3 => Some("m"),
        0 => Some(""),
        3 => Some("k"),
        6 => Some("M"),
        9 => Some("G"),
        12 => Some("T"),
        _ => None,
    }
}

fn format_engineering(value: f64, decimals: Option<usize>, aux: Option<AuxFmt>) -> String {
    if value == 0.0 {
        let d = decimals.unwrap_or(6);
        return format!("{:.*}", d, 0.0);
    }

    let mut exp: i32 = match aux {
        Some(AuxFmt::Number(n)) => n,
        Some(AuxFmt::Question) | None | Some(AuxFmt::FromArg) => {
            let abs = value.abs();
            ((abs.log10() / 3.0).floor() as i32) * 3
        }
    };

    let mut scaled = value / 10f64.powi(exp);
    // Normalize to [1, 1000) where possible for auto mode.
    if matches!(aux, Some(AuxFmt::Question) | None | Some(AuxFmt::FromArg)) {
        while scaled.abs() >= 1000.0 {
            exp += 3;
            scaled /= 1000.0;
        }
        while scaled.abs() < 1.0 {
            exp -= 3;
            scaled *= 1000.0;
            if scaled == 0.0 {
                break;
            }
        }
    }

    let d = decimals.unwrap_or(6);
    let mut num = format!("{:.*}", d, scaled);
    if decimals.is_none() {
        while num.contains('.') && num.ends_with('0') {
            num.pop();
        }
        if num.ends_with('.') {
            num.pop();
        }
    }

    match aux {
        Some(AuxFmt::Question) | Some(AuxFmt::Number(_)) => {
            if let Some(suffix) = si_suffix(exp) {
                format!("{num}{suffix}")
            } else {
                format!("{num}e{exp}")
            }
        }
        _ => format!("{num}e{exp}"),
    }
}

fn next_fmt_arg<'a>(
    args: &'a [super::vm::Value],
    idx: &mut usize,
) -> Result<&'a super::vm::Value, String> {
    let Some(v) = args.get(*idx) else {
        return Err("not enough arguments for format string".to_string());
    };
    *idx += 1;
    Ok(v)
}

fn fmt_char(v: &super::vm::Value) -> Result<char, String> {
    match v {
        super::vm::Value::Str(s) => s
            .chars()
            .next()
            .ok_or_else(|| "empty string for %c".to_string()),
        _ => Ok((v.as_i64()? as u8) as char),
    }
}

pub(super) const CDATE_FREQ_HZ: i64 = 49_710;

fn cdate_split(cdt: i64) -> (i32, u32) {
    let time = (cdt as u64 & 0xffff_ffff) as u32;
    let date = (cdt >> 32) as i32;
    (date, time)
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    // Howard Hinnant's algorithm: https://howardhinnant.github.io/date_algorithms.html
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = mp + if mp < 10 { 3 } else { -9 }; // [1, 12]
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m as u32, d as u32)
}

fn fmt_cdate_date(cdt: i64) -> String {
    let (date_days, _time) = cdate_split(cdt);
    let (year, month, day) = civil_from_days(date_days as i64);
    format!("{:02}/{:02}/{:02}", month, day, (year % 100).abs())
}

fn fmt_cdate_time(cdt: i64) -> String {
    let (_date_days, time_ticks) = cdate_split(cdt);
    let secs = (time_ticks as i64 / CDATE_FREQ_HZ).clamp(0, 86_399);
    let hour = secs / 3600;
    let min = (secs / 60) % 60;
    let sec = secs % 60;
    format!("{:02}:{:02}:{:02}", hour, min, sec)
}

pub(super) fn format_temple_fmt_with_cstr<F, Z>(
    fmt: &str,
    args: &[super::vm::Value],
    mut read_cstr: F,
    mut define_sub: Z,
) -> Result<String, String>
where
    F: FnMut(i64) -> Result<String, String>,
    Z: FnMut(i64, &str) -> Option<String>,
{
    let mut out = String::new();
    let mut it = fmt.chars().peekable();
    let mut arg_idx: usize = 0;

    while let Some(ch) = it.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }

        if it.peek() == Some(&'%') {
            it.next();
            out.push('%');
            continue;
        }

        let mut left_align = false;
        let mut zero_pad = false;
        if it.peek() == Some(&'-') {
            left_align = true;
            it.next();
        }
        if it.peek() == Some(&'0') {
            zero_pad = true;
            it.next();
        }

        let mut width: Option<usize> = None;
        let mut width_str = String::new();
        while matches!(it.peek(), Some('0'..='9')) {
            width_str.push(it.next().unwrap());
        }
        if !width_str.is_empty() {
            width = Some(
                width_str
                    .parse::<usize>()
                    .map_err(|_| format!("bad width: {width_str}"))?,
            );
        }

        let mut decimals: Option<usize> = None;
        if it.peek() == Some(&'.') {
            it.next();
            let mut dec_str = String::new();
            while matches!(it.peek(), Some('0'..='9')) {
                dec_str.push(it.next().unwrap());
            }
            if dec_str.is_empty() {
                return Err("expected decimals after '.'".to_string());
            }
            decimals = Some(
                dec_str
                    .parse::<usize>()
                    .map_err(|_| format!("bad decimals: {dec_str}"))?,
            );
        }

        let mut flag_commas = false;
        loop {
            match it.peek().copied() {
                Some(',') => {
                    flag_commas = true;
                    it.next();
                }
                Some('t') | Some('$') | Some('/') => {
                    // Not needed yet; accepted for compatibility.
                    it.next();
                }
                _ => break,
            }
        }

        let mut aux: Option<AuxFmt> = None;
        if it.peek() == Some(&'h') {
            it.next();
            match it.peek().copied() {
                Some('*') => {
                    it.next();
                    aux = Some(AuxFmt::FromArg);
                }
                Some('?') => {
                    it.next();
                    aux = Some(AuxFmt::Question);
                }
                Some('-') | Some('0'..='9') => {
                    let mut sign: i32 = 1;
                    if it.peek() == Some(&'-') {
                        sign = -1;
                        it.next();
                    }
                    let mut num_str = String::new();
                    while matches!(it.peek(), Some('0'..='9')) {
                        num_str.push(it.next().unwrap());
                    }
                    if num_str.is_empty() {
                        return Err("expected aux fmt number after 'h'".to_string());
                    }
                    let n: i32 = num_str
                        .parse::<i32>()
                        .map_err(|_| format!("bad aux fmt: {num_str}"))?
                        .saturating_mul(sign);
                    aux = Some(AuxFmt::Number(n));
                }
                _ => return Err("expected aux fmt after 'h'".to_string()),
            }
        }

        let code = it
            .next()
            .ok_or_else(|| "unexpected end of format string".to_string())?;

        let seg = match code {
            'd' | 'u' => {
                let v = next_fmt_arg(args, &mut arg_idx)?;
                let mut s = if code == 'u' {
                    format!("{}", v.as_i64()? as u64)
                } else {
                    format!("{}", v.as_i64()?)
                };
                if flag_commas || matches!(aux, Some(AuxFmt::Question) | Some(AuxFmt::Number(_))) {
                    let (sign, digits) = s.strip_prefix('-').map_or(("", s.as_str()), |d| ("-", d));
                    s = format!("{sign}{}", group_dec_digits(digits));
                }
                apply_width(s, width, left_align, zero_pad)
            }
            'x' | 'X' => {
                let v = next_fmt_arg(args, &mut arg_idx)?;
                let u = v.as_i64()? as u64;
                let s = if code == 'x' {
                    format!("{:x}", u)
                } else {
                    format!("{:X}", u)
                };
                apply_width(s, width, left_align, zero_pad)
            }
            'n' => {
                let v = next_fmt_arg(args, &mut arg_idx)?;
                let s = format_engineering(v.as_f64()?, decimals, aux);
                apply_width(s, width, left_align, zero_pad)
            }
            'f' => {
                let v = next_fmt_arg(args, &mut arg_idx)?;
                let d = decimals.unwrap_or(6);
                let s = format!("{:.*}", d, v.as_f64()?);
                apply_width(s, width, left_align, zero_pad)
            }
            'c' | 'C' => {
                let repeat = match aux {
                    Some(AuxFmt::FromArg) => next_fmt_arg(args, &mut arg_idx)?.as_i64()? as usize,
                    Some(AuxFmt::Number(n)) => (n as i64).max(0) as usize,
                    _ => 1usize,
                };
                let mut c = fmt_char(next_fmt_arg(args, &mut arg_idx)?)?;
                if code == 'C' {
                    c = c.to_ascii_uppercase();
                }
                let s: String = std::iter::repeat(c).take(repeat).collect();
                apply_width(s, width, left_align, zero_pad)
            }
            's' => {
                let v = next_fmt_arg(args, &mut arg_idx)?;
                let s = match v {
                    super::vm::Value::Str(s) => s.clone(),
                    _ => read_cstr(v.as_i64()?)?,
                };
                apply_width(s, width, left_align, zero_pad)
            }
            'z' => {
                let idx = next_fmt_arg(args, &mut arg_idx)?.as_i64()?;
                let lst = next_fmt_arg(args, &mut arg_idx)?;
                let list = match lst {
                    super::vm::Value::Str(s) => s.clone(),
                    _ => read_cstr(lst.as_i64()?)?,
                };

                let mut picked: Option<String> = None;
                if idx >= 0 {
                    for (i, item) in list.split('\0').enumerate() {
                        if i == idx as usize {
                            picked = Some(item.to_string());
                            break;
                        }
                    }
                }

                let s = picked.unwrap_or_else(|| idx.to_string());
                apply_width(s, width, left_align, false)
            }
            'Z' => {
                let idx = next_fmt_arg(args, &mut arg_idx)?.as_i64()?;
                let lst = next_fmt_arg(args, &mut arg_idx)?;
                let name = match lst {
                    super::vm::Value::Str(s) => s.clone(),
                    _ => read_cstr(lst.as_i64()?)?,
                };
                let s = define_sub(idx, &name).unwrap_or_else(|| idx.to_string());
                apply_width(s, width, left_align, false)
            }
            'D' => {
                let v = next_fmt_arg(args, &mut arg_idx)?;
                let s = fmt_cdate_date(v.as_i64()?);
                apply_width(s, width, left_align, zero_pad)
            }
            'T' => {
                let v = next_fmt_arg(args, &mut arg_idx)?;
                let s = fmt_cdate_time(v.as_i64()?);
                apply_width(s, width, left_align, zero_pad)
            }
            _ => return Err(format!("unsupported format code: %{code}")),
        };

        out.push_str(&seg);
    }

    Ok(out)
}

pub(super) fn format_temple_fmt(fmt: &str, args: &[super::vm::Value]) -> Result<String, String> {
    format_temple_fmt_with_cstr(
        fmt,
        args,
        |_ptr| Err("%s requires a VM heap (use format_temple_fmt_with_cstr)".to_string()),
        |_idx, _name| None,
    )
}
