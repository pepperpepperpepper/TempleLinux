use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use temple_rt::{
    protocol,
    rt::{Event, TempleRt},
};

const FONT_W: i32 = 8;
const FONT_H: i32 = 8;
const UI_BG: u8 = 0;
const UI_FG: u8 = 15;
const BAR_BG: u8 = 4;
const SEL_BG: u8 = 1;
const SEL_FG: u8 = 15;

const LINE_NO_W: usize = 6; // "##### "

const KEY_S_LOWER: u32 = b's' as u32;
const KEY_S_UPPER: u32 = b'S' as u32;
const KEY_Q_LOWER: u32 = b'q' as u32;
const KEY_Q_UPPER: u32 = b'Q' as u32;
const KEY_C_LOWER: u32 = b'c' as u32;
const KEY_C_UPPER: u32 = b'C' as u32;
const KEY_X_LOWER: u32 = b'x' as u32;
const KEY_X_UPPER: u32 = b'X' as u32;
const KEY_A_LOWER: u32 = b'a' as u32;
const KEY_A_UPPER: u32 = b'A' as u32;
const KEY_F_LOWER: u32 = b'f' as u32;
const KEY_F_UPPER: u32 = b'F' as u32;

fn clamp_usize(v: usize, min_v: usize, max_v: usize) -> usize {
    v.max(min_v).min(max_v)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pos {
    line: usize,
    col: usize,
}

impl Pos {
    fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

fn normalize_sel(a: Pos, b: Pos) -> (Pos, Pos) {
    if (a.line, a.col) <= (b.line, b.col) {
        (a, b)
    } else {
        (b, a)
    }
}

fn selection_is_empty(sel: (Pos, Pos)) -> bool {
    let (a, b) = sel;
    a == b
}

fn is_selected(sel: (Pos, Pos), line: usize, col: usize) -> bool {
    let (start, end) = normalize_sel(sel.0, sel.1);
    if (start.line, start.col) == (end.line, end.col) {
        return false;
    }

    if line < start.line || line > end.line {
        return false;
    }
    if start.line == end.line {
        return col >= start.col && col < end.col;
    }
    if line == start.line {
        return col >= start.col;
    }
    if line == end.line {
        return col < end.col;
    }
    true
}

fn delete_selection(
    lines: &mut Vec<Vec<u8>>,
    cursor_line: &mut usize,
    cursor_col: &mut usize,
    selection: &mut Option<(Pos, Pos)>,
) -> bool {
    let Some(sel) = *selection else {
        return false;
    };
    let (start, end) = normalize_sel(sel.0, sel.1);
    if start == end {
        *selection = None;
        return false;
    }
    if start.line >= lines.len() || end.line >= lines.len() {
        *selection = None;
        return false;
    }

    if start.line == end.line {
        let line = &mut lines[start.line];
        let s = start.col.min(line.len());
        let e = end.col.min(line.len());
        if s < e {
            line.drain(s..e);
        }
    } else {
        let first = lines[start.line].clone();
        let last = lines[end.line].clone();
        let prefix = first[..start.col.min(first.len())].to_vec();
        let suffix = last[end.col.min(last.len())..].to_vec();

        lines[start.line] = prefix;
        lines[start.line].extend_from_slice(&suffix);

        let remove_from = start.line + 1;
        let remove_to = end.line + 1;
        if remove_from < remove_to && remove_from < lines.len() {
            let end_idx = remove_to.min(lines.len());
            lines.drain(remove_from..end_idx);
        }
    }

    if lines.is_empty() {
        lines.push(Vec::new());
    }

    *cursor_line = start.line.min(lines.len() - 1);
    *cursor_col = start.col.min(lines[*cursor_line].len());
    *selection = None;
    true
}

fn selected_text(lines: &[Vec<u8>], sel: (Pos, Pos)) -> String {
    let (start, end) = normalize_sel(sel.0, sel.1);
    if start == end || start.line >= lines.len() || end.line >= lines.len() {
        return String::new();
    }

    if start.line == end.line {
        let line = &lines[start.line];
        let s = start.col.min(line.len());
        let e = end.col.min(line.len());
        return String::from_utf8_lossy(&line[s..e]).to_string();
    }

    let mut out: Vec<u8> = Vec::new();
    let first = &lines[start.line];
    out.extend_from_slice(&first[start.col.min(first.len())..]);
    out.push(b'\n');

    for mid in lines.iter().take(end.line).skip(start.line + 1) {
        out.extend_from_slice(mid);
        out.push(b'\n');
    }

    let last = &lines[end.line];
    out.extend_from_slice(&last[..end.col.min(last.len())]);

    String::from_utf8_lossy(&out).to_string()
}

fn topic_from_cursor_or_selection(
    lines: &[Vec<u8>],
    cursor_line: usize,
    cursor_col: usize,
    selection: Option<(Pos, Pos)>,
) -> Option<String> {
    if let Some(sel) = selection {
        if !selection_is_empty(sel) {
            let mut text = selected_text(lines, sel);
            if let Some((first, _)) = text.split_once('\n') {
                text = first.to_string();
            }
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            return Some(strip_tdoc_link_wrappers(text).to_string());
        }
    }

    let line = lines.get(cursor_line)?;
    if line.is_empty() {
        return None;
    }

    let mut idx = cursor_col.min(line.len());
    if idx == line.len() && idx > 0 {
        idx -= 1;
    }

    if idx >= line.len() {
        return None;
    }

    if !is_word_byte(line[idx]) {
        if idx > 0 && is_word_byte(line[idx - 1]) {
            idx -= 1;
        } else {
            return None;
        }
    }

    let mut start = idx;
    while start > 0 && is_word_byte(line[start - 1]) {
        start -= 1;
    }
    let mut end = idx + 1;
    while end < line.len() && is_word_byte(line[end]) {
        end += 1;
    }

    let word = String::from_utf8_lossy(&line[start..end]).to_string();
    let word = word.trim();
    if word.is_empty() {
        return None;
    }
    Some(strip_tdoc_link_wrappers(word).to_string())
}

fn strip_tdoc_link_wrappers(s: &str) -> &str {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix("[[").and_then(|x| x.strip_suffix("]]")) {
        inner.trim()
    } else {
        s
    }
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn read_file_lines(path: &Path) -> io::Result<Vec<Vec<u8>>> {
    let buf = std::fs::read(path)?;
    let mut lines: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        let b = buf[i];
        i += 1;
        match b {
            b'\r' => {
                // drop
            }
            b'\n' => {
                lines.push(cur);
                cur = Vec::new();
            }
            b'\t' => {
                cur.extend_from_slice(b"    ");
            }
            b => {
                if b.is_ascii() {
                    cur.push(b);
                }
            }
        }
    }
    lines.push(cur);
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    Ok(lines)
}

fn write_file_lines(path: &Path, lines: &[Vec<u8>]) -> io::Result<()> {
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        out.extend_from_slice(line);
        if idx + 1 < lines.len() {
            out.push(b'\n');
        }
    }
    std::fs::write(path, out)
}

fn draw_text_cells(rt: &mut TempleRt, col: i32, row: i32, fg: u8, bg: u8, text: &str) {
    rt.draw_text(col * FONT_W, row * FONT_H, fg, bg, text);
}

fn draw_cell(rt: &mut TempleRt, col: i32, row: i32, fg: u8, bg: u8, ch: u8) {
    rt.draw_char_8x8(col * FONT_W, row * FONT_H, fg, bg, ch as char);
}

fn ensure_cursor_visible(cursor_line: usize, top_line: &mut usize, view_rows: usize) {
    if view_rows == 0 {
        return;
    }
    if cursor_line < *top_line {
        *top_line = cursor_line;
        return;
    }
    if cursor_line >= *top_line + view_rows {
        *top_line = cursor_line.saturating_sub(view_rows - 1);
    }
}

fn default_temple_root() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".templelinux")
    } else {
        PathBuf::from(".templelinux")
    }
}

fn temple_root_dir() -> PathBuf {
    if let Some(root) = std::env::var_os("TEMPLE_ROOT") {
        PathBuf::from(root)
    } else {
        default_temple_root()
    }
}

fn temple_hc_program() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            let candidate = exe.with_file_name("temple-hc");
            candidate.exists().then_some(candidate)
        })
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "temple-hc".to_string())
}

fn is_read_only_templeos_path(path: &Path) -> bool {
    let Some(root) = std::env::var_os("TEMPLEOS_ROOT") else {
        return false;
    };
    let root = PathBuf::from(root);
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    abs.starts_with(&root)
}

#[derive(Debug, Clone)]
struct HelpOverlay {
    title: String,
    lines: Vec<String>,
    scroll: usize,
}

fn load_help_overlay(topic: &str) -> io::Result<Option<HelpOverlay>> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;

    let topic = topic.trim();
    if topic.is_empty() {
        return Ok(None);
    }

    let root = temple_root_dir();
    let rel = topic.trim_start_matches('/');
    let rel = if rel.starts_with("Doc/") {
        rel.to_string()
    } else {
        format!("Doc/{rel}")
    };

    let candidates = [
        root.join(format!("{rel}.TD")),
        root.join(format!("{rel}.td")),
        root.join(format!("{rel}.txt")),
        root.join(&rel),
    ];

    for path in candidates {
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

        use std::io::Read as _;
        let mut buf = Vec::new();
        if file.take(MAX_BYTES).read_to_end(&mut buf).is_err() {
            continue;
        }
        let text = String::from_utf8_lossy(&buf);
        let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
        if meta_len > MAX_BYTES {
            lines.push(format!("[truncated: {} bytes total]", meta_len));
        }

        return Ok(Some(HelpOverlay {
            title: path.to_string_lossy().to_string(),
            lines,
            scroll: 0,
        }));
    }

    Ok(None)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn find_next(lines: &[Vec<u8>], query: &[u8], start: Pos) -> Option<(Pos, Pos)> {
    if query.is_empty() || lines.is_empty() {
        return None;
    }

    let start_line = start.line.min(lines.len() - 1);
    let start_col = start.col;

    for line_idx in start_line..lines.len() {
        let line = &lines[line_idx];
        let from = if line_idx == start_line {
            start_col.min(line.len())
        } else {
            0
        };
        if let Some(off) = find_subslice(&line[from..], query) {
            let col = from + off;
            return Some((
                Pos::new(line_idx, col),
                Pos::new(line_idx, col.saturating_add(query.len())),
            ));
        }
    }

    // Wrap.
    for line_idx in 0..=start_line {
        let line = &lines[line_idx];
        let to = if line_idx == start_line {
            start_col.min(line.len())
        } else {
            line.len()
        };
        if let Some(off) = find_subslice(&line[..to], query) {
            let col = off;
            return Some((
                Pos::new(line_idx, col),
                Pos::new(line_idx, col.saturating_add(query.len())),
            ));
        }
    }

    None
}

fn draw_tdoc_line(rt: &mut TempleRt, row: i32, bg: u8, line: &str, max_cols: usize) {
    let mut col: usize = 0;

    if let Some(heading) = line.strip_prefix('#') {
        let mut level = 1usize;
        for c in heading.chars() {
            if c == '#' {
                level += 1;
            } else {
                break;
            }
        }
        let text = line.trim_start_matches('#').trim();
        let heading_fg = match level {
            1 => 14,
            2 => 11,
            _ => 10,
        };
        for ch in text.chars() {
            if col >= max_cols {
                break;
            }
            if !ch.is_ascii() {
                continue;
            }
            draw_cell(rt, col as i32, row, heading_fg, bg, ch as u8);
            col += 1;
        }
        return;
    }

    let mut fg = UI_FG;
    let mut in_code = false;
    let mut rest = line;

    while !rest.is_empty() && col < max_cols {
        let next_tick = rest.find('`');
        let next_link = if in_code { None } else { rest.find("[[") };

        let next = match (next_tick, next_link) {
            (None, None) => (rest.len(), "text"),
            (Some(i), None) => (i, "tick"),
            (None, Some(i)) => (i, "link"),
            (Some(i), Some(j)) => {
                if i <= j {
                    (i, "tick")
                } else {
                    (j, "link")
                }
            }
        };

        let (idx, kind) = next;
        let (before, after) = rest.split_at(idx);
        for ch in before.chars() {
            if col >= max_cols {
                break;
            }
            if !ch.is_ascii() {
                continue;
            }
            draw_cell(rt, col as i32, row, fg, bg, ch as u8);
            col += 1;
        }
        if col >= max_cols {
            break;
        }

        match kind {
            "text" => break,
            "tick" => {
                rest = &after[1..];
                in_code = !in_code;
                fg = if in_code { 11 } else { UI_FG };
            }
            "link" => {
                let Some(close) = after.find("]]") else {
                    rest = after;
                    continue;
                };
                let inner = &after[2..close];
                let saved = fg;
                fg = 10;
                for ch in inner.chars() {
                    if col >= max_cols {
                        break;
                    }
                    if !ch.is_ascii() {
                        continue;
                    }
                    draw_cell(rt, col as i32, row, fg, bg, ch as u8);
                    col += 1;
                }
                fg = saved;
                rest = &after[close + 2..];
            }
            _ => break,
        }
    }
}

#[derive(Clone, Debug)]
struct HcDiag {
    file: String,
    line: usize,
    col: usize,
    msg: String,
}

fn parse_hc_diag_line(line: &str) -> Option<HcDiag> {
    // Parse: file:line:col: message
    let mut it = line.rsplitn(4, ':');
    let msg = it.next()?.trim().to_string();
    let col = it.next()?.trim().parse::<usize>().ok()?;
    let line_no = it.next()?.trim().parse::<usize>().ok()?;
    let file = it.next()?.trim().to_string();
    if file.is_empty() || line_no == 0 || col == 0 {
        return None;
    }
    Some(HcDiag {
        file,
        line: line_no,
        col,
        msg,
    })
}

fn parse_hc_diag_text(text: &str) -> Option<HcDiag> {
    for line in text.lines() {
        if let Some(diag) = parse_hc_diag_line(line) {
            return Some(diag);
        }
    }
    None
}

fn main() -> io::Result<()> {
    let mut rt = TempleRt::connect()?;
    let (w_u32, h_u32) = rt.size();
    let (w, h) = (w_u32 as i32, h_u32 as i32);
    let cols = (w / FONT_W).max(1) as usize;
    let rows = (h / FONT_H).max(1) as usize;

    let mut args = std::env::args().skip(1);
    let path = args.next().map(PathBuf::from);
    let path = path.unwrap_or_else(|| PathBuf::from("Untitled.txt"));
    let read_only = is_read_only_templeos_path(&path);

    let mut lines = match read_file_lines(&path) {
        Ok(v) => v,
        Err(err) if err.kind() == io::ErrorKind::NotFound => vec![Vec::new()],
        Err(err) => return Err(err),
    };

    let mut cursor_line: usize = 0;
    let mut cursor_col: usize = 0;
    let mut top_line: usize = 0;
    let mut modified = false;
    let mut status_msg: String = String::new();

    let mut ctrl = false;
    let mut shift = false;
    let mut selection: Option<(Pos, Pos)> = None;
    let mut selection_anchor: Option<Pos> = None;

    let mut search_active = false;
    let mut search_query: String = String::new();
    let mut search_feedback: String = String::new();
    let mut last_search: String = String::new();
    let mut last_match_end: Option<Pos> = None;

    let mut help: Option<HelpOverlay> = None;

    enum RunMsg {
        Launched {
            pid: u32,
        },
        CompileError {
            diag: Option<HcDiag>,
            stderr: String,
        },
        BuildError {
            msg: String,
        },
    }
    let (run_tx, run_rx) = mpsc::channel::<RunMsg>();
    let mut build_in_flight = false;

    loop {
        while let Ok(msg) = run_rx.try_recv() {
            build_in_flight = false;
            match msg {
                RunMsg::Launched { pid } => {
                    status_msg = format!("[run launched pid {pid}]");
                }
                RunMsg::BuildError { msg } => {
                    status_msg = msg;
                }
                RunMsg::CompileError { diag, stderr } => {
                    if let Some(diag) = diag {
                        let target_line = diag.line.saturating_sub(1);
                        let target_col = diag.col.saturating_sub(1);

                        if Path::new(&diag.file) == path.as_path() {
                            cursor_line = target_line.min(lines.len().saturating_sub(1));
                            cursor_col = target_col.min(lines[cursor_line].len());
                            selection_anchor = None;
                            selection = Some((
                                Pos::new(cursor_line, cursor_col),
                                Pos::new(cursor_line, cursor_col.saturating_add(1)),
                            ));
                            status_msg = format!(
                                "[error] {}:{}:{}: {}",
                                diag.file, diag.line, diag.col, diag.msg
                            );
                        } else {
                            status_msg = format!(
                                "[error] {}:{}:{}: {}",
                                diag.file, diag.line, diag.col, diag.msg
                            );
                        }
                    } else {
                        let first = stderr.lines().next().unwrap_or("compile error");
                        status_msg = format!("[compile error] {first}");
                    }
                }
            }
        }

        let view_rows = rows.saturating_sub(2);
        ensure_cursor_visible(cursor_line, &mut top_line, view_rows);

        rt.clear(UI_BG);

        // Top bar
        rt.fill_rect(0, 0, w, FONT_H, BAR_BG);
        let title = if let Some(overlay) = help.as_ref() {
            format!(
                "HELP: {}  PgUp/PgDn scroll  Esc close",
                overlay.title.as_str()
            )
        } else {
            format!(
                "{}{}{}  Ctrl+S save  Ctrl+Q quit  Ctrl+F find  F3 next  F5 run  F1 help",
                path.display(),
                if read_only { " [RO]" } else { "" },
                if modified { " *" } else { "" }
            )
        };
        draw_text_cells(&mut rt, 0, 0, UI_FG, BAR_BG, &title);

        // Bottom status bar
        rt.fill_rect(0, (rows as i32 - 1) * FONT_H, w, FONT_H, BAR_BG);
        let line_no = cursor_line + 1;
        let col_no = cursor_col + 1;
        let status = if help.is_some() {
            format!("Help  PgUp/PgDn scroll  Esc close")
        } else if build_in_flight {
            "[building...]".to_string()
        } else if search_active {
            let mut s = format!("Find: {search_query}  Enter find  Esc cancel  F3 next");
            if !search_feedback.is_empty() {
                s.push_str("  ");
                s.push_str(&search_feedback);
            }
            s
        } else if status_msg.is_empty() {
            format!(
                "Ln {line_no}  Col {col_no}  PgUp/PgDn scroll  F5 run  Tab=spaces  Ctrl+C copy  Ctrl+X cut  Ctrl+A all"
            )
        } else {
            status_msg.clone()
        };
        draw_text_cells(&mut rt, 0, rows as i32 - 1, UI_FG, BAR_BG, &status);

        // Text area (or help overlay)
        if let Some(overlay) = help.as_ref() {
            for row_idx in 0..view_rows {
                let screen_row = 1 + row_idx;
                let doc_idx = overlay.scroll + row_idx;
                if doc_idx >= overlay.lines.len() {
                    continue;
                }
                draw_tdoc_line(
                    &mut rt,
                    screen_row as i32,
                    UI_BG,
                    &overlay.lines[doc_idx],
                    cols,
                );
            }
        } else {
            for row_idx in 0..view_rows {
                let line_idx = top_line + row_idx;
                let screen_row = 1 + row_idx;
                if line_idx >= lines.len() {
                    continue;
                }

                let line_no_text = format!("{:>5} ", line_idx + 1);
                let ln_col = if screen_row % 2 == 0 { 7 } else { 8 };
                draw_text_cells(&mut rt, 0, screen_row as i32, ln_col, UI_BG, &line_no_text);

                let line = &lines[line_idx];
                let avail_cols = cols.saturating_sub(LINE_NO_W);

                for (col_idx, &b) in line.iter().take(avail_cols).enumerate() {
                    let pos = (line_idx, col_idx);
                    let is_cursor = pos == (cursor_line, cursor_col) && cursor_col < avail_cols;
                    let is_sel = selection.is_some_and(|sel| is_selected(sel, line_idx, col_idx));
                    if is_cursor {
                        draw_cell(
                            &mut rt,
                            (LINE_NO_W + col_idx) as i32,
                            screen_row as i32,
                            UI_BG,
                            UI_FG,
                            b,
                        );
                    } else if is_sel {
                        draw_cell(
                            &mut rt,
                            (LINE_NO_W + col_idx) as i32,
                            screen_row as i32,
                            SEL_FG,
                            SEL_BG,
                            b,
                        );
                    } else {
                        draw_cell(
                            &mut rt,
                            (LINE_NO_W + col_idx) as i32,
                            screen_row as i32,
                            UI_FG,
                            UI_BG,
                            b,
                        );
                    }
                }

                // Cursor at end-of-line (or beyond visible text)
                if line_idx == cursor_line && cursor_col >= line.len().min(avail_cols) {
                    let cx = (LINE_NO_W + cursor_col.min(avail_cols)) as i32;
                    if cx < cols as i32 {
                        draw_cell(&mut rt, cx, screen_row as i32, UI_BG, UI_FG, b' ');
                    }
                }
            }
        }

        rt.present()?;

        let mut did_event = false;
        while let Some(ev) = rt.try_next_event() {
            did_event = true;
            match ev {
                Event::Key { code, down } => {
                    if code == protocol::KEY_CONTROL {
                        ctrl = down;
                        continue;
                    }
                    if code == protocol::KEY_SHIFT {
                        shift = down;
                        if down {
                            selection_anchor = Some(Pos::new(cursor_line, cursor_col));
                        } else {
                            selection_anchor = None;
                        }
                        continue;
                    }
                    if !down {
                        continue;
                    }

                    if !search_active {
                        status_msg.clear();
                    }

                    if ctrl {
                        match code {
                            KEY_S_LOWER | KEY_S_UPPER => {
                                if read_only {
                                    status_msg = "[read-only]".to_string();
                                } else {
                                    match write_file_lines(&path, &lines) {
                                        Ok(()) => {
                                            modified = false;
                                            status_msg = "[saved]".to_string();
                                        }
                                        Err(err) => {
                                            status_msg = format!("[save error: {err}]");
                                        }
                                    }
                                }
                            }
                            KEY_Q_LOWER | KEY_Q_UPPER => return Ok(()),
                            KEY_C_LOWER | KEY_C_UPPER => {
                                let text = if let Some(sel) = selection {
                                    if selection_is_empty(sel) {
                                        String::new()
                                    } else {
                                        selected_text(&lines, sel)
                                    }
                                } else {
                                    String::new()
                                };

                                if text.is_empty() {
                                    status_msg = "[copy: no selection]".to_string();
                                } else {
                                    match rt.clipboard_set_text(&text) {
                                        Ok(()) => status_msg = "[copied]".to_string(),
                                        Err(err) => status_msg = format!("[copy error: {err}]"),
                                    }
                                }
                            }
                            KEY_X_LOWER | KEY_X_UPPER => {
                                if read_only {
                                    status_msg = "[read-only]".to_string();
                                    continue;
                                }
                                if let Some(sel) = selection {
                                    if selection_is_empty(sel) {
                                        status_msg = "[cut: no selection]".to_string();
                                    } else {
                                        let text = selected_text(&lines, sel);
                                        let clip_res = rt.clipboard_set_text(&text);
                                        if delete_selection(
                                            &mut lines,
                                            &mut cursor_line,
                                            &mut cursor_col,
                                            &mut selection,
                                        ) {
                                            modified = true;
                                        }
                                        match clip_res {
                                            Ok(()) => status_msg = "[cut]".to_string(),
                                            Err(err) => status_msg = format!("[cut error: {err}]"),
                                        }
                                    }
                                } else {
                                    status_msg = "[cut: no selection]".to_string();
                                }
                            }
                            KEY_A_LOWER | KEY_A_UPPER => {
                                let end_line = lines.len().saturating_sub(1);
                                let end_col = lines.get(end_line).map(|l| l.len()).unwrap_or(0);
                                selection = Some((Pos::new(0, 0), Pos::new(end_line, end_col)));
                                selection_anchor = None;
                            }
                            KEY_F_LOWER | KEY_F_UPPER => {
                                search_active = true;
                                search_query.clear();
                                search_feedback.clear();
                            }
                            protocol::KEY_HOME => {
                                top_line = 0;
                                cursor_line = 0;
                                cursor_col = 0;
                                selection_anchor = None;
                                selection = None;
                            }
                            protocol::KEY_END => {
                                cursor_line = lines.len().saturating_sub(1);
                                cursor_col = lines[cursor_line].len();
                                top_line = lines.len().saturating_sub(view_rows);
                                selection_anchor = None;
                                selection = None;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if help.is_some() {
                        if code == protocol::KEY_ESCAPE || code == protocol::KEY_F1 {
                            help = None;
                        } else if let Some(overlay) = help.as_mut() {
                            let view = view_rows.max(1);
                            let max_scroll = overlay.lines.len().saturating_sub(view);
                            match code {
                                protocol::KEY_PAGE_UP => {
                                    overlay.scroll = overlay.scroll.saturating_sub(view);
                                }
                                protocol::KEY_PAGE_DOWN => {
                                    overlay.scroll = (overlay.scroll + view).min(max_scroll);
                                }
                                protocol::KEY_UP => {
                                    overlay.scroll = overlay.scroll.saturating_sub(1);
                                }
                                protocol::KEY_DOWN => {
                                    overlay.scroll = (overlay.scroll + 1).min(max_scroll);
                                }
                                protocol::KEY_HOME => overlay.scroll = 0,
                                protocol::KEY_END => overlay.scroll = max_scroll,
                                _ => {}
                            }
                        }
                        continue;
                    }

                    if search_active {
                        match code {
                            protocol::KEY_ESCAPE => {
                                search_active = false;
                                search_feedback.clear();
                            }
                            protocol::KEY_ENTER => {
                                let query = search_query.trim().to_string();
                                let q = query.as_bytes();
                                if q.is_empty() {
                                    search_feedback = "[empty]".to_string();
                                    continue;
                                }

                                let start = Pos::new(cursor_line, cursor_col);
                                if let Some((m0, m1)) = find_next(&lines, q, start) {
                                    cursor_line = m0.line;
                                    cursor_col = m0.col;
                                    selection = Some((m0, m1));
                                    selection_anchor = None;
                                    last_search = query;
                                    last_match_end = Some(m1);
                                    search_active = false;
                                    search_feedback.clear();
                                } else {
                                    search_feedback = "[not found]".to_string();
                                }
                            }
                            protocol::KEY_BACKSPACE => {
                                search_query.pop();
                            }
                            protocol::KEY_F3 => {
                                if last_search.is_empty() {
                                    search_feedback = "[no previous search]".to_string();
                                } else {
                                    let q = last_search.as_bytes();
                                    let start =
                                        last_match_end.unwrap_or(Pos::new(cursor_line, cursor_col));
                                    if let Some((m0, m1)) = find_next(&lines, q, start) {
                                        cursor_line = m0.line;
                                        cursor_col = m0.col;
                                        selection = Some((m0, m1));
                                        selection_anchor = None;
                                        last_match_end = Some(m1);
                                        search_feedback.clear();
                                    } else {
                                        search_feedback = "[not found]".to_string();
                                    }
                                }
                            }
                            _ if code <= 0xFF => {
                                let ch = code as u8 as char;
                                if ch.is_ascii_graphic() || ch == ' ' {
                                    search_query.push(ch);
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match code {
                        protocol::KEY_F1 => {
                            if let Some(topic) = topic_from_cursor_or_selection(
                                &lines,
                                cursor_line,
                                cursor_col,
                                selection,
                            ) {
                                match load_help_overlay(&topic) {
                                    Ok(Some(ov)) => help = Some(ov),
                                    Ok(None) => status_msg = format!("[no help for: {topic}]"),
                                    Err(err) => status_msg = format!("[help error: {err}]"),
                                }
                            } else {
                                status_msg = "[help: no topic]".to_string();
                            }
                        }
                        protocol::KEY_F5 => {
                            if build_in_flight {
                                status_msg = "[build already running]".to_string();
                                continue;
                            }

                            if modified {
                                if read_only {
                                    status_msg = "[read-only]".to_string();
                                    continue;
                                }
                                match write_file_lines(&path, &lines) {
                                    Ok(()) => {
                                        modified = false;
                                    }
                                    Err(err) => {
                                        status_msg = format!("[save error: {err}]");
                                        continue;
                                    }
                                }
                            }

                            build_in_flight = true;
                            status_msg = "[building...]".to_string();

                            let tx = run_tx.clone();
                            let hc = temple_hc_program();
                            let file_path = path.clone();
                            thread::spawn(move || {
                                let cwd = file_path.parent().unwrap_or(Path::new("."));
                                let out = Command::new(&hc)
                                    .arg("--check")
                                    .arg(&file_path)
                                    .current_dir(cwd)
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::piped())
                                    .output();

                                let out = match out {
                                    Ok(v) => v,
                                    Err(err) => {
                                        let _ = tx.send(RunMsg::BuildError {
                                            msg: format!("[build spawn error: {err}]"),
                                        });
                                        return;
                                    }
                                };

                                if out.status.success() {
                                    let mut cmd = Command::new(&hc);
                                    cmd.arg(&file_path).current_dir(cwd);
                                    let child = cmd.spawn();
                                    match child {
                                        Ok(child) => {
                                            let _ = tx.send(RunMsg::Launched { pid: child.id() });
                                        }
                                        Err(err) => {
                                            let _ = tx.send(RunMsg::BuildError {
                                                msg: format!("[run spawn error: {err}]"),
                                            });
                                        }
                                    }
                                    return;
                                }

                                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                                let merged = if stderr.trim().is_empty() {
                                    stdout
                                } else {
                                    stderr
                                };
                                let diag = parse_hc_diag_text(&merged);
                                let _ = tx.send(RunMsg::CompileError {
                                    diag,
                                    stderr: merged,
                                });
                            });
                        }
                        protocol::KEY_F3 => {
                            if last_search.is_empty() {
                                status_msg = "[no previous search]".to_string();
                            } else {
                                let q = last_search.as_bytes();
                                let start =
                                    last_match_end.unwrap_or(Pos::new(cursor_line, cursor_col));
                                if let Some((m0, m1)) = find_next(&lines, q, start) {
                                    cursor_line = m0.line;
                                    cursor_col = m0.col;
                                    selection = Some((m0, m1));
                                    selection_anchor = None;
                                    last_match_end = Some(m1);
                                } else {
                                    status_msg = "[not found]".to_string();
                                }
                            }
                        }
                        protocol::KEY_UP => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            if cursor_line > 0 {
                                cursor_line -= 1;
                                cursor_col = cursor_col.min(lines[cursor_line].len());
                            }
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_DOWN => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            if cursor_line + 1 < lines.len() {
                                cursor_line += 1;
                                cursor_col = cursor_col.min(lines[cursor_line].len());
                            }
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_LEFT => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            if cursor_col > 0 {
                                cursor_col -= 1;
                            } else if cursor_line > 0 {
                                cursor_line -= 1;
                                cursor_col = lines[cursor_line].len();
                            }
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_RIGHT => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            if cursor_col < lines[cursor_line].len() {
                                cursor_col += 1;
                            } else if cursor_line + 1 < lines.len() {
                                cursor_line += 1;
                                cursor_col = 0;
                            }
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_HOME => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            cursor_col = 0;
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_END => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            cursor_col = lines[cursor_line].len();
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_PAGE_UP => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            let jump = view_rows.max(1);
                            cursor_line = cursor_line.saturating_sub(jump);
                            cursor_col = cursor_col.min(lines[cursor_line].len());
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_PAGE_DOWN => {
                            let prev = Pos::new(cursor_line, cursor_col);
                            let jump = view_rows.max(1);
                            cursor_line = clamp_usize(
                                cursor_line.saturating_add(jump),
                                0,
                                lines.len().saturating_sub(1),
                            );
                            cursor_col = cursor_col.min(lines[cursor_line].len());
                            if shift {
                                selection_anchor.get_or_insert(prev);
                                if let Some(anchor) = selection_anchor {
                                    selection = Some((anchor, Pos::new(cursor_line, cursor_col)));
                                }
                            } else {
                                selection_anchor = None;
                                selection = None;
                            }
                        }
                        protocol::KEY_BACKSPACE => {
                            if read_only {
                                status_msg = "[read-only]".to_string();
                                continue;
                            }
                            if delete_selection(
                                &mut lines,
                                &mut cursor_line,
                                &mut cursor_col,
                                &mut selection,
                            ) {
                                modified = true;
                                continue;
                            }
                            if cursor_col > 0 {
                                cursor_col -= 1;
                                lines[cursor_line].remove(cursor_col);
                                modified = true;
                            } else if cursor_line > 0 {
                                let cur = lines.remove(cursor_line);
                                cursor_line -= 1;
                                cursor_col = lines[cursor_line].len();
                                lines[cursor_line].extend_from_slice(&cur);
                                modified = true;
                            }
                        }
                        protocol::KEY_DELETE => {
                            if read_only {
                                status_msg = "[read-only]".to_string();
                                continue;
                            }
                            if delete_selection(
                                &mut lines,
                                &mut cursor_line,
                                &mut cursor_col,
                                &mut selection,
                            ) {
                                modified = true;
                                continue;
                            }
                            if cursor_col < lines[cursor_line].len() {
                                lines[cursor_line].remove(cursor_col);
                                modified = true;
                            } else if cursor_line + 1 < lines.len() {
                                let next = lines.remove(cursor_line + 1);
                                lines[cursor_line].extend_from_slice(&next);
                                modified = true;
                            }
                        }
                        protocol::KEY_ENTER => {
                            if read_only {
                                status_msg = "[read-only]".to_string();
                                continue;
                            }
                            let _ = delete_selection(
                                &mut lines,
                                &mut cursor_line,
                                &mut cursor_col,
                                &mut selection,
                            );
                            let cur = lines[cursor_line].split_off(cursor_col);
                            cursor_line += 1;
                            cursor_col = 0;
                            lines.insert(cursor_line, cur);
                            modified = true;
                        }
                        protocol::KEY_TAB => {
                            if read_only {
                                status_msg = "[read-only]".to_string();
                                continue;
                            }
                            let _ = delete_selection(
                                &mut lines,
                                &mut cursor_line,
                                &mut cursor_col,
                                &mut selection,
                            );
                            for _ in 0..4 {
                                lines[cursor_line].insert(cursor_col, b' ');
                                cursor_col += 1;
                            }
                            modified = true;
                        }
                        _ if code <= 0xFF => {
                            if read_only {
                                status_msg = "[read-only]".to_string();
                                continue;
                            }
                            let deleted = delete_selection(
                                &mut lines,
                                &mut cursor_line,
                                &mut cursor_col,
                                &mut selection,
                            );
                            let ch = code as u8;
                            if (ch as char).is_ascii_graphic() || ch == b' ' {
                                lines[cursor_line].insert(cursor_col, ch);
                                cursor_col += 1;
                                modified = true;
                            } else if deleted {
                                modified = true;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if !did_event {
            thread::sleep(Duration::from_millis(16));
        }
    }
}
