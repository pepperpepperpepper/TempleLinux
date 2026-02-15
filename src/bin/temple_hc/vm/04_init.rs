use super::prelude::*;
use super::{ArrayValue, Env, Obj, Value, Vm};

impl Vm {
    pub(crate) fn new(
        rt: TempleRt,
        program: Program,
        macros: Arc<HashMap<String, String>>,
    ) -> Self {
        let mut env = Env::new();

        let (w, h) = rt.size();
        env.define("SCR_W".to_string(), Value::Int(w as i64));
        env.define("SCR_H".to_string(), Value::Int(h as i64));
        env.define("TEXT_COLS".to_string(), Value::Int((w / 8) as i64));
        env.define("TEXT_ROWS".to_string(), Value::Int((h / 8) as i64));

        env.define("TRUE".to_string(), Value::Int(1));
        env.define("FALSE".to_string(), Value::Int(0));
        env.define("NULL".to_string(), Value::Int(0));
        env.define("local_time_offset".to_string(), Value::Int(0));
        // Common math constants used in CP437-heavy upstream sources. For example, many TempleOS
        // programs use CP437 byte 0xE3 (`π`) directly in expressions.
        env.define("π".to_string(), Value::Float(std::f64::consts::PI));
        env.define("sqrt2".to_string(), Value::Float(2.0_f64.sqrt()));

        let cwd = Self::compute_initial_cwd();

        // TempleOS-ish char constants (from ::/Kernel/KernelA.HH).
        env.define("CH_BACKSPACE".to_string(), Value::Int(0x08));
        env.define("CH_ESC".to_string(), Value::Int(0x1B));
        env.define("CH_SHIFT_ESC".to_string(), Value::Int(0x1C));
        env.define("CH_SHIFT_SPACE".to_string(), Value::Int(0x1F));
        env.define("CH_SPACE".to_string(), Value::Int(0x20));

        // TempleOS-ish std palette indices.
        env.define("BLACK".to_string(), Value::Int(0));
        env.define("BLUE".to_string(), Value::Int(1));
        env.define("GREEN".to_string(), Value::Int(2));
        env.define("CYAN".to_string(), Value::Int(3));
        env.define("RED".to_string(), Value::Int(4));
        env.define("MAGENTA".to_string(), Value::Int(5));
        env.define("PURPLE".to_string(), Value::Int(5));
        env.define("BROWN".to_string(), Value::Int(6));
        env.define("LGRAY".to_string(), Value::Int(7));
        env.define("LTGRAY".to_string(), Value::Int(7));
        env.define("DGRAY".to_string(), Value::Int(8));
        env.define("DKGRAY".to_string(), Value::Int(8));
        env.define("LTBLUE".to_string(), Value::Int(9));
        env.define("LTGREEN".to_string(), Value::Int(10));
        env.define("LTCYAN".to_string(), Value::Int(11));
        env.define("LTRED".to_string(), Value::Int(12));
        env.define("LTMAGENTA".to_string(), Value::Int(13));
        env.define("LTPURPLE".to_string(), Value::Int(13));
        env.define("YELLOW".to_string(), Value::Int(14));
        env.define("WHITE".to_string(), Value::Int(15));
        env.define("COLORS_NUM".to_string(), Value::Int(16));

        let mut define_lists: HashMap<String, Vec<String>> = HashMap::new();
        define_lists.insert(
            "ST_COLORS".to_string(),
            vec![
                "BLACK".to_string(),
                "BLUE".to_string(),
                "GREEN".to_string(),
                "CYAN".to_string(),
                "RED".to_string(),
                "PURPLE".to_string(),
                "BROWN".to_string(),
                "LTGRAY".to_string(),
                "DKGRAY".to_string(),
                "LTBLUE".to_string(),
                "LTGREEN".to_string(),
                "LTCYAN".to_string(),
                "LTRED".to_string(),
                "LTPURPLE".to_string(),
                "YELLOW".to_string(),
                "WHITE".to_string(),
            ],
        );

        env.define(
            "KEY_ESCAPE".to_string(),
            Value::Int(protocol::KEY_ESCAPE as i64),
        );
        env.define(
            "KEY_ENTER".to_string(),
            Value::Int(protocol::KEY_ENTER as i64),
        );
        env.define(
            "KEY_BACKSPACE".to_string(),
            Value::Int(protocol::KEY_BACKSPACE as i64),
        );
        env.define(
            "KEY_DELETE".to_string(),
            Value::Int(protocol::KEY_DELETE as i64),
        );
        env.define("KEY_TAB".to_string(), Value::Int(protocol::KEY_TAB as i64));
        env.define(
            "KEY_HOME".to_string(),
            Value::Int(protocol::KEY_HOME as i64),
        );
        env.define("KEY_END".to_string(), Value::Int(protocol::KEY_END as i64));
        env.define(
            "KEY_PAGE_UP".to_string(),
            Value::Int(protocol::KEY_PAGE_UP as i64),
        );
        env.define(
            "KEY_PAGE_DOWN".to_string(),
            Value::Int(protocol::KEY_PAGE_DOWN as i64),
        );
        env.define(
            "KEY_INSERT".to_string(),
            Value::Int(protocol::KEY_INSERT as i64),
        );
        env.define(
            "KEY_SHIFT".to_string(),
            Value::Int(protocol::KEY_SHIFT as i64),
        );
        env.define(
            "KEY_CONTROL".to_string(),
            Value::Int(protocol::KEY_CONTROL as i64),
        );
        env.define("KEY_ALT".to_string(), Value::Int(protocol::KEY_ALT as i64));
        env.define(
            "KEY_SUPER".to_string(),
            Value::Int(protocol::KEY_SUPER as i64),
        );
        env.define("KEY_F1".to_string(), Value::Int(protocol::KEY_F1 as i64));
        env.define("KEY_F2".to_string(), Value::Int(protocol::KEY_F2 as i64));
        env.define("KEY_F3".to_string(), Value::Int(protocol::KEY_F3 as i64));
        env.define("KEY_F4".to_string(), Value::Int(protocol::KEY_F4 as i64));
        env.define("KEY_F5".to_string(), Value::Int(protocol::KEY_F5 as i64));
        env.define("KEY_F6".to_string(), Value::Int(protocol::KEY_F6 as i64));
        env.define("KEY_F7".to_string(), Value::Int(protocol::KEY_F7 as i64));
        env.define("KEY_F8".to_string(), Value::Int(protocol::KEY_F8 as i64));
        env.define("KEY_F9".to_string(), Value::Int(protocol::KEY_F9 as i64));
        env.define("KEY_F10".to_string(), Value::Int(protocol::KEY_F10 as i64));
        env.define("KEY_F11".to_string(), Value::Int(protocol::KEY_F11 as i64));
        env.define("KEY_F12".to_string(), Value::Int(protocol::KEY_F12 as i64));
        env.define(
            "KEY_LEFT".to_string(),
            Value::Int(protocol::KEY_LEFT as i64),
        );
        env.define(
            "KEY_RIGHT".to_string(),
            Value::Int(protocol::KEY_RIGHT as i64),
        );
        env.define("KEY_UP".to_string(), Value::Int(protocol::KEY_UP as i64));
        env.define(
            "KEY_DOWN".to_string(),
            Value::Int(protocol::KEY_DOWN as i64),
        );

        let ms_pos = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("x".to_string(), Value::Int(0)),
                ("y".to_string(), Value::Int(0)),
            ]),
        }));
        let ms = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("lb".to_string(), Value::Int(0)),
                ("pos".to_string(), Value::Obj(ms_pos.clone())),
            ]),
        }));
        env.define("ms".to_string(), Value::Obj(ms.clone()));

        let dc_ls = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("x".to_string(), Value::Int(0)),
                ("y".to_string(), Value::Int(0)),
                ("z".to_string(), Value::Int(0)),
            ]),
        }));
        let dc_alias = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("color".to_string(), Value::Int(15)),
                ("thick".to_string(), Value::Int(1)),
                ("flags".to_string(), Value::Int(0)),
                ("ls".to_string(), Value::Obj(dc_ls)),
                ("width".to_string(), Value::Int(w as i64)),
                ("height".to_string(), Value::Int(h as i64)),
            ]),
        }));

        // TempleOS-ish control list head (for `Fs->last_ctrl`).
        let ctrl_head = Rc::new(RefCell::new(Obj {
            fields: HashMap::new(),
        }));
        ctrl_head
            .borrow_mut()
            .fields
            .insert("next".to_string(), Value::Obj(ctrl_head.clone()));
        ctrl_head
            .borrow_mut()
            .fields
            .insert("last".to_string(), Value::Obj(ctrl_head.clone()));

        // Minimal task scrollbar state (`CWinScroll`-ish) used by demos like `::/Demo/Graphics/ScrollBars.HC`.
        let horz_scroll = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("min".to_string(), Value::Int(0)),
                ("pos".to_string(), Value::Int(0)),
                ("max".to_string(), Value::Int(0)),
                ("flags".to_string(), Value::Int(0)),
                ("color".to_string(), Value::Int(15)),
            ]),
        }));
        let vert_scroll = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("min".to_string(), Value::Int(0)),
                ("pos".to_string(), Value::Int(0)),
                ("max".to_string(), Value::Int(0)),
                ("flags".to_string(), Value::Int(0)),
                ("color".to_string(), Value::Int(15)),
            ]),
        }));

        let fs = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("pix_width".to_string(), Value::Int(w as i64)),
                ("pix_height".to_string(), Value::Int(h as i64)),
                ("pix_left".to_string(), Value::Int(0)),
                ("pix_top".to_string(), Value::Int(0)),
                ("win_width".to_string(), Value::Int((w / 8) as i64)),
                ("win_height".to_string(), Value::Int((h / 8) as i64)),
                ("cur_dir".to_string(), Value::Str(cwd.clone())),
                ("win_inhibit".to_string(), Value::Int(0)),
                ("draw_it".to_string(), Value::Int(0)),
                ("task_end_cb".to_string(), Value::Int(0)),
                ("animate_task".to_string(), Value::Int(0)),
                ("text_attr".to_string(), Value::Int(0)),
                ("last_ctrl".to_string(), Value::Obj(ctrl_head)),
                ("horz_scroll".to_string(), Value::Obj(horz_scroll)),
                ("vert_scroll".to_string(), Value::Obj(vert_scroll)),
            ]),
        }));
        env.define("Fs".to_string(), Value::Obj(fs));

        // Some TempleOS sources expect persistent/global tasks like `adam_task` and
        // `sys_winmgr_task` to exist (even if TempleLinux doesn't emulate full task lifetimes).
        // By default, these are distinct tasks so "Must be Adam Included" guards still fire
        // unless the program explicitly sets them up.
        let adam_ctrl_head = Rc::new(RefCell::new(Obj {
            fields: HashMap::new(),
        }));
        adam_ctrl_head
            .borrow_mut()
            .fields
            .insert("next".to_string(), Value::Obj(adam_ctrl_head.clone()));
        adam_ctrl_head
            .borrow_mut()
            .fields
            .insert("last".to_string(), Value::Obj(adam_ctrl_head.clone()));
        let adam_horz_scroll = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("min".to_string(), Value::Int(0)),
                ("pos".to_string(), Value::Int(0)),
                ("max".to_string(), Value::Int(0)),
                ("flags".to_string(), Value::Int(0)),
                ("color".to_string(), Value::Int(15)),
            ]),
        }));
        let adam_vert_scroll = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("min".to_string(), Value::Int(0)),
                ("pos".to_string(), Value::Int(0)),
                ("max".to_string(), Value::Int(0)),
                ("flags".to_string(), Value::Int(0)),
                ("color".to_string(), Value::Int(15)),
            ]),
        }));
        let adam_task = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("pix_width".to_string(), Value::Int(w as i64)),
                ("pix_height".to_string(), Value::Int(h as i64)),
                ("pix_left".to_string(), Value::Int(0)),
                ("pix_top".to_string(), Value::Int(0)),
                ("win_width".to_string(), Value::Int((w / 8) as i64)),
                ("win_height".to_string(), Value::Int((h / 8) as i64)),
                ("win_inhibit".to_string(), Value::Int(0)),
                ("draw_it".to_string(), Value::Int(0)),
                ("task_end_cb".to_string(), Value::Int(0)),
                ("animate_task".to_string(), Value::Int(0)),
                ("text_attr".to_string(), Value::Int(0)),
                ("last_ctrl".to_string(), Value::Obj(adam_ctrl_head)),
                ("horz_scroll".to_string(), Value::Obj(adam_horz_scroll)),
                ("vert_scroll".to_string(), Value::Obj(adam_vert_scroll)),
            ]),
        }));
        env.define("adam_task".to_string(), Value::Obj(adam_task));

        let winmgr_ctrl_head = Rc::new(RefCell::new(Obj {
            fields: HashMap::new(),
        }));
        winmgr_ctrl_head
            .borrow_mut()
            .fields
            .insert("next".to_string(), Value::Obj(winmgr_ctrl_head.clone()));
        winmgr_ctrl_head
            .borrow_mut()
            .fields
            .insert("last".to_string(), Value::Obj(winmgr_ctrl_head.clone()));
        let winmgr_horz_scroll = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("min".to_string(), Value::Int(0)),
                ("pos".to_string(), Value::Int(0)),
                ("max".to_string(), Value::Int(0)),
                ("flags".to_string(), Value::Int(0)),
                ("color".to_string(), Value::Int(15)),
            ]),
        }));
        let winmgr_vert_scroll = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("min".to_string(), Value::Int(0)),
                ("pos".to_string(), Value::Int(0)),
                ("max".to_string(), Value::Int(0)),
                ("flags".to_string(), Value::Int(0)),
                ("color".to_string(), Value::Int(15)),
            ]),
        }));
        let sys_winmgr_task = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("pix_width".to_string(), Value::Int(w as i64)),
                ("pix_height".to_string(), Value::Int(h as i64)),
                ("pix_left".to_string(), Value::Int(0)),
                ("pix_top".to_string(), Value::Int(0)),
                ("win_width".to_string(), Value::Int((w / 8) as i64)),
                ("win_height".to_string(), Value::Int((h / 8) as i64)),
                ("win_inhibit".to_string(), Value::Int(0)),
                ("draw_it".to_string(), Value::Int(0)),
                ("task_end_cb".to_string(), Value::Int(0)),
                ("animate_task".to_string(), Value::Int(0)),
                ("text_attr".to_string(), Value::Int(0)),
                ("last_ctrl".to_string(), Value::Obj(winmgr_ctrl_head)),
                ("horz_scroll".to_string(), Value::Obj(winmgr_horz_scroll)),
                ("vert_scroll".to_string(), Value::Obj(winmgr_vert_scroll)),
            ]),
        }));
        env.define("sys_winmgr_task".to_string(), Value::Obj(sys_winmgr_task));

        let gr = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("dc".to_string(), Value::Obj(dc_alias.clone())),
                ("dc2".to_string(), Value::Obj(dc_alias.clone())),
                ("hide_col".to_string(), Value::Int(0)),
                ("hide_row".to_string(), Value::Int(0)),
                ("pan_text_x".to_string(), Value::Int(0)),
                ("pan_text_y".to_string(), Value::Int(0)),
                ("fp_draw_ms".to_string(), Value::Int(0)),
                ("fp_wall_paper".to_string(), Value::Int(0)),
            ]),
        }));
        env.define("gr".to_string(), Value::Obj(gr));

        // Grid snapping globals (minimal), used by demos like `::/Demo/Graphics/Grid.HC`.
        let ms_grid = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([
                ("snap".to_string(), Value::Int(0)),
                ("x".to_string(), Value::Int(0)),
                ("y".to_string(), Value::Int(0)),
            ]),
        }));
        env.define("ms_grid".to_string(), Value::Obj(ms_grid));

        // TempleOS-ish global text state (`CTextGlbls`), at least enough for font hacking demos like
        // `::/Demo/ExtChars.HC` (which edits `text.font[255]` at runtime).
        let text_font = Rc::new(RefCell::new(ArrayValue {
            elems: temple_rt::assets::TEMPLEOS_SYS_FONT_STD_U64
                .iter()
                .map(|&bits| Value::Int(bits as i64))
                .collect(),
            elem_bytes: 8,
        }));
        let text = Rc::new(RefCell::new(Obj {
            fields: HashMap::from([("font".to_string(), Value::Array(text_font))]),
        }));
        env.define("text".to_string(), Value::Obj(text));

        let seed = std::env::var("TEMPLE_HC_SEED")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let (rng_seed, rng_state) = if seed == 0 {
            (0, Self::now_nanos() ^ 0x9E37_79B9_7F4A_7C15)
        } else {
            (seed, seed)
        };

        let start_instant = std::time::Instant::now();
        let fixed_ts = std::env::var("TEMPLE_HC_FIXED_TS")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok());

        let mut rt = rt;
        rt.clear(0);

        Self {
            rt,
            env,
            macros,
            define_lists,
            reg_defaults: HashMap::new(),
            program,
            cwd,
            doldoc_bin_ptr_cache: HashMap::new(),
            doldoc_bin_len_by_ptr: HashMap::new(),
            heap: vec![0u8],
            scan_char: 0,
            key_queue: VecDeque::new(),
            msg_queue: VecDeque::new(),
            shift_down: false,
            ctrl_down: false,
            alt_down: false,
            ctrl_capture_left: None,
            ms,
            ms_pos,
            dc_alias,
            text_x: 0,
            text_y: 0,
            text_fg: 15,
            text_bg: 0,
            rng_seed,
            rng_state,
            start_instant,
            fixed_ts,
            is_mute: false,
            capture: None,
            menu_stack: Vec::new(),
            in_draw_it: false,
            last_host_error: None,
            main_called: false,
        }
    }

    fn compute_initial_cwd() -> String {
        let Ok(root) = std::env::var("TEMPLE_ROOT") else {
            return "/Home".to_string();
        };
        let root = root.trim();
        if root.is_empty() {
            return "/Home".to_string();
        }

        let root = PathBuf::from(root);
        let root = std::fs::canonicalize(&root).unwrap_or(root);

        let Ok(cur) = std::env::current_dir() else {
            return "/Home".to_string();
        };
        let cur = std::fs::canonicalize(&cur).unwrap_or(cur);

        let Ok(rel) = cur.strip_prefix(&root) else {
            return "/Home".to_string();
        };

        let rel = rel.to_string_lossy().replace('\\', "/");
        let rel = rel.trim_start_matches('/');
        if rel.is_empty() {
            "/".to_string()
        } else {
            format!("/{rel}")
        }
    }

    pub(super) fn define_sub(&self, idx: i64, list_name: &str) -> Option<String> {
        let list = self.define_lists.get(list_name)?;
        if idx < 0 {
            return None;
        }
        list.get(idx as usize).cloned()
    }

    pub(super) fn now_nanos() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};

        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
