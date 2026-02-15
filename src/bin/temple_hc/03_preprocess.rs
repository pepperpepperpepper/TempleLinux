use super::{Lexer, ParseError, Parser, Program, TokenKind};
use std::{
    collections::{BTreeMap, HashMap},
    env, fs, io, mem,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub(super) struct SourceSegment {
    pub(super) file: Arc<str>,
    pub(super) start_line: usize,
    pub(super) bytes: Vec<u8>,
}

pub(super) fn compile_segments(
    segments: Vec<SourceSegment>,
    macros: Arc<HashMap<String, String>>,
    bins_by_file: HashMap<Arc<str>, BTreeMap<u32, Vec<u8>>>,
) -> Result<Program, ParseError> {
    let mut tokens = Vec::new();

    let mut iter = segments.into_iter().peekable();
    while let Some(seg) = iter.next() {
        let is_last = iter.peek().is_none();
        let mut lex = Lexer::new(
            seg.file.clone(),
            seg.bytes.as_slice(),
            seg.start_line,
            macros.clone(),
        );
        loop {
            let t = lex.next_token()?;
            let is_eof = matches!(t.kind, TokenKind::Eof);
            if is_eof {
                if is_last {
                    tokens.push(t);
                }
                break;
            }
            tokens.push(t);
        }
    }

    let mut program = Parser::new(tokens).parse_program()?;
    program.bins_by_file = bins_by_file;
    Ok(program)
}

fn read_text_lossy(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    let cutoff = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..cutoff]).to_string())
}

pub(super) fn discover_templeos_root() -> Option<PathBuf> {
    if let Ok(v) = env::var("TEMPLEOS_ROOT") {
        if !v.trim().is_empty() {
            return Some(PathBuf::from(v));
        }
    }

    let mut bases = Vec::new();
    if let Ok(cwd) = env::current_dir() {
        bases.push(cwd);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            bases.push(dir.to_path_buf());
        }
    }

    for base in bases {
        let mut dir = base.clone();
        for _ in 0..8usize {
            let candidate = dir.join("third_party/TempleOS");
            if candidate.join("Kernel/FontStd.HC").exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    let sys = PathBuf::from("/usr/share/templelinux/TempleOS");
    if sys.join("Kernel/FontStd.HC").exists() {
        return Some(sys);
    }

    None
}

pub(super) fn resolve_templeos_path(
    spec: &str,
    base_dir: &Path,
    templeos_root: Option<&Path>,
) -> io::Result<PathBuf> {
    let base = if spec.starts_with("::/") {
        let Some(root) = templeos_root else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "TEMPLEOS_ROOT is not set and a TempleOS tree could not be discovered",
            ));
        };
        root.join(spec.trim_start_matches("::/"))
    } else if Path::new(spec).is_absolute() {
        PathBuf::from(spec)
    } else {
        base_dir.join(spec)
    };

    if base.exists() {
        return Ok(base);
    }

    if base.extension().is_none() {
        for ext in ["HC", "HH", "H"] {
            let candidate = base.with_extension(ext);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("include not found: {spec}"),
    ))
}

pub(super) fn preprocess_entry(
    path: &Path,
    templeos_root: Option<&Path>,
) -> io::Result<(
    Vec<SourceSegment>,
    HashMap<String, String>,
    HashMap<Arc<str>, BTreeMap<u32, Vec<u8>>>,
)> {
    let mut out = Vec::new();
    let mut defines: HashMap<String, String> = HashMap::new();
    let mut bins_by_file: HashMap<Arc<str>, BTreeMap<u32, Vec<u8>>> = HashMap::new();
    let mut stack: Vec<PathBuf> = Vec::new();
    preprocess_file(
        path,
        templeos_root,
        &mut stack,
        &mut defines,
        &mut bins_by_file,
        &mut out,
    )?;
    Ok((out, defines, bins_by_file))
}

fn preprocess_file(
    path: &Path,
    templeos_root: Option<&Path>,
    stack: &mut Vec<PathBuf>,
    defines: &mut HashMap<String, String>,
    bins_by_file: &mut HashMap<Arc<str>, BTreeMap<u32, Vec<u8>>>,
    out: &mut Vec<SourceSegment>,
) -> io::Result<()> {
    let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if stack.iter().any(|p| p == &abs) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cyclic include detected: {}", abs.display()),
        ));
    }
    stack.push(abs.clone());

    let file_label: Arc<str> = abs.display().to_string().into();
    let bytes = fs::read(&abs)?;
    let cutoff = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let src = &bytes[..cutoff];

    let mut bins: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    if cutoff < bytes.len() {
        let mut p = cutoff + 1;
        while p + 16 <= bytes.len() {
            let read_u32 = |buf: &[u8], off: usize| -> u32 {
                u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
            };

            let num = read_u32(&bytes, p);
            let _flags = read_u32(&bytes, p + 4);
            let size = read_u32(&bytes, p + 8) as usize;
            let _use_cnt = read_u32(&bytes, p + 12);
            p += 16;

            let remaining = bytes.len().saturating_sub(p);
            if size > remaining {
                // Some vendored `.DD`/`.HC` files appear to have truncated or corrupted bin tails.
                //
                // - If we're only missing a byte or two (common off-by-one truncation), keep the
                //   record by clamping to what's left.
                // - If the size is wildly out of range, stop parsing bins to avoid inventing a
                //   garbage record that then poisons `$IB` lookups.
                let overshoot = size - remaining;
                if overshoot <= 8 {
                    bins.insert(num, bytes[p..].to_vec());
                }
                break;
            }

            bins.insert(num, bytes[p..p + size].to_vec());
            p += size;
        }
    }
    bins_by_file.insert(file_label.clone(), bins);
    let base_dir = abs.parent().unwrap_or(Path::new("."));

    let mut seg_start_line: usize = 1;
    let mut line_no: usize = 1;
    let mut seg_bytes: Vec<u8> = Vec::new();

    fn trim_start_ascii(mut line: &[u8]) -> &[u8] {
        while let Some((&b, rest)) = line.split_first() {
            if b == b' ' || b == b'\t' {
                line = rest;
                continue;
            }
            break;
        }
        line
    }

    fn line_ends_with_define_continuation(line: &[u8]) -> bool {
        let mut end = line.len();
        if end > 0 && line[end - 1] == b'\n' {
            end -= 1;
        }
        if end > 0 && line[end - 1] == b'\r' {
            end -= 1;
        }
        while end > 0 {
            let b = line[end - 1];
            if b == b' ' || b == b'\t' {
                end -= 1;
                continue;
            }
            break;
        }
        end > 0 && line[end - 1] == b'\\'
    }

    fn strip_define_continuation_suffix(bytes: &mut Vec<u8>) {
        while matches!(bytes.last(), Some(b'\n' | b'\r')) {
            bytes.pop();
        }
        while matches!(bytes.last(), Some(b' ' | b'\t')) {
            bytes.pop();
        }
        if matches!(bytes.last(), Some(b'\\')) {
            bytes.pop();
        }
    }

    let mut p = 0usize;
    while p < src.len() {
        let nl = src[p..].iter().position(|&b| b == b'\n');
        let end = nl.map(|i| p + i + 1).unwrap_or(src.len());
        let line = &src[p..end];
        p = end;

        let trimmed = trim_start_ascii(line);
        let is_directive = trimmed.first() == Some(&b'#');

        if is_directive {
            let mut directive_bytes: Vec<u8> = trimmed.to_vec();
            if trimmed.starts_with(b"#define") {
                while line_ends_with_define_continuation(&directive_bytes) && p < src.len() {
                    strip_define_continuation_suffix(&mut directive_bytes);
                    directive_bytes.push(b' ');

                    let nl = src[p..].iter().position(|&b| b == b'\n');
                    let end = nl.map(|i| p + i + 1).unwrap_or(src.len());
                    let next_line = &src[p..end];
                    p = end;
                    directive_bytes.extend_from_slice(trim_start_ascii(next_line));

                    if next_line.ends_with(b"\n") {
                        line_no += 1;
                    }
                }
            }

            let trimmed_str = temple_rt::assets::decode_cp437_bytes(&directive_bytes);
            if trimmed.starts_with(b"#include") {
                if !seg_bytes.is_empty() {
                    out.push(SourceSegment {
                        file: file_label.clone(),
                        start_line: seg_start_line,
                        bytes: mem::take(&mut seg_bytes),
                    });
                }

                let spec = parse_include_spec(&trimmed_str)?;
                let include_path = resolve_templeos_path(&spec, base_dir, templeos_root)?;
                preprocess_file(
                    &include_path,
                    templeos_root,
                    stack,
                    defines,
                    bins_by_file,
                    out,
                )?;

                seg_start_line = line_no + 1;
            } else {
                if trimmed.starts_with(b"#define") {
                    if let Some((k, v)) = parse_define(&trimmed_str) {
                        defines.insert(k, v);
                    }
                }
                if !seg_bytes.is_empty() {
                    out.push(SourceSegment {
                        file: file_label.clone(),
                        start_line: seg_start_line,
                        bytes: mem::take(&mut seg_bytes),
                    });
                }
                seg_start_line = line_no + 1;
            }
        } else {
            seg_bytes.extend_from_slice(line);
        }

        if line.ends_with(b"\n") {
            line_no += 1;
        }
    }

    if !seg_bytes.is_empty() {
        out.push(SourceSegment {
            file: file_label.clone(),
            start_line: seg_start_line,
            bytes: seg_bytes,
        });
    }

    stack.pop();
    Ok(())
}

fn parse_include_spec(line: &str) -> io::Result<String> {
    let rest = line
        .strip_prefix("#include")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad include directive"))?
        .trim_start();

    if let Some(rest) = rest.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return Ok(rest[..end].to_string());
        }
    }

    if let Some(rest) = rest.strip_prefix('<') {
        if let Some(end) = rest.find('>') {
            return Ok(rest[..end].to_string());
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("could not parse include: {line}"),
    ))
}

fn parse_define(line: &str) -> Option<(String, String)> {
    let line = line.trim_end();
    let rest = line.strip_prefix("#define")?.trim_start();
    if rest.is_empty() {
        return None;
    }

    let mut end = 0usize;
    for (i, ch) in rest.char_indices() {
        if ch.is_whitespace() || ch == '(' {
            break;
        }
        end = i + ch.len_utf8();
    }

    if end == 0 {
        return None;
    }

    let name = rest[..end].to_string();

    // Skip function-like macros: #define F(x) ...
    if rest[end..].starts_with('(') {
        return None;
    }

    let mut value = rest[end..].trim_start().to_string();
    if let Some((before, _)) = value.split_once("//") {
        value = before.trim_end().to_string();
    }
    if value.is_empty() {
        value = "0".to_string();
    }
    Some((name, value))
}

pub(super) fn builtin_defines() -> HashMap<String, String> {
    let mut out = HashMap::new();

    for (k, v) in [
        ("TRUE", "1"),
        ("FALSE", "0"),
        ("ON", "1"),
        ("OFF", "0"),
        ("NULL", "0"),
        // TempleOS-like special macros.
        //
        // NOTE: `__DIR__`/`__FILE__` are handled by the lexer as built-ins so they can expand to
        // per-file values even when TempleOS headers define them via `#exe{...}`.
        // Char constants (from ::/Kernel/KernelA.HH).
        ("CH_CTRLA", "0x01"),
        ("CH_CTRLB", "0x02"),
        ("CH_CTRLC", "0x03"),
        ("CH_CTRLD", "0x04"),
        ("CH_CTRLE", "0x05"),
        ("CH_CTRLF", "0x06"),
        ("CH_CTRLG", "0x07"),
        ("CH_CTRLH", "0x08"),
        ("CH_CTRLI", "0x09"),
        ("CH_CTRLJ", "0x0A"),
        ("CH_CTRLK", "0x0B"),
        ("CH_CTRLL", "0x0C"),
        ("CH_CTRLM", "0x0D"),
        ("CH_CTRLN", "0x0E"),
        ("CH_CTRLO", "0x0F"),
        ("CH_CTRLP", "0x10"),
        ("CH_CTRLQ", "0x11"),
        ("CH_CTRLR", "0x12"),
        ("CH_CTRLS", "0x13"),
        ("CH_CTRLT", "0x14"),
        ("CH_CTRLU", "0x15"),
        ("CH_CTRLV", "0x16"),
        ("CH_CTRLW", "0x17"),
        ("CH_CTRLX", "0x18"),
        ("CH_CTRLY", "0x19"),
        ("CH_CTRLZ", "0x1A"),
        ("CH_BACKSPACE", "0x08"),
        ("CH_ESC", "0x1B"),
        ("CH_SHIFT_ESC", "0x1C"),
        ("CH_SHIFT_SPACE", "0x1F"),
        ("CH_SPACE", "0x20"),
        // Messages (from ::/Kernel/KernelA.HH).
        ("MSG_NULL", "0"),
        ("MSG_CMD", "1"),
        ("MSG_KEY_DOWN", "2"),
        ("MSG_KEY_UP", "3"),
        ("MSG_MS_MOVE", "4"),
        ("MSG_MS_L_DOWN", "5"),
        ("MSG_MS_L_UP", "6"),
        ("MSG_MS_R_DOWN", "9"),
        ("MSG_MS_R_UP", "10"),
        // Window inhibit flags (subset).
        ("WIF_SELF_MS_L", "0x0008"),
        ("WIF_SELF_MS_R", "0x0020"),
        ("WIF_SELF_KEY_DESC", "0x1000"),
        ("WIF_FOCUS_TASK_MS_L_D", "0x00100000"),
        ("WIF_FOCUS_TASK_MS_R_D", "0x00400000"),
        ("WIG_DBL_CLICK", "0x00500000"),
        ("WIG_USER_TASK_DFT", "0x1000"),
        // Device context flags (subset).
        ("DCF_TRANSFORMATION", "0x100"),
        ("DCF_SYMMETRY", "0x200"),
        ("DCF_JUST_MIRROR", "0x400"),
        // Scan codes (subset).
        ("SC_ESC", "0x01"),
        ("SC_BACKSPACE", "0x0E"),
        ("SC_TAB", "0x0F"),
        ("SC_ENTER", "0x1C"),
        ("SC_SHIFT", "0x2A"),
        ("SC_CTRL", "0x1D"),
        ("SC_ALT", "0x38"),
        ("SC_CAPS", "0x3A"),
        ("SC_NUM", "0x45"),
        ("SC_SCROLL", "0x46"),
        ("SC_CURSOR_UP", "0x48"),
        ("SC_CURSOR_DOWN", "0x50"),
        ("SC_CURSOR_LEFT", "0x4B"),
        ("SC_CURSOR_RIGHT", "0x4D"),
        ("SC_PAGE_UP", "0x49"),
        ("SC_PAGE_DOWN", "0x51"),
        ("SC_HOME", "0x47"),
        ("SC_END", "0x4F"),
        ("SC_INS", "0x52"),
        ("SC_DELETE", "0x53"),
        ("SC_F1", "0x3B"),
        ("SC_F2", "0x3C"),
        ("SC_F3", "0x3D"),
        ("SC_F4", "0x3E"),
        ("SC_F5", "0x3F"),
        ("SC_F6", "0x40"),
        ("SC_F7", "0x41"),
        ("SC_F8", "0x42"),
        ("SC_F9", "0x43"),
        ("SC_F10", "0x44"),
        ("SC_F11", "0x57"),
        ("SC_F12", "0x58"),
        // Scan code flags (subset; pre-expanded numeric values).
        ("SCF_KEY_UP", "0x100"),
        ("SCF_SHIFT", "0x200"),
        ("SCF_CTRL", "0x400"),
        ("SCF_ALT", "0x800"),
        ("SCF_DELETE", "0x40000"),
        ("SCF_INS", "0x80000"),
        // File utils (subset).
        ("FUF_JUST_DIRS", "0x0000400"),
        // GetStr flags (subset).
        ("GSF_WITH_NEW_LINE", "2"),
        // Control flags/types (from ::/Kernel/KernelA.HH).
        ("CTRLT_GENERIC", "0"),
        ("CTRLF_SHOW", "1"),
        ("CTRLF_BORDER", "2"),
        ("CTRLF_CAPTURE_LEFT_MS", "4"),
        ("CTRLF_CAPTURE_RIGHT_MS", "8"),
        ("CTRLF_CLICKED", "16"),
        // Graphics (common upstream globals/macros).
        ("GR_WIDTH", "SCR_W"),
        ("GR_HEIGHT", "SCR_H"),
        ("GR_Z_ALL", "1073741823"),
        ("COLORS_NUM", "16"),
        ("COLOR_INVALID", "16"),
        ("COLOR_MONO", "0xFF"),
        ("FONT_WIDTH", "8"),
        ("FONT_HEIGHT", "8"),
        // Date/time.
        ("CDATE_FREQ", "49710"),
    ] {
        out.insert(k.to_string(), v.to_string());
    }

    out
}
