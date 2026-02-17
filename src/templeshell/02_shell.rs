struct Shell {
    root_dir: PathBuf,
    cwd: TemplePath,
    input: String,
    cursor: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    history_draft: String,
    clipboard: HostClipboard,
    vars: std::collections::BTreeMap<String, String>,
    tapp_connected: bool,
    tapp_child: Option<std::process::Child>,
    tapp_last: Option<TappLaunch>,
    pending_window_titles: std::collections::VecDeque<String>,
    pending_window_kinds: std::collections::VecDeque<PendingWindowKind>,
    exit_requested: bool,
    pending_screenshot: Option<(String, PathBuf)>,
    browser: Option<FileBrowserState>,
    doc_viewer: Option<DocViewerState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingWindowKind {
    Normal,
    Wallpaper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocActionOutcome {
    Unsupported,
    KeepDocViewer,
    CloseDocViewer,
}

#[derive(Default)]
struct DolDocMeta {
    links: Vec<DocLink>,
    anchors: std::collections::BTreeMap<String, usize>,
    sprites: Vec<DocSprite>,
}

struct BuiltDoc {
    lines: Vec<Vec<Cell>>,
    links: Vec<DocLink>,
    anchors: std::collections::BTreeMap<String, usize>,
    sprites: Vec<DocSprite>,
}

impl Shell {
    fn new(test_mode: bool) -> Self {
        let root_dir = pick_temple_root(test_mode);
        let _ = std::fs::create_dir_all(&root_dir);
        let _ = std::fs::create_dir_all(root_dir.join("Home"));
        let _ = std::fs::create_dir_all(root_dir.join("Doc"));
        let _ = std::fs::create_dir_all(root_dir.join("Cfg"));
        let _ = std::fs::create_dir_all(root_dir.join("Apps"));
        let linuxbridge_path = root_dir.join("Apps/LinuxBridge.HC");
        if test_mode || !linuxbridge_path.exists() {
            let _ = std::fs::write(&linuxbridge_path, TEMPLELINUX_LINUXBRIDGE_HC);
        }

        if !test_mode {
            let cfg_dir = root_dir.join("Cfg");
            let history_path = cfg_dir.join("History.txt");
            let vars_path = cfg_dir.join("Vars.txt");
            let autostart_path = cfg_dir.join("AutoStart.tl");

            let first_run =
                !history_path.exists() && !vars_path.exists() && !autostart_path.exists();
            let disable = std::env::var("TEMPLE_NO_FIRST_RUN_AUTOSTART")
                .ok()
                .is_some_and(|v| v == "1");

            if first_run && !disable {
                let _ = std::fs::write(&autostart_path, TEMPLELINUX_DEFAULT_AUTOSTART_TL);
            }
        }

        let mut shell = Self {
            root_dir,
            cwd: TemplePath::root(),
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_pos: None,
            history_draft: String::new(),
            clipboard: HostClipboard::default(),
            vars: std::collections::BTreeMap::new(),
            tapp_connected: false,
            tapp_child: None,
            tapp_last: None,
            pending_window_titles: std::collections::VecDeque::new(),
            pending_window_kinds: std::collections::VecDeque::new(),
            exit_requested: false,
            pending_screenshot: None,
            browser: None,
            doc_viewer: None,
        };
        if !test_mode {
            shell.load_state();
        }
        shell
    }

    fn load_state(&mut self) {
        self.load_vars();
        self.load_history();
    }

    fn queue_window_title(&mut self, title: String) {
        self.queue_window(title, PendingWindowKind::Normal);
    }

    fn queue_wallpaper_title(&mut self, title: String) {
        self.queue_window(title, PendingWindowKind::Wallpaper);
    }

    fn queue_window(&mut self, title: String, kind: PendingWindowKind) {
        self.pending_window_titles.push_back(title);
        self.pending_window_kinds.push_back(kind);
    }

    fn take_queued_window_title(&mut self) -> Option<String> {
        self.pending_window_titles.pop_front()
    }

    fn take_queued_window_kind(&mut self) -> PendingWindowKind {
        self.pending_window_kinds
            .pop_front()
            .unwrap_or(PendingWindowKind::Normal)
    }

    fn take_exit_requested(&mut self) -> bool {
        if self.exit_requested {
            self.exit_requested = false;
            true
        } else {
            false
        }
    }

    fn take_pending_screenshot(&mut self) -> Option<(String, PathBuf)> {
        self.pending_screenshot.take()
    }

    fn cfg_dir(&self) -> PathBuf {
        self.root_dir.join("Cfg")
    }

    fn history_path(&self) -> PathBuf {
        self.cfg_dir().join("History.txt")
    }

    fn vars_path(&self) -> PathBuf {
        self.cfg_dir().join("Vars.txt")
    }

    fn autostart_path(&self) -> PathBuf {
        self.cfg_dir().join("AutoStart.tl")
    }

    fn load_history(&mut self) {
        const MAX_HISTORY: usize = 1000;

        let path = self.history_path();
        let Ok(buf) = std::fs::read(&path) else {
            return;
        };
        let text = String::from_utf8_lossy(&buf);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            self.history.push(line.to_string());
        }
        if self.history.len() > MAX_HISTORY {
            let start = self.history.len() - MAX_HISTORY;
            self.history.drain(..start);
        }
    }

    fn save_history(&self) {
        const MAX_HISTORY: usize = 1000;

        let path = self.history_path();
        let mut out = String::new();
        let start = self.history.len().saturating_sub(MAX_HISTORY);
        for line in &self.history[start..] {
            out.push_str(line);
            out.push('\n');
        }
        let _ = std::fs::write(&path, out);
    }

    fn load_vars(&mut self) {
        let path = self.vars_path();
        let Ok(buf) = std::fs::read(&path) else {
            return;
        };
        let text = String::from_utf8_lossy(&buf);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let k = k.trim();
            if k.is_empty() {
                continue;
            }
            self.vars.insert(k.to_string(), v.trim().to_string());
        }
    }

    fn save_vars(&self) {
        let path = self.vars_path();
        let mut out = String::new();
        for (k, v) in &self.vars {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
        let _ = std::fs::write(&path, out);
    }

    fn run_autostart(&mut self, term: &mut Terminal) {
        let path = self.autostart_path();
        let Ok(buf) = std::fs::read(&path) else {
            return;
        };
        let text = String::from_utf8_lossy(&buf);

        use fmt::Write as _;
        let _ = writeln!(term, "[autostart: {}]", path.display());
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let _ = writeln!(term, ">> {line}");
            self.exec_line(line, term);
        }
    }

    fn prompt(&self) -> String {
        format!("{}> ", self.cwd.display())
    }

    fn draw_prompt(&self, term: &mut Terminal) {
        if self.doc_viewer.is_some() {
            self.render_doc_viewer(term);
            return;
        }
        if self.browser.is_some() {
            self.render_browser(term);
            return;
        }

        term.fill_row(PROMPT_ROW, COLOR_FG, COLOR_BG);

        let prompt = self.prompt();
        let max_cols = TERM_COLS as usize;
        if max_cols == 0 {
            return;
        }

        if prompt.len() >= max_cols {
            let start = prompt.len() - max_cols;
            term.write_at(0, PROMPT_ROW, COLOR_FG, COLOR_BG, &prompt[start..]);
            term.invert_cell((max_cols - 1) as u32, PROMPT_ROW);
            return;
        }

        let prompt_len = prompt.len();
        let avail = max_cols - prompt_len;

        let cursor = self.cursor.min(self.input.len());
        let start = if cursor > avail { cursor - avail } else { 0 };
        let end = (start + avail).min(self.input.len());
        let visible_input = &self.input[start..end];

        let mut line = String::with_capacity(prompt_len + visible_input.len());
        line.push_str(&prompt);
        line.push_str(visible_input);
        term.write_at(0, PROMPT_ROW, COLOR_FG, COLOR_BG, &line);

        let cursor_col = prompt_len.saturating_add(cursor - start);
        if cursor_col < max_cols {
            term.invert_cell(cursor_col as u32, PROMPT_ROW);
        }
    }

    fn in_browser(&self) -> bool {
        self.browser.is_some()
    }

    fn in_doc_viewer(&self) -> bool {
        self.doc_viewer.is_some()
    }

    fn browser_list_rows() -> usize {
        let header_rows = 2u32; // title + tabs
        PROMPT_ROW.saturating_sub(header_rows).max(1) as usize
    }

    fn browser_current_len(state: &FileBrowserState) -> usize {
        match state.tab {
            BrowserTab::Files => state.entries.len(),
            BrowserTab::Apps => BROWSER_APPS.len(),
        }
    }

    fn refresh_browser_entries(state: &mut FileBrowserState, cwd: &TemplePath, root_dir: &Path) {
        state.msg.clear();
        state.entries.clear();

        if !cwd.components.is_empty() {
            state.entries.push(BrowserEntry {
                name: "..".to_string(),
                kind: BrowserEntryKind::Parent,
            });
        }

        let host = cwd.to_host_path(root_dir);
        let rd = match std::fs::read_dir(&host) {
            Ok(rd) => rd,
            Err(err) => {
                state.msg = format!("read_dir {}: {err}", host.display());
                return;
            }
        };

        let mut found: Vec<BrowserEntry> = Vec::new();
        for entry in rd {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "." || name == ".." {
                continue;
            }

            let kind = match entry.file_type() {
                Ok(ft) if ft.is_dir() => BrowserEntryKind::Dir,
                Ok(_) => BrowserEntryKind::File,
                Err(_) => BrowserEntryKind::File,
            };

            found.push(BrowserEntry { name, kind });
        }

        fn rank(kind: BrowserEntryKind) -> u8 {
            match kind {
                BrowserEntryKind::Parent => 0,
                BrowserEntryKind::Dir => 1,
                BrowserEntryKind::File => 2,
            }
        }

        found.sort_by(|a, b| {
            rank(a.kind)
                .cmp(&rank(b.kind))
                .then_with(|| a.name.cmp(&b.name))
        });
        state.entries.extend(found);

        let len = Self::browser_current_len(state);
        if len == 0 {
            state.selected = 0;
            state.scroll = 0;
            return;
        }
        state.selected = state.selected.min(len - 1);
        state.scroll = state.scroll.min(state.selected);
    }

    fn render_browser(&self, term: &mut Terminal) {
        let Some(state) = self.browser.as_ref() else {
            return;
        };

        term.scroll_view_to_bottom();
        for row in 0..=PROMPT_ROW {
            term.fill_row(row, COLOR_FG, COLOR_BG);
        }

        // Title bar
        term.fill_row(0, COLOR_FG, COLOR_STATUS_BG);
        let mut title = format!("File Browser: {}", self.cwd.display());
        if !state.msg.is_empty() {
            title.push_str("  [");
            title.push_str(&state.msg);
            title.push(']');
        }
        term.write_at(0, 0, COLOR_FG, COLOR_STATUS_BG, &title);

        // Tabs
        let files_bg = if state.tab == BrowserTab::Files {
            COLOR_STATUS_BG
        } else {
            COLOR_BG
        };
        let apps_bg = if state.tab == BrowserTab::Apps {
            COLOR_STATUS_BG
        } else {
            COLOR_BG
        };
        term.write_at(0, 1, COLOR_FG, files_bg, " [Files] ");
        term.write_at(9, 1, COLOR_FG, apps_bg, " [Apps] ");

        // List
        let list_start_row = 2u32;
        let list_rows = Self::browser_list_rows();
        let len = Self::browser_current_len(state);

        for row_idx in 0..list_rows {
            let row = list_start_row + row_idx as u32;
            let idx = state.scroll + row_idx;
            if idx >= len {
                continue;
            }

            let (line, is_selected) = match state.tab {
                BrowserTab::Files => {
                    let entry = &state.entries[idx];
                    let mut name = entry.name.clone();
                    if entry.kind == BrowserEntryKind::Dir {
                        name.push('/');
                    }
                    let prefix = match entry.kind {
                        BrowserEntryKind::Parent => "[..] ",
                        BrowserEntryKind::Dir => "[D]  ",
                        BrowserEntryKind::File => "     ",
                    };
                    (format!("{prefix}{name}"), idx == state.selected)
                }
                BrowserTab::Apps => {
                    let app = &BROWSER_APPS[idx];
                    (
                        format!("{:<8}  {}", app.name, app.hint),
                        idx == state.selected,
                    )
                }
            };

            let (fg, bg) = if is_selected {
                (COLOR_SEL_FG, COLOR_SEL_BG)
            } else {
                (COLOR_FG, COLOR_BG)
            };
            term.write_at(0, row, fg, bg, &line);
        }

        // Bottom bar (prompt row)
        term.fill_row(PROMPT_ROW, COLOR_FG, COLOR_STATUS_BG);
        let hint = match state.tab {
            BrowserTab::Files => "Enter open  Backspace up  Tab apps  Esc back",
            BrowserTab::Apps => "Enter launch  Tab files  Esc back",
        };
        term.write_at(0, PROMPT_ROW, COLOR_FG, COLOR_STATUS_BG, hint);
    }

    fn doc_view_rows() -> usize {
        // Row 0: title bar, row PROMPT_ROW: hint bar.
        PROMPT_ROW.saturating_sub(1).max(1) as usize
    }

    fn find_line_containing(lines: &[Vec<Cell>], needle: &str) -> Option<usize> {
        let needle = needle.trim();
        if needle.is_empty() {
            return None;
        }

        for (i, line) in lines.iter().enumerate() {
            let mut s = String::with_capacity(line.len());
            for cell in line {
                let b = cell.ch;
                s.push(if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    ' '
                });
            }
            if s.contains(needle) {
                return Some(i);
            }
        }

        None
    }

    fn build_doc(
        &self,
        kind: DocKind,
        text: &str,
        bins: &std::collections::BTreeMap<u32, Vec<u8>>,
    ) -> BuiltDoc {
        use fmt::Write as _;

        let mut t = Terminal::new(COLOR_FG, COLOR_BG, Self::doc_view_rows() as u32);
        t.scrollback_max = 10_000;

        let mut meta = DolDocMeta::default();
        match kind {
            DocKind::TempleDoc => self.render_tdoc(text, &mut t),
            DocKind::DolDoc => meta = self.render_doldoc(text, bins, &mut t),
            DocKind::PlainText => {
                for line in text.lines() {
                    let _ = writeln!(&mut t, "{line}");
                }
            }
        }

        let mut lines: Vec<Vec<Cell>> = t.scrollback.into_iter().collect();
        let row_count = t.scroll_rows.min(TERM_ROWS);
        for row in 0..row_count {
            let start = (row * TERM_COLS) as usize;
            let end = start + TERM_COLS as usize;
            lines.push(t.cells[start..end].to_vec());
        }

        while lines
            .last()
            .is_some_and(|line| line.iter().all(|c| c.ch == b' '))
        {
            lines.pop();
        }
        BuiltDoc {
            lines,
            links: meta.links,
            anchors: meta.anchors,
            sprites: meta.sprites,
        }
    }

    fn open_doc_viewer(
        &mut self,
        spec: String,
        kind: DocKind,
        text: &str,
        bins: std::collections::BTreeMap<u32, Vec<u8>>,
        truncated: bool,
        jump: Option<&str>,
        term: &mut Terminal,
    ) {
        self.browser = None;
        let BuiltDoc {
            lines,
            links,
            anchors,
            sprites,
        } = self.build_doc(kind, text, &bins);
        let selected_link = if links.is_empty() { None } else { Some(0) };
        let mut msg = String::new();
        if truncated {
            msg.push_str("truncated");
        }

        let mut scroll = 0usize;
        if let Some(jump) = jump.map(str::trim).filter(|s| !s.is_empty()) {
            scroll = anchors.get(jump).copied().unwrap_or_else(|| {
                Self::find_line_containing(&lines, jump).unwrap_or(0)
            });
            if !msg.is_empty() {
                msg.push_str("; ");
            }
            msg.push_str("jump");
        }
        let max_scroll = lines.len().saturating_sub(Self::doc_view_rows().max(1));
        scroll = scroll.min(max_scroll);

        self.doc_viewer = Some(DocViewerState {
            spec,
            kind,
            lines,
            scroll,
            links,
            selected_link,
            selected_sprite: None,
            anchors,
            sprites,
            bins,
            msg,
        });
        self.render_doc_viewer(term);
    }

    fn render_doc_viewer(&self, term: &mut Terminal) {
        let Some(state) = self.doc_viewer.as_ref() else {
            return;
        };

        term.scroll_view_to_bottom();
        for row in 0..=PROMPT_ROW {
            term.fill_row(row, COLOR_FG, COLOR_BG);
        }

        // Title bar
        term.fill_row(0, COLOR_FG, COLOR_STATUS_BG);
        let kind = match state.kind {
            DocKind::TempleDoc => "TempleDoc",
            DocKind::DolDoc => "DolDoc",
            DocKind::PlainText => "Text",
        };
        let mut title = format!("{kind}: {}", state.spec);
        if !state.msg.is_empty() {
            title.push_str("  [");
            title.push_str(&state.msg);
            title.push(']');
        }
        if let Some(sel) = state.selected_link {
            if !state.links.is_empty() {
                title.push_str(&format!("  (link {}/{})", sel + 1, state.links.len()));
            }
        }
        term.write_at(0, 0, COLOR_FG, COLOR_STATUS_BG, &title);

        // Content
        let content_start_row = 1u32;
        let content_rows = Self::doc_view_rows();
        for row_idx in 0..content_rows {
            let row = content_start_row + row_idx as u32;
            let line_idx = state.scroll + row_idx;
            if line_idx >= state.lines.len() {
                continue;
            }

            let mut line = state.lines[line_idx].clone();
            if let Some(sel) = state.selected_link {
                if let Some(link) = state.links.get(sel) {
                    if link.line == line_idx {
                        for col in link.col_start..link.col_end.min(line.len()) {
                            let cell = &mut line[col];
                            std::mem::swap(&mut cell.fg, &mut cell.bg);
                        }
                    }
                }
            }

            for col in 0..TERM_COLS {
                let idx = term.idx(col, row);
                term.cells[idx] = line.get(col as usize).copied().unwrap_or(Cell {
                    ch: b' ',
                    fg: COLOR_FG,
                    bg: COLOR_BG,
                });
            }
        }

        // Bottom bar
        term.fill_row(PROMPT_ROW, COLOR_FG, COLOR_STATUS_BG);
        let hint = "Esc back  ↑↓ scroll  PgUp/PgDn  Tab link  Enter open";
        term.write_at(0, PROMPT_ROW, COLOR_FG, COLOR_STATUS_BG, hint);
    }

    fn handle_key_doc_viewer(&mut self, key: &Key, term: &mut Terminal) -> bool {
        let mut open_doc: Option<String> = None;
        let mut open_action: Option<String> = None;

        if let Some(state) = self.doc_viewer.as_mut() {
            let content_rows = Self::doc_view_rows().max(1);
            let max_scroll = state.lines.len().saturating_sub(content_rows);

            match key {
                Key::Named(NamedKey::Escape) => {
                    self.doc_viewer = None;
                    self.draw_prompt(term);
                    return true;
                }
                Key::Named(NamedKey::ArrowUp) => {
                    state.scroll = state.scroll.saturating_sub(1);
                }
                Key::Named(NamedKey::ArrowDown) => {
                    state.scroll = (state.scroll + 1).min(max_scroll);
                }
                Key::Named(NamedKey::PageUp) => {
                    state.scroll = state.scroll.saturating_sub(content_rows);
                }
                Key::Named(NamedKey::PageDown) => {
                    state.scroll = (state.scroll + content_rows).min(max_scroll);
                }
                Key::Named(NamedKey::Home) => state.scroll = 0,
                Key::Named(NamedKey::End) => state.scroll = max_scroll,
                Key::Named(NamedKey::Tab) => {
                    if state.links.is_empty() {
                        return false;
                    }
                    state.selected_sprite = None;
                    let next = match state.selected_link {
                        None => 0,
                        Some(i) => (i + 1) % state.links.len(),
                    };
                    state.selected_link = Some(next);
                    if let Some(link) = state.links.get(next) {
                        if link.line < state.scroll {
                            state.scroll = link.line;
                        } else if link.line >= state.scroll + content_rows {
                            state.scroll = link
                                .line
                                .saturating_sub(content_rows.saturating_sub(1))
                                .min(max_scroll);
                        }
                    }
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(sel) = state.selected_link {
                        if let Some(link) = state.links.get(sel) {
                            match &link.target {
                                DocLinkTarget::Doc(spec) => open_doc = Some(spec.clone()),
                                DocLinkTarget::Action(action) => {
                                    open_action = Some(action.clone());
                                }
                            }
                        }
                    } else if let Some(sel) = state.selected_sprite {
                        if let Some(sp) = state.sprites.get(sel) {
                            open_action = sp.action.clone();
                        }
                    }
                }
                _ => return false,
            }
        }

        if let Some(action) = open_action {
            match self.exec_doldoc_action(&action, term) {
                DocActionOutcome::Unsupported => {
                    if let Some(state) = self.doc_viewer.as_mut() {
                        let mut shown = action.replace('\n', "\\n");
                        if shown.len() > 80 {
                            shown.truncate(80);
                            shown.push_str("...");
                        }
                        state.msg = format!("unsupported: {shown}");
                    }
                    self.render_doc_viewer(term);
                }
                DocActionOutcome::KeepDocViewer => {
                    self.render_doc_viewer(term);
                }
                DocActionOutcome::CloseDocViewer => {
                    self.doc_viewer = None;
                    self.draw_prompt(term);
                }
            }
            return true;
        }

        if let Some(target) = open_doc {
            return self.try_show_doc(&target, term);
        }

        self.render_doc_viewer(term);
        true
    }

	    fn exec_doldoc_action(&mut self, action: &str, term: &mut Terminal) -> DocActionOutcome {
	        use fmt::Write as _;

	        let action = action.trim();
		        if let Some(url) = action.strip_prefix("templelinux:browse:") {
		            let url = url.trim();
		            if url.is_empty() {
		                return DocActionOutcome::Unsupported;
		            }

		            let (did_switch, ws_err) = if self.env_bool("TEMPLE_AUTO_LINUX_WS") {
		                let linux_ws = self.env_u32("TEMPLE_WS_LINUX", 2);
		                match self.sway_workspace_number(linux_ws) {
		                    Ok(()) => (true, None),
		                    Err(err) => (false, Some(err)),
		                }
		            } else {
		                (false, None)
		            };

		            match std::process::Command::new("xdg-open").arg(url).spawn() {
		                Ok(child) => {
		                    let mut msg = format!("browse pid {}", child.id());
		                    if let Some(err) = ws_err.as_deref() {
		                        msg.push_str(&format!("; ws: {err}"));
		                    }
		                    if let Some(state) = self.doc_viewer.as_mut() {
		                        state.msg = msg;
		                    }
		                    return DocActionOutcome::KeepDocViewer;
		                }
		                Err(err) => {
		                    if did_switch {
		                        let temple_ws = self.env_u32("TEMPLE_WS_TEMPLE", 1);
		                        let _ = self.sway_workspace_number(temple_ws);
		                    }
		                    if let Some(state) = self.doc_viewer.as_mut() {
		                        let mut msg = format!("browse: xdg-open: {err}");
		                        if let Some(ws_err) = ws_err.as_deref() {
		                            msg.push_str(&format!("; ws: {ws_err}"));
		                        }
		                        state.msg = msg;
		                    }
		                    return DocActionOutcome::KeepDocViewer;
		                }
		            }
		        }

	        if action.starts_with("templelinux:song:") {
	            if let Some(state) = self.doc_viewer.as_mut() {
	                state.msg = "song: unsupported".to_string();
	            }
	            return DocActionOutcome::KeepDocViewer;
	        }

	        fn split_macro_stmts(s: &str) -> Vec<String> {
	            let mut out: Vec<String> = Vec::new();
	            let mut buf = String::new();
	            let mut in_str = false;
	            let mut escaped = false;
            for ch in s.chars() {
                if escaped {
                    buf.push(ch);
                    escaped = false;
                    continue;
                }
                if in_str && ch == '\\' {
                    buf.push(ch);
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    in_str = !in_str;
                    buf.push(ch);
                    continue;
                }
                if ch == ';' && !in_str {
                    let stmt = buf.trim();
                    if !stmt.is_empty() {
                        out.push(stmt.to_string());
                    }
                    buf.clear();
                    continue;
                }
                buf.push(ch);
            }
            let stmt = buf.trim();
            if !stmt.is_empty() {
                out.push(stmt.to_string());
            }
            out
        }

        fn extract_first_string_lit(s: &str) -> Option<String> {
            let start = s.find('"')? + 1;
            let mut out = String::new();
            let mut escaped = false;
            for ch in s[start..].chars() {
                if escaped {
                    out.push(match ch {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    return Some(out);
                }
                out.push(ch);
            }
            Some(out)
        }

        fn encode_attr_value(s: &str) -> String {
            let mut out = String::new();
            for ch in s.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    other => out.push(other),
                }
            }
            out
        }

        fn join_spec(dir_spec: &str, name: &str) -> String {
            if dir_spec.ends_with('/') {
                format!("{dir_spec}{name}")
            } else {
                format!("{dir_spec}/{name}")
            }
        }

        fn spec_parent(spec: &str) -> Option<String> {
            let spec = spec.trim_end_matches('/');
            if spec.starts_with("::/") {
                let rel = spec.trim_start_matches("::/");
                if rel.is_empty() {
                    return None;
                }
                let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
                parts.pop();
                if parts.is_empty() {
                    Some("::/".to_string())
                } else {
                    Some(format!("::/{}", parts.join("/")))
                }
            } else {
                let spec = if spec.is_empty() { "/" } else { spec };
                if spec == "/" {
                    return None;
                }
                let rel = spec.trim_start_matches('/');
                let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
                parts.pop();
                if parts.is_empty() {
                    Some("/".to_string())
                } else {
                    Some(format!("/{}", parts.join("/")))
                }
            }
        }

        fn spec_to_host_dir(spec: &str, temple_root: &Path, cwd: &TemplePath) -> Option<PathBuf> {
            let t = spec.trim();
            if t.starts_with("::/") {
                let root = discover_templeos_root()?;
                let rel = t.trim_start_matches("::/").trim_start_matches('/');
                Some(if rel.is_empty() { root } else { root.join(rel) })
            } else {
                let path = if t.starts_with('/') {
                    TemplePath::root().resolve(t)
                } else {
                    cwd.resolve(t)
                };
                Some(path.to_host_path(temple_root))
            }
        }

        fn build_dir_doc(spec: &str, entries: &[(String, bool)]) -> String {
            let mut out = String::new();
            out.push_str(&format!("$FG,14$Directory: {spec}$FG$\n\n"));
            if let Some(parent) = spec_parent(spec) {
                let parent_esc = encode_attr_value(&parent);
                let lm = format!("Cd(\\\"{parent_esc}\\\");Dir;View;\\n");
                out.push_str(&format!("$MA-X,\"..\",LM=\"{lm}\"$\n\n"));
            }

            for (name, is_dir) in entries {
                if *is_dir {
                    let shown = format!("{name}/");
                    let child = join_spec(spec, name);
                    let child_esc = encode_attr_value(&child);
                    let lm = format!("Cd(\\\"{child_esc}\\\");Dir;View;\\n");
                    out.push_str(&format!("$MA-X,\"{shown}\",LM=\"{lm}\"$\n"));
                } else {
                    let target = join_spec(spec, name);
                    let target_esc = encode_attr_value(&target);
                    out.push_str(&format!("$LK,\"{name}\",A=\"{target_esc}\"$\n"));
                }
            }
            out
        }

        fn extract_include_spec(s: &str) -> Option<String> {
            let s = s.trim_start();
            let idx = s.find("#include")?;
            let mut rest = s[idx + "#include".len()..].trim_start();
            if !rest.starts_with('"') {
                return None;
            }
            rest = &rest[1..];

            let mut out = String::new();
            let mut escaped = false;
            for ch in rest.chars() {
                if escaped {
                    out.push(match ch {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    return Some(out);
                }
                out.push(ch);
            }
            Some(out)
        }

        let action = action.trim();
        if action.is_empty() {
            return DocActionOutcome::Unsupported;
        }

        if let Some(spec) = extract_include_spec(action) {
            let spec = spec.trim();
            if spec.is_empty() {
                return DocActionOutcome::Unsupported;
            }

            let _ = writeln!(term, "Launching: {spec}");
            let spec_owned = spec.to_string();
            self.cmd_tapp(&["hc", spec_owned.as_str()], term);
            return DocActionOutcome::CloseDocViewer;
        }

        // Minimal, safe, deny-by-default `LM="..."` macro support.
        //
        // TempleOS docs often use macros like:
        //   LM="Cd(\"::/Demo/Games\");Dir;View;\n"
        // We interpret a small subset to open a read-only directory listing view.
        let stmts = split_macro_stmts(action);
        let mut cd_spec: Option<String> = None;
        let mut want_dir = false;
        let mut want_view = false;
        let mut want_keymap = false;
        let mut saw_infile = false;

        for stmt in &stmts {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            let s = s.strip_suffix("()").unwrap_or(s).trim();

            if s == "Dir" {
                want_dir = true;
                continue;
            }
            if s == "View" {
                want_view = true;
                continue;
            }
            if s == "KeyMap" {
                want_keymap = true;
                continue;
            }
            if s.starts_with("InFile") {
                saw_infile = true;
                continue;
            }
            if s.starts_with("Cd") {
                if let Some(spec) = extract_first_string_lit(s) {
                    cd_spec = Some(spec);
                } else {
                    cd_spec = Some("::/".to_string());
                }
                continue;
            }
        }

        if want_keymap && self.try_show_doc("::/Doc/KeyMap.DD", term) {
            return DocActionOutcome::KeepDocViewer;
        }

        // If the macro looks like a "directory action", open a directory listing view.
        // Also allow `Cd("...")` alone (often paired with `InFile`) to at least navigate.
        if want_view || want_dir || saw_infile || cd_spec.is_some() {
            let spec = cd_spec
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("::/");

            let Some(host_dir) = spec_to_host_dir(spec, &self.root_dir, &self.cwd) else {
                let _ = writeln!(term, "DolDoc action: TempleOS tree not found.");
                return DocActionOutcome::CloseDocViewer;
            };
            if !host_dir.is_dir() {
                let _ = writeln!(term, "DolDoc action: not a directory: {spec}");
                return DocActionOutcome::CloseDocViewer;
            }

            let mut entries: Vec<(String, bool)> = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&host_dir) {
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_dir = entry.file_type().ok().is_some_and(|ft| ft.is_dir());
                    entries.push((name, is_dir));
                }
            }
            entries.sort_by(|(a_name, a_dir), (b_name, b_dir)| match b_dir.cmp(a_dir) {
                std::cmp::Ordering::Equal => a_name.cmp(b_name),
                other => other,
            });

            let doc = build_dir_doc(spec, &entries);
            let bins: std::collections::BTreeMap<u32, Vec<u8>> = std::collections::BTreeMap::new();
            self.open_doc_viewer(spec.to_string(), DocKind::DolDoc, &doc, bins, false, None, term);
            return DocActionOutcome::KeepDocViewer;
        }

        DocActionOutcome::Unsupported
    }

    fn doc_wheel(&mut self, delta_lines: isize, term: &mut Terminal) -> bool {
        let Some(state) = self.doc_viewer.as_mut() else {
            return false;
        };

        let content_rows = Self::doc_view_rows().max(1);
        let max_scroll = state.lines.len().saturating_sub(content_rows);

        if delta_lines > 0 {
            state.scroll = state.scroll.saturating_sub(delta_lines as usize);
        } else if delta_lines < 0 {
            state.scroll = (state.scroll + (-delta_lines) as usize).min(max_scroll);
        } else {
            return false;
        }

        self.render_doc_viewer(term);
        true
    }

    fn doc_click_cell(&mut self, col: u32, row: u32, term: &mut Terminal) -> Option<usize> {
        let content_start_row = 1u32;
        if row < content_start_row || row >= PROMPT_ROW {
            return None;
        }

        let hit = {
            let Some(state) = self.doc_viewer.as_mut() else {
                return None;
            };

            let row_idx = (row - content_start_row) as usize;
            let line_idx = state.scroll + row_idx;
            let col = col as usize;

            let hit_link = state
                .links
                .iter()
                .enumerate()
                .find(|(_, link)| {
                    link.line == line_idx && col >= link.col_start && col < link.col_end
                })
                .map(|(i, _)| i);

            if let Some(hit) = hit_link {
                state.selected_link = Some(hit);
                state.selected_sprite = None;
                Some(hit)
            } else {
                let hit_sprite = state
                    .sprites
                    .iter()
                    .enumerate()
                    .find(|(_, sp)| {
                        let line = line_idx as i32;
                        let col = col as i32;
                        let line_ok = line >= sp.bbox_line0 && line < sp.bbox_line1;
                        let col_ok = col >= sp.bbox_col0 && col < sp.bbox_col1;
                        line_ok && col_ok
                    })
                    .map(|(i, _)| i)?;

                state.selected_link = None;
                state.selected_sprite = Some(hit_sprite);
                Some(state.links.len().saturating_add(hit_sprite))
            }
        };

        if hit.is_some() {
            self.render_doc_viewer(term);
        }
        hit
    }

    fn handle_key(&mut self, key: &Key, term: &mut Terminal) -> bool {
        if self.doc_viewer.is_some() {
            return self.handle_key_doc_viewer(key, term);
        }
        if self.browser.is_some() {
            return self.handle_key_browser(key, term);
        }

        if matches!(key, Key::Named(NamedKey::F2)) {
            self.cmd_apps(&[], term);
            return true;
        }

        let mut changed = false;

        match key {
            Key::Named(NamedKey::Enter) => {
                changed = true;
                self.submit_line(term);
            }
            Key::Named(NamedKey::Backspace) => {
                changed = true;
                self.backspace();
            }
            Key::Named(NamedKey::Delete) => {
                changed = true;
                self.delete();
            }
            Key::Named(NamedKey::ArrowLeft) => {
                changed = true;
                self.cursor = self.cursor.saturating_sub(1);
            }
            Key::Named(NamedKey::ArrowRight) => {
                changed = true;
                self.cursor = (self.cursor + 1).min(self.input.len());
            }
            Key::Named(NamedKey::Home) => {
                changed = true;
                self.cursor = 0;
            }
            Key::Named(NamedKey::End) => {
                changed = true;
                self.cursor = self.input.len();
            }
            Key::Named(NamedKey::ArrowUp) => {
                changed = true;
                self.history_prev();
            }
            Key::Named(NamedKey::ArrowDown) => {
                changed = true;
                self.history_next();
            }
            Key::Named(NamedKey::Space) => {
                changed = true;
                self.insert_char(' ');
            }
            Key::Character(s) => {
                let mut inserted = false;
                for ch in s.chars() {
                    if ch.is_ascii_graphic() || ch == ' ' {
                        self.insert_char(ch);
                        inserted = true;
                    }
                }
                changed |= inserted;
            }
            _ => {}
        }

        if changed {
            self.draw_prompt(term);
        }
        changed
    }

    fn handle_key_browser(&mut self, key: &Key, term: &mut Terminal) -> bool {
        enum Action {
            None,
            Exit,
            Navigate(TemplePath),
            RunCommand(String),
            Redraw,
        }

        let mut action = Action::None;
        let list_rows = Self::browser_list_rows();

        if let Some(state) = self.browser.as_mut() {
            let len = Self::browser_current_len(state);

            match key {
                Key::Named(NamedKey::Escape) => action = Action::Exit,
                Key::Named(NamedKey::Tab) => {
                    state.tab = match state.tab {
                        BrowserTab::Files => BrowserTab::Apps,
                        BrowserTab::Apps => BrowserTab::Files,
                    };
                    let new_len = Self::browser_current_len(state);
                    if new_len == 0 {
                        state.selected = 0;
                        state.scroll = 0;
                    } else {
                        state.selected = state.selected.min(new_len - 1);
                        if state.selected < state.scroll {
                            state.scroll = state.selected;
                        }
                    }
                    action = Action::Redraw;
                }
                Key::Named(NamedKey::ArrowUp) => {
                    if len > 0 {
                        state.selected = state.selected.saturating_sub(1);
                        if state.selected < state.scroll {
                            state.scroll = state.selected;
                        }
                    }
                    action = Action::Redraw;
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if len > 0 && state.selected + 1 < len {
                        state.selected += 1;
                        if state.selected >= state.scroll + list_rows {
                            state.scroll = state.selected + 1 - list_rows;
                        }
                    }
                    action = Action::Redraw;
                }
                Key::Named(NamedKey::Home) => {
                    state.selected = 0;
                    state.scroll = 0;
                    action = Action::Redraw;
                }
                Key::Named(NamedKey::End) => {
                    if len > 0 {
                        state.selected = len - 1;
                        state.scroll = state.selected.saturating_sub(list_rows.saturating_sub(1));
                    }
                    action = Action::Redraw;
                }
                Key::Named(NamedKey::PageUp) => {
                    if len > 0 {
                        state.selected = state.selected.saturating_sub(list_rows.max(1));
                        if state.selected < state.scroll {
                            state.scroll = state.selected;
                        }
                    }
                    action = Action::Redraw;
                }
                Key::Named(NamedKey::PageDown) => {
                    if len > 0 {
                        state.selected =
                            (state.selected + list_rows.max(1)).min(len.saturating_sub(1));
                        if state.selected >= state.scroll + list_rows {
                            state.scroll = state.selected + 1 - list_rows;
                        }
                    }
                    action = Action::Redraw;
                }
                Key::Named(NamedKey::Backspace) => {
                    if state.tab == BrowserTab::Files {
                        action = Action::Navigate(self.cwd.resolve(".."));
                    }
                }
                Key::Named(NamedKey::Enter) => match state.tab {
                    BrowserTab::Files => {
                        if state.selected < state.entries.len() {
                            let entry = state.entries[state.selected].clone();
                            match entry.kind {
                                BrowserEntryKind::Parent => {
                                    action = Action::Navigate(self.cwd.resolve(".."));
                                }
                                BrowserEntryKind::Dir => {
                                    action = Action::Navigate(self.cwd.resolve(&entry.name));
                                }
                                BrowserEntryKind::File => {
                                    let path = self.cwd.resolve(&entry.name);
                                    action = Action::RunCommand(format!("edit {}", path.display()));
                                }
                            }
                        }
                    }
                    BrowserTab::Apps => {
                        if state.selected < BROWSER_APPS.len() {
                            action = Action::RunCommand(
                                BROWSER_APPS[state.selected].command.to_string(),
                            );
                        }
                    }
                },
                _ => {}
            }
        }

        match action {
            Action::None => false,
            Action::Exit => {
                self.browser = None;
                self.draw_prompt(term);
                true
            }
            Action::Navigate(path) => {
                self.cwd = path;
                if let Some(state) = self.browser.as_mut() {
                    Self::refresh_browser_entries(state, &self.cwd, &self.root_dir);
                }
                self.render_browser(term);
                true
            }
            Action::RunCommand(cmd) => {
                self.browser = None;
                self.exec_line(&cmd, term);
                self.draw_prompt(term);
                true
            }
            Action::Redraw => {
                self.render_browser(term);
                true
            }
        }
    }

    fn browser_wheel(&mut self, delta_lines: isize, term: &mut Terminal) -> bool {
        let Some(state) = self.browser.as_mut() else {
            return false;
        };

        let len = Self::browser_current_len(state);
        if len == 0 {
            return false;
        }

        let list_rows = Self::browser_list_rows().max(1);
        if delta_lines > 0 {
            state.selected = state.selected.saturating_sub(delta_lines as usize);
        } else if delta_lines < 0 {
            let down = (-delta_lines) as usize;
            state.selected = (state.selected + down).min(len.saturating_sub(1));
        }

        if state.selected < state.scroll {
            state.scroll = state.selected;
        } else if state.selected >= state.scroll + list_rows {
            state.scroll = state.selected + 1 - list_rows;
        }

        self.render_browser(term);
        true
    }

    fn browser_click_row(&mut self, row: u32, term: &mut Terminal) -> Option<usize> {
        let Some(state) = self.browser.as_mut() else {
            return None;
        };

        let list_start_row = 2u32;
        if row < list_start_row || row >= PROMPT_ROW {
            return None;
        }

        let row_idx = (row - list_start_row) as usize;
        let idx = state.scroll + row_idx;
        let len = Self::browser_current_len(state);
        if idx >= len {
            return None;
        }

        state.selected = idx;
        self.render_browser(term);
        Some(idx)
    }

    fn paste_text(&mut self, text: &str, term: &mut Terminal) -> bool {
        const MAX_PASTE_CHARS: usize = 4096;

        let mut changed = false;
        for ch in text.chars().take(MAX_PASTE_CHARS) {
            if ch.is_ascii_graphic() || ch == ' ' {
                self.insert_char(ch);
                changed = true;
            }
        }

        if changed {
            self.draw_prompt(term);
        }
        changed
    }

    fn insert_char(&mut self, ch: char) {
        if !ch.is_ascii() {
            return;
        }
        self.cursor = self.cursor.min(self.input.len());
        self.input.insert(self.cursor, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 || self.input.is_empty() {
            return;
        }
        self.cursor = self.cursor.min(self.input.len());
        self.cursor -= 1;
        self.input.remove(self.cursor);
    }

    fn delete(&mut self) {
        self.cursor = self.cursor.min(self.input.len());
        if self.cursor >= self.input.len() {
            return;
        }
        self.input.remove(self.cursor);
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_pos {
            None => {
                self.history_draft = self.input.clone();
                self.history_pos = Some(self.history.len() - 1);
            }
            Some(0) => {}
            Some(i) => self.history_pos = Some(i - 1),
        }
        if let Some(i) = self.history_pos {
            self.input = self.history[i].clone();
            self.cursor = self.input.len();
        }
    }

    fn history_next(&mut self) {
        let Some(i) = self.history_pos else {
            return;
        };
        let next = i + 1;
        if next >= self.history.len() {
            self.history_pos = None;
            self.input = std::mem::take(&mut self.history_draft);
            self.cursor = self.input.len();
            return;
        }
        self.history_pos = Some(next);
        self.input = self.history[next].clone();
        self.cursor = self.input.len();
    }

    fn submit_line(&mut self, term: &mut Terminal) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            self.reset_input();
            return;
        }

        let prompt = self.prompt();
        use fmt::Write as _;
        let _ = writeln!(term, "{}{}", prompt, line);
        self.exec_line(&line, term);

        if self.history.last().map(|s| s.as_str()) != Some(line.as_str()) {
            self.history.push(line);
        }
        self.save_history();
        self.reset_input();
    }

    fn reset_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_pos = None;
        self.history_draft.clear();
    }

    fn exec_line(&mut self, line: &str, term: &mut Terminal) {
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();

	        match cmd {
	            "help" => self.cmd_help(&args, term),
	            "clear" => {
	                term.clear_output();
	            }
            "pwd" => self.cmd_pwd(term),
            "cd" => self.cmd_cd(&args, term),
            "ls" => self.cmd_ls(&args, term),
            "cat" => self.cmd_cat(&args, term),
            "cp" => self.cmd_cp(&args, term),
            "mv" => self.cmd_mv(&args, term),
            "rm" => self.cmd_rm(&args, term),
            "mkdir" => self.cmd_mkdir(&args, term),
            "touch" => self.cmd_touch(&args, term),
            "grep" => self.cmd_grep(&args, term),
            "find" => self.cmd_find(&args, term),
            "head" => self.cmd_head(&args, term),
            "tail" => self.cmd_tail(&args, term),
            "wc" => self.cmd_wc(&args, term),
            "more" => self.cmd_more(&args, term),
            "less" => self.cmd_more(&args, term),
            "clip" => self.cmd_clip(line, &args, term),
            "env" => self.cmd_env(&args, term),
            "set" => self.cmd_set(&args, term),
	            "ws" => self.cmd_ws(&args, term),
	            "run" => self.cmd_run(&args, term),
	            "hc" | "holyc" => {
	                let mut tapp_args: Vec<&str> = Vec::with_capacity(args.len() + 1);
	                tapp_args.push("hc");
	                tapp_args.extend(args.iter().copied());
	                self.cmd_tapp(&tapp_args, term);
	            }
	            "tapp" => self.cmd_tapp(&args, term),
	            "edit" => self.cmd_edit(&args, term),
	            "files" => self.cmd_files(&args, term),
	            "fm" => self.cmd_files(&args, term),
	            "apps" | "launcher" => self.cmd_apps(&args, term),
	            "menu" => self.cmd_menu(&args, term),
            "open" => self.cmd_open(&args, term),
            "browse" => self.cmd_browse(&args, term),
            "" => {}
            "screenshot" | "shot" => self.cmd_screenshot(&args, term),
            "shutdown" | "exit" => self.cmd_shutdown(&args, term),
            other => {
                use fmt::Write as _;
                let _ = writeln!(term, "Unknown command: {other}. Type 'help'.");
            }
        }
    }

    fn cmd_help(&mut self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;
        if args.is_empty() {
            let _ = writeln!(term, "Commands:");
            let _ = writeln!(term, "  help [cmd]   Show help");
            let _ = writeln!(term, "  clear        Clear output");
            let _ = writeln!(term, "  ls [path]    List directory");
            let _ = writeln!(term, "  cd [path]    Change directory");
            let _ = writeln!(term, "  pwd          Print working directory");
            let _ = writeln!(term, "  cat <path>   Print file");
            let _ = writeln!(term, "  cp <a> <b>   Copy file");
            let _ = writeln!(term, "  mv <a> <b>   Move/rename");
            let _ = writeln!(term, "  rm <path>    Remove file");
            let _ = writeln!(term, "  mkdir <path> Make directory");
            let _ = writeln!(term, "  touch <path> Create empty file");
            let _ = writeln!(term, "  grep <s> <p> Search file");
            let _ = writeln!(term, "  find [p] [s] Find files (substring)");
            let _ = writeln!(term, "  head <path>  First lines");
            let _ = writeln!(term, "  tail <path>  Last lines");
            let _ = writeln!(term, "  wc <path>    Count lines/words/bytes");
            let _ = writeln!(term, "  more <path>  Pager");
            let _ = writeln!(term, "  less <path>  Pager");
            let _ = writeln!(term, "  clip <cmd>   Host clipboard (get/set)");
            let _ = writeln!(term, "  env          Show TempleShell vars");
            let _ = writeln!(term, "  set <k=v>    Set TempleShell var");
            let _ = writeln!(term, "  open <path>  Open file with xdg-open");
            let _ = writeln!(term, "  browse <url> Open URL with xdg-open");
            let _ = writeln!(term, "  screenshot   Save a PNG screenshot");
            let _ = writeln!(term, "  run <cmd>    Launch a Linux command");
            let _ = writeln!(term, "  shutdown     Exit TempleShell");
            let _ = writeln!(term, "  ws <target>  Switch workspace (sway IPC)");
	            let _ = writeln!(
	                term,
	                "  tapp <cmd>   Launch a Temple app / TempleOS program"
	            );
	            let _ = writeln!(term, "  hc [file]    Run HolyC (alias for 'tapp hc')");
	            let _ = writeln!(term, "  edit <path>  Open Temple editor");
	            let _ = writeln!(term, "  files [dir]  File browser (UI)");
	            let _ = writeln!(term, "  apps         App launcher (UI)");
	            let _ = writeln!(term, "  menu         TempleOS menu (icons)");
	            let _ = writeln!(term, "");
	            let _ = writeln!(term, "Window manager:");
            let _ = writeln!(term, "  Click window to focus; drag title to move");
            let _ = writeln!(term, "  Ctrl+W closes focused; Alt+Tab cycles focus");
	            let _ = writeln!(term, "");
	            let _ = writeln!(term, "Hotkeys:");
	            let _ = writeln!(term, "  F2 opens the app launcher");
            return;
        }

        if let Some(topic) = args.first().copied() {
            if self.try_show_doc(topic, term) {
                return;
            }
        }

        match args[0] {
            "help" => {
                let _ = writeln!(term, "help [cmd]");
            }
            "clear" => {
                let _ = writeln!(term, "clear");
            }
            "ls" => {
                let _ = writeln!(term, "ls [path]");
            }
            "cd" => {
                let _ = writeln!(term, "cd [path]");
            }
            "pwd" => {
                let _ = writeln!(term, "pwd");
            }
            "cat" => {
                let _ = writeln!(term, "cat <path>");
            }
            "cp" => {
                let _ = writeln!(term, "cp <src> <dst>");
            }
            "mv" => {
                let _ = writeln!(term, "mv <src> <dst>");
            }
            "rm" => {
                let _ = writeln!(term, "rm <path>");
                let _ = writeln!(term, "rm -r <path>");
            }
            "mkdir" => {
                let _ = writeln!(term, "mkdir <path>");
            }
            "touch" => {
                let _ = writeln!(term, "touch <path>");
            }
            "grep" => {
                let _ = writeln!(term, "grep <needle> <path>");
            }
            "find" => {
                let _ = writeln!(term, "find [path] [name-substring]");
            }
            "head" => {
                let _ = writeln!(term, "head [-n N] <path>");
            }
            "tail" => {
                let _ = writeln!(term, "tail [-n N] <path>");
            }
            "wc" => {
                let _ = writeln!(term, "wc <path>");
            }
            "more" | "less" => {
                let _ = writeln!(term, "more <path>");
                let _ = writeln!(term, "less <path>");
                let _ = writeln!(term, "hotkeys: PgUp/PgDn and mouse wheel");
            }
            "clip" => {
                let _ = writeln!(term, "clip get");
                let _ = writeln!(term, "clip set <text...>");
                let _ = writeln!(term, "clip clear");
                let _ = writeln!(term, "hotkey: Ctrl+V (or Shift+Ins) to paste");
            }
            "env" => {
                let _ = writeln!(term, "env");
                let _ = writeln!(term, "env <name>");
            }
            "set" => {
                let _ = writeln!(term, "set <name>=<value>");
                let _ = writeln!(term, "set <name> <value...>");
                let _ = writeln!(term, "set <name> (clears)");
            }
            "open" => {
                let _ = writeln!(term, "open <path>");
            }
            "browse" => {
                let _ = writeln!(term, "browse <url>");
            }
            "screenshot" | "shot" => {
                let _ = writeln!(term, "screenshot [path.png]");
                let _ = writeln!(term, "shot [path.png]");
                let _ = writeln!(term, "default: /Home/screenshot-<nanos>.png");
            }
            "run" => {
                let _ = writeln!(term, "run <cmd> [args...]");
            }
            "shutdown" | "exit" => {
                let _ = writeln!(term, "shutdown");
                let _ = writeln!(term, "exit");
            }
            "ws" => {
                let _ = writeln!(term, "ws <temple|linux|num>");
                let _ = writeln!(
                    term,
                    "env: TEMPLE_WS_TEMPLE=1 TEMPLE_WS_LINUX=2 TEMPLE_AUTO_LINUX_WS=1"
                );
	            }
	            "tapp" => {
	                let _ = writeln!(term, "tapp <cmd> [args...]");
                let _ = writeln!(term, "tapp list");
                let _ = writeln!(term, "tapp tree");
                let _ = writeln!(term, "tapp search <text>");
                let _ = writeln!(term, "tapp run <alias|path>");
                let _ = writeln!(term, "tapp kill");
                let _ = writeln!(term, "tapp restart");
                let _ = writeln!(term, "tapp demo");
                let _ = writeln!(term, "tapp hc [file.hc]");
                let _ = writeln!(term, "tapp paint");
                let _ = writeln!(term, "tapp linuxbridge");
                let _ = writeln!(term, "tapp timeclock");
                let _ = writeln!(term, "tapp sounddemo");
                let _ = writeln!(term, "tapp logic");
                let _ = writeln!(term, "tapp keepaway");
                let _ = writeln!(term, "tapp wallpaperctrl");
                let _ = writeln!(term, "tapp wallpaperfish");
	                let _ = writeln!(term, "tapp edit <file>");
	            }
	            "hc" | "holyc" => {
	                let _ = writeln!(term, "hc [file.hc]");
	                let _ = writeln!(term, "alias for: tapp hc [file.hc]");
	            }
	            "edit" => {
	                let _ = writeln!(term, "edit <path>");
	                let _ = writeln!(term, "hotkeys: Ctrl+S save  Ctrl+Q quit  F5 run/check  F1 help");
	            }
	            "files" | "fm" => {
	                let _ = writeln!(term, "files [dir]");
	                let _ = writeln!(term, "fm [dir]");
                let _ = writeln!(
                    term,
                    "hotkeys: arrows/PgUp/PgDn navigate  Enter open  Tab apps/files  Esc back"
                );
            }
	            "apps" | "launcher" => {
	                let _ = writeln!(term, "apps");
	                let _ = writeln!(term, "launcher");
	                let _ = writeln!(term, "opens the launcher (Apps tab)");
	                let _ = writeln!(term, "hotkey: F2");
	            }
	            "menu" => {
	                let _ = writeln!(term, "menu");
	                let _ = writeln!(term, "opens TempleOS PersonalMenu.DD (game icons, etc)");
	                let _ = writeln!(
	                    term,
	                    "needs the TempleOS tree (set TEMPLEOS_ROOT or keep third_party/TempleOS nearby)"
	                );
	            }
            other => {
                let _ = writeln!(term, "No help for: {other}");
            }
        }
    }

    fn cmd_files(&mut self, args: &[&str], term: &mut Terminal) {
        if let Some(target) = args.first().copied() {
            let path = self.cwd.resolve(target);
            let host = path.to_host_path(&self.root_dir);
            if host.is_dir() {
                self.cwd = path;
            }
        }

        let mut state = FileBrowserState::new();
        Self::refresh_browser_entries(&mut state, &self.cwd, &self.root_dir);
        self.browser = Some(state);
        self.render_browser(term);
    }

    fn cmd_apps(&mut self, args: &[&str], term: &mut Terminal) {
        if let Some(target) = args.first().copied() {
            let path = self.cwd.resolve(target);
            let host = path.to_host_path(&self.root_dir);
            if host.is_dir() {
                self.cwd = path;
            }
        }

        let mut state = FileBrowserState::new();
        state.tab = BrowserTab::Apps;
        Self::refresh_browser_entries(&mut state, &self.cwd, &self.root_dir);
        self.browser = Some(state);
        self.render_browser(term);
    }

    fn cmd_menu(&mut self, _args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let opened = if self.try_show_doc("/Home/PersonalMenu.DD", term) {
            true
        } else {
            self.try_show_doc("::/PersonalMenu.DD", term)
        };
        if opened {
            // Jump near the icon area so it feels like a "desktop" right away.
            if let Some(state) = self.doc_viewer.as_mut() {
                let jump = Self::find_line_containing(&state.lines, "Fun Games")
                    .or_else(|| Self::find_line_containing(&state.lines, "Nongames"));
                if let Some(scroll) = jump {
                    let max_scroll = state.lines.len().saturating_sub(Self::doc_view_rows().max(1));
                    state.scroll = scroll.min(max_scroll);
                    state.msg = "jump".to_string();
                }
            }
            self.render_doc_viewer(term);
            return;
        }

        let _ = writeln!(term, "menu: PersonalMenu.DD not found.");
        let _ = writeln!(
            term,
            "Hint: set TEMPLEOS_ROOT, or copy ::/PersonalMenu.DD to /Home/PersonalMenu.DD."
        );
    }

    fn resolve_manual_node_target(&self, sym: &str) -> Option<(String, String)> {
        let sym = sym.trim();
        if sym.is_empty() {
            return None;
        }

        let templeos_root = discover_templeos_root()?;

        let mut best: Option<(i32, PathBuf, String)> = None;
        let search_dirs = ["Kernel", "Adam", "Compiler", "Apps", "Demo"];
        let fn_needle = format!("{sym}(");
        let class_needle = format!("class {sym}");
        let ext_class_needle = format!("extern class {sym}");
        let define_needle = format!("#define {sym}");

        for dir in search_dirs {
            let root = templeos_root.join(dir);
            if !root.is_dir() {
                continue;
            }

            let mut stack = vec![root];
            while let Some(path) = stack.pop() {
                let entries = match std::fs::read_dir(&path) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        stack.push(p);
                        continue;
                    }
                    let ext = p
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if !matches!(ext.as_str(), "hc" | "hh" | "dd" | "td" | "txt") {
                        continue;
                    }
                    let meta_len = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
                    if meta_len > 2 * 1024 * 1024 {
                        continue;
                    }

                    let Ok(buf) = std::fs::read(&p) else {
                        continue;
                    };
                    let cutoff = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                    let text = String::from_utf8_lossy(&buf[..cutoff]);
                    if !text.contains(sym) {
                        continue;
                    }
                    let lines: Vec<&str> = text.lines().collect();
                    for (i, line) in lines.iter().enumerate() {
                        let lt = line.trim_start();
                        if lt.is_empty() {
                            continue;
                        }

                        let mut match_score = None::<i32>;

                        if let Some(pos) = line.find(&fn_needle) {
                            if pos > 0 {
                                let prev = line.as_bytes()[pos.saturating_sub(1)];
                                if prev.is_ascii_alphanumeric() || prev == b'_' {
                                    continue;
                                }
                            }

                            let mut is_def = false;
                            if line.contains('{') {
                                is_def = true;
                            } else {
                                let mut j = i + 1;
                                while j < lines.len() && lines[j].trim().is_empty() {
                                    j += 1;
                                }
                                if j < lines.len() && lines[j].trim_start().starts_with('{') {
                                    is_def = true;
                                }
                            }

                            let mut score = 0i32;
                            if is_def {
                                score -= 100;
                            }
                            match_score = Some(score);
                        } else if lt.contains(&ext_class_needle) || lt.contains(&class_needle) {
                            match_score = Some(-30);
                        } else if lt.contains(&define_needle) {
                            match_score = Some(-20);
                        } else if lt.contains(sym) {
                            // Last resort: some "manual node" links refer to constants/types without
                            // an obvious signature pattern.
                            match_score = Some(100);
                        }

                        let Some(mut score) = match_score else {
                            continue;
                        };

                        match ext.as_str() {
                            "hc" => {}
                            "hh" => score += 50,
                            _ => score += 100,
                        }
                        if lt.starts_with("//") {
                            score += 200;
                        }
                        if p.to_string_lossy().contains("/Kernel/") {
                            score -= 5;
                        }

                        let better = best
                            .as_ref()
                            .map(|(best_score, _, _)| score < *best_score)
                            .unwrap_or(true);
                        if better {
                            best = Some((score, p.clone(), (*line).to_string()));
                        }
                    }
                }
            }
        }

        let (_score, path, line) = best?;
        let rel = path.strip_prefix(&templeos_root).ok()?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        let spec = format!("::/{rel}");

        let line = line.trim();
        let jump = if let Some(pos) = line.find(sym) {
            let start = pos.saturating_sub(20);
            let end = (pos + sym.len() + 40).min(line.len());
            line[start..end].trim().to_string()
        } else {
            fn_needle
        };

        Some((spec, jump))
    }

    fn normalize_doc_target(&self, topic: &str) -> (String, Option<String>) {
        let t = topic.trim();
        if t.is_empty() {
            return (String::new(), None);
        }

        let Some((prefix, rest)) = t.split_once(':') else {
            return (t.to_string(), None);
        };
        let prefix = prefix.trim();
        let rest = rest.trim();

        let is_prefix = prefix.len() == 2 && prefix.chars().all(|c| c.is_ascii_uppercase());
        if !is_prefix {
            return (t.to_string(), None);
        }

        match prefix {
            // File links: allow comma-separated "jump to" hints.
            "FF" | "FI" => {
                let (mut file, jump) = rest
                    .split_once(',')
                    .map(|(a, b)| (a.trim(), Some(b.trim().to_string())))
                    .unwrap_or((rest, None));

                if file.starts_with("~/") {
                    file = file.trim_start_matches("~/");
                    return (format!("::/{file}"), jump);
                }
                if file.starts_with("::/") {
                    return (file.to_string(), jump);
                }
                if file.starts_with("::") {
                    let file = file.trim_start_matches("::").trim_start_matches('/');
                    return (format!("/{file}"), jump);
                }
                if file.starts_with('/') {
                    return (format!("::{file}"), jump);
                }
                if file.contains('/') {
                    return (format!("::/{file}"), jump);
                }
                (t.to_string(), None)
            }
            // Manual-node style links: best-effort jump to the symbol definition.
            "MN" | "HI" => self
                .resolve_manual_node_target(rest)
                .map(|(spec, jump)| (spec, Some(jump)))
                .unwrap_or((t.to_string(), None)),
            _ => (t.to_string(), None),
        }
    }

    fn try_show_doc(&mut self, topic: &str, term: &mut Terminal) -> bool {
        const MAX_BYTES: u64 = 2 * 1024 * 1024;

        fn parse_doc_blob(buf: &[u8]) -> (String, std::collections::BTreeMap<u32, Vec<u8>>) {
            fn read_u32_le(buf: &[u8], off: usize) -> Option<u32> {
                let b = buf.get(off..off + 4)?;
                Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }

            fn extract_expected_bins(text: &str) -> std::collections::BTreeSet<u32> {
                let mut out = std::collections::BTreeSet::new();
                let bytes = text.as_bytes();
                let mut i = 0usize;
                while i + 3 <= bytes.len() {
                    let b0 = bytes[i];
                    let b1 = bytes[i + 1];
                    if (b0 == b'B' || b0 == b'b') && (b1 == b'I' || b1 == b'i') && bytes[i + 2] == b'='
                    {
                        let mut j = i + 3;
                        let mut v: u32 = 0;
                        let mut any = false;
                        while j < bytes.len() && bytes[j].is_ascii_digit() {
                            any = true;
                            v = v.saturating_mul(10).saturating_add((bytes[j] - b'0') as u32);
                            j += 1;
                        }
                        if any && v != 0 {
                            out.insert(v);
                        }
                        i = j;
                        continue;
                    }
                    i += 1;
                }
                out
            }

            fn parse_doc_bins(
                bin_blob: &[u8],
                expected_bins: &std::collections::BTreeSet<u32>,
            ) -> std::collections::BTreeMap<u32, Vec<u8>> {
                const BIN_HEADER_LEN: usize = 16;
                const MAX_DATA_LEN: usize = 256 * 1024;

                fn read_i32_le(buf: &[u8], off: usize) -> Option<i32> {
                    let b = buf.get(off..off + 4)?;
                    Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                }

                fn ceil_to_multiple(v: i32, step: i32) -> i32 {
                    if step <= 0 {
                        return v;
                    }
                    ((v + step - 1) / step) * step
                }

                fn repair_sprite_blob(prefix: &[u8]) -> Option<Vec<u8>> {
                    if prefix.is_empty() {
                        return None;
                    }

                    // --- Happy path: parse a sprite starting at offset 0. -------------------
                    //
                    // We intentionally accept trailing junk after the first `SPT_END` and truncate,
                    // because some vendored `.DD` files include extra (non-zero) padding after the
                    // sprite. TempleOS walks sprites until `SPT_END` and ignores any tail bytes.
                    if let Some(end) = temple_rt::sprite::sprite_parse_end_at_start(prefix) {
                        if end > 1 {
                            return Some(prefix[..end].to_vec());
                        }
                    }

                    // --- Small common corruptions / quirks ----------------------------------

                    // 1) Off-by-one: trailing `SPT_END` becomes `0xff`. Fix and retry.
                    if prefix.last() == Some(&0xff) {
                        let mut fixed = prefix.to_vec();
                        if let Some(last) = fixed.last_mut() {
                            *last = 0;
                        }
                        if let Some(end) = temple_rt::sprite::sprite_parse_end_at_start(&fixed) {
                            if end > 1 {
                                fixed.truncate(end);
                                return Some(fixed);
                            }
                        }
                    }

                    // 2) Missing trailing `SPT_END`: append it and retry.
                    {
                        let mut fixed = prefix.to_vec();
                        fixed.push(0);
                        if let Some(end) = temple_rt::sprite::sprite_parse_end_at_start(&fixed) {
                            if end > 1 {
                                fixed.truncate(end);
                                return Some(fixed);
                            }
                        }
                    }

                    // 2b) Missing/garbled `SPT_END` with trailing garbage: truncate at the last
                    // successfully-parsed element boundary and append `SPT_END`.
                    if let Some(last) = temple_rt::sprite::sprite_parse_last_good_prefix_len_at_start(prefix) {
                        if last > 1 && last <= prefix.len() {
                            let mut fixed = prefix[..last].to_vec();
                            fixed.push(0);
                            if temple_rt::sprite::sprite_parse_end_at_start(&fixed)
                                == Some(fixed.len())
                            {
                                return Some(fixed);
                            }
                        }
                    }

                    // 3) Bitmap-only sprite missing `SPT_END`, with `0xff`/0 padding after the bitmap.
                    // This shows up in some PersonalMenu icons.
                    if prefix.first().copied().unwrap_or(0) & 0x7f == 23 {
                        // SPT_BITMAP: type + x + y + w + h + data[stride*h]
                        if prefix.len() >= 1 + 4 * 4 {
                            let w = read_i32_le(prefix, 1 + 8)?;
                            let h = read_i32_le(prefix, 1 + 12)?;
                            if w > 0 && h > 0 && w <= 2048 && h <= 2048 {
                                let stride = ceil_to_multiple(w, 8) as usize;
                                let data_len = stride.saturating_mul(h as usize);
                                let elem_len = 1usize + 4 * 4 + data_len;
                                if elem_len <= prefix.len()
                                    && prefix[elem_len..]
                                        .iter()
                                        .all(|&b| b == 0 || b == 0xff)
                                {
                                    let mut fixed = prefix[..elem_len].to_vec();
                                    fixed.push(0); // SPT_END
                                    return Some(fixed);
                                }
                            }
                        }
                    }

                    // 4) Missing `SPT_BITMAP` type byte: the blob starts at `x1` instead of `type`.
                    //
                    // Layout without the type byte:
                    //   x1(i32), y1(i32), w(i32), h(i32), data[stride*h], [optional SPT_END]
                    if prefix.len() >= 16 {
                        let w = read_i32_le(prefix, 8)?;
                        let h = read_i32_le(prefix, 12)?;
                        if w > 0 && h > 0 && w <= 2048 && h <= 2048 {
                            let stride = ceil_to_multiple(w, 8) as usize;
                            let data_len = stride.saturating_mul(h as usize);
                            let min_len = 16usize.saturating_add(data_len);
                            if min_len <= prefix.len() {
                                let mut fixed = Vec::with_capacity(min_len.saturating_add(2));
                                fixed.push(23u8); // SPT_BITMAP
                                fixed.extend_from_slice(&prefix[..min_len]);
                                fixed.push(0); // SPT_END

                                if temple_rt::sprite::sprite_parse_end_at_start(&fixed)
                                    == Some(fixed.len())
                                {
                                    return Some(fixed);
                                }
                            }
                        }
                    }

                    None
                }

                let mut bins: std::collections::BTreeMap<u32, Vec<u8>> =
                    std::collections::BTreeMap::new();
                if bin_blob.len() < BIN_HEADER_LEN + 1 {
                    return bins;
                }

                if expected_bins.is_empty() {
                    return bins;
                }

                // Some TempleOS docs have a binary tail that isn't cleanly parseable by walking
                // `CDocBin.size` sequentially (sizes can be corrupted, padding can be non-zero, etc).
                //
                // Instead, scan the bin tail for plausible `CDocBin` headers and recover the best
                // sprite for each referenced `BI=<n>` number.
                let scan_end = bin_blob.len().saturating_sub(BIN_HEADER_LEN + 1);
                for off in 0..=scan_end {
                    let Some(num) = read_u32_le(bin_blob, off) else {
                        continue;
                    };
                    if !expected_bins.contains(&num) {
                        continue;
                    }

                    let Some(flags) = read_u32_le(bin_blob, off + 4) else {
                        continue;
                    };
                    if flags != 0 {
                        continue;
                    }

                    // Records are typically preceded by the previous sprite's `SPT_END` (0).
                    // Some vendored files have 0xff here due to off-by-one corruption.
                    if off != 0 && !matches!(bin_blob.get(off - 1), Some(0 | 0xff)) {
                        continue;
                    }

                    let data_start = off.saturating_add(BIN_HEADER_LEN);
                    if data_start >= bin_blob.len() {
                        continue;
                    }

                    let mut best: Option<Vec<u8>> = None;

                    // Try the stored `size` field first when it looks sane.
                    if let Some(size_raw) = read_u32_le(bin_blob, off + 8) {
                        let size = size_raw as usize;
                        if size != 0 && size <= MAX_DATA_LEN && data_start + size <= bin_blob.len()
                        {
                            let raw = &bin_blob[data_start..data_start + size];
                            best = repair_sprite_blob(raw);
                        }
                    }

                    // Attempt to repair from a bounded prefix too (ignore `size`).
                    //
                    // Prefer the longer recovered sprite, because some vendored docs (notably
                    // `::/PersonalMenu.DD`) have corrupted `CDocBin.size` fields that truncate the
                    // stored sprite data mid-element.
                    {
                        let end = (data_start + MAX_DATA_LEN).min(bin_blob.len());
                        let raw = &bin_blob[data_start..end];
                        if let Some(candidate) = repair_sprite_blob(raw) {
                            match &best {
                                Some(prev) if prev.len() >= candidate.len() => {}
                                _ => best = Some(candidate),
                            }
                        }
                    }

                    let Some(best) = best else {
                        continue;
                    };

                    if best.len() <= 1 {
                        continue;
                    }

                    match bins.get(&num) {
                        Some(prev) if prev.len() >= best.len() => {}
                        _ => {
                            bins.insert(num, best);
                        }
                    }
                }

                bins
            }

            let cutoff = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let text_bytes = &buf[..cutoff];
            let text = std::str::from_utf8(text_bytes)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| assets::decode_cp437_bytes(text_bytes));

            if cutoff >= buf.len() {
                return (text, std::collections::BTreeMap::new());
            }

            let expected_bins = extract_expected_bins(&text);
            let bins = parse_doc_bins(&buf[cutoff + 1..], &expected_bins);
            (text, bins)
        }

        let (topic, jump) = self.normalize_doc_target(topic);
        let topic = topic.trim();
        let jump = jump.as_deref();
        if topic.is_empty() {
            return false;
        }

        if let Some(state) = self.doc_viewer.as_mut() {
            if let Some(&line) = state.anchors.get(topic) {
                let content_rows = Self::doc_view_rows().max(1);
                let max_scroll = state.lines.len().saturating_sub(content_rows);
                state.scroll = line.min(max_scroll);
                state.msg = "jump".to_string();
                self.render_doc_viewer(term);
                return true;
            }
        }

        let mut candidates: Vec<TemplePath> = Vec::new();
        if topic.starts_with('/') {
            candidates.push(self.cwd.resolve(topic));
        } else {
            candidates.push(TemplePath::root().resolve(&format!("/Doc/{topic}.TD")));
            candidates.push(TemplePath::root().resolve(&format!("/Doc/{topic}.td")));
            candidates.push(TemplePath::root().resolve(&format!("/Doc/{topic}.txt")));
            candidates.push(TemplePath::root().resolve(&format!("/Doc/{topic}")));
        }

        for doc_path in candidates {
            let host = doc_path.to_host_path(&self.root_dir);
            let file = match std::fs::File::open(&host) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

            use std::io::Read as _;
            let mut buf = Vec::new();
            if file.take(MAX_BYTES).read_to_end(&mut buf).is_err() {
                continue;
            }
            let (text, bins) = parse_doc_blob(&buf);

            let kind = if host
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("DD"))
            {
                DocKind::DolDoc
            } else {
                DocKind::TempleDoc
            };
            self.open_doc_viewer(
                doc_path.display(),
                kind,
                &text,
                bins,
                meta_len > MAX_BYTES,
                jump,
                term,
            );
            return true;
        }

        // Fall back to TempleOS' vendored docs tree (DolDoc .DD files).
        let Some(templeos_root) = discover_templeos_root() else {
            return false;
        };

        let mut templeos_candidates: Vec<(String, PathBuf)> = Vec::new();
        let t = topic.trim();
        if t.starts_with("::/") {
            let rel = t.trim_start_matches("::/");
            templeos_candidates.push((t.to_string(), templeos_root.join(rel)));
        } else {
            // Common case: "help Foo" should open ::/Doc/Foo.DD.
            templeos_candidates.push((
                format!("::/Doc/{t}.DD"),
                templeos_root.join(format!("Doc/{t}.DD")),
            ));
            templeos_candidates.push((
                format!("::/Doc/{t}.dd"),
                templeos_root.join(format!("Doc/{t}.dd")),
            ));
            // Allow explicit filenames or paths (e.g. "Doc/Foo.DD", "Demo/Print.HC").
            templeos_candidates.push((
                format!("::/Doc/{t}"),
                templeos_root.join(format!("Doc/{t}")),
            ));
            if t.contains('/') {
                templeos_candidates.push((format!("::/{t}"), templeos_root.join(t)));
            }
        }

        for (spec, host) in templeos_candidates {
            let file = match std::fs::File::open(&host) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

            use std::io::Read as _;
            let mut buf = Vec::new();
            if file.take(MAX_BYTES).read_to_end(&mut buf).is_err() {
                continue;
            }
            let (text, bins) = parse_doc_blob(&buf);

            let kind = if host
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("DD"))
            {
                DocKind::DolDoc
            } else {
                DocKind::PlainText
            };
            self.open_doc_viewer(spec.clone(), kind, &text, bins, meta_len > MAX_BYTES, jump, term);
            return true;
        }

        false
    }

    fn render_tdoc(&self, text: &str, term: &mut Terminal) {
        use fmt::Write as _;

        let saved_fg = term.fg;
        let saved_bg = term.bg;

        for line in text.lines() {
            if let Some(heading) = line.strip_prefix('#') {
                let mut level = 1usize;
                for c in heading.chars() {
                    if c == '#' {
                        level += 1;
                    } else {
                        break;
                    }
                }
                let heading = line.trim_start_matches('#').trim();
                let heading_fg = match level {
                    1 => 14,
                    2 => 11,
                    _ => 10,
                };
                term.set_colors(heading_fg, saved_bg);
                let _ = writeln!(term, "{heading}");
                term.set_colors(saved_fg, saved_bg);
                continue;
            }

            let bg = saved_bg;
            let mut fg = saved_fg;

            let mut rest = line;
            let mut in_code = false;
            term.set_colors(saved_fg, saved_bg);

            while !rest.is_empty() {
                let next_tick = rest.find('`');
                let next_link = if in_code { None } else { rest.find("[[") };

                let next = match (next_tick, next_link) {
                    (None, None) => {
                        let _ = write!(term, "{rest}");
                        break;
                    }
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
                let _ = write!(term, "{before}");

                match kind {
                    "tick" => {
                        rest = &after[1..];
                        in_code = !in_code;
                        if in_code {
                            fg = 11;
                        } else {
                            fg = saved_fg;
                        }
                        term.set_colors(fg, bg);
                    }
                    "link" => {
                        let Some(close) = after.find("]]") else {
                            let _ = write!(term, "{after}");
                            break;
                        };
                        let inner = &after[2..close];
                        term.set_colors(10, bg);
                        let _ = write!(term, "{inner}");
                        term.set_colors(fg, bg);
                        rest = &after[close + 2..];
                    }
                    _ => unreachable!(),
                }
            }

            let _ = writeln!(term, "");
        }

        term.set_colors(saved_fg, saved_bg);
    }

    fn render_doldoc(
        &self,
        text: &str,
        bins: &std::collections::BTreeMap<u32, Vec<u8>>,
        term: &mut Terminal,
    ) -> DolDocMeta {
        use fmt::Write as _;

        let default_fg = term.fg;
        let default_bg = term.bg;
        let mut fg = default_fg;
        let mut bg = default_bg;
        let mut indent_cols: i32 = 0;
        let mut meta = DolDocMeta::default();

        let mut bk_saved_bg: Option<u8> = None;
        let mut iv_saved: Option<(u8, u8)> = None;
        let mut hl_saved_fg: Option<u8> = None;
        let mut ul_saved_fg: Option<u8> = None;

        fn write_text_chunk(term: &mut Terminal, chunk: &str, fg: u8, bg: u8, indent_cols: i32) {
            term.set_colors(fg, bg);

            for line in chunk.split_inclusive('\n') {
                let (mut body, has_nl) = line
                    .strip_suffix('\n')
                    .map(|s| (s, true))
                    .unwrap_or((line, false));

                // TempleOS docs commonly embed "//" comments in the source that are not meant
                // to display. Be conservative and only treat it as a comment when preceded
                // by whitespace (to avoid "http://").
                if let Some(pos) = body.find("//") {
                    if pos == 0
                        || body
                            .as_bytes()
                            .get(pos.saturating_sub(1))
                            .is_some_and(|b| b.is_ascii_whitespace())
                    {
                        body = body[..pos].trim_end();
                    }
                }

                for ch in body.chars() {
                    if term.cursor_col == 0 && ch != '\n' && indent_cols > 0 {
                        for _ in 0..(indent_cols as usize).min(TERM_COLS as usize) {
                            term.put_char(' ');
                        }
                    }
                    term.put_char(ch);
                }

                if has_nl {
                    term.put_char('\n');
                }
            }
        }

        fn parse_quoted_args(cmd: &str, max: usize) -> Vec<String> {
            let mut out = Vec::new();
            if max == 0 {
                return out;
            }

            let mut chars = cmd.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch != '"' {
                    continue;
                }

                let mut s = String::new();
                let mut escaped = false;
                while let Some(c) = chars.next() {
                    if escaped {
                        s.push(match c {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            '"' => '"',
                            '\\' => '\\',
                            other => other,
                        });
                        escaped = false;
                        continue;
                    }
                    if c == '\\' {
                        escaped = true;
                        continue;
                    }
                    if c == '"' {
                        break;
                    }
                    s.push(c);
                }
                out.push(s);
                if out.len() >= max {
                    break;
                }
            }
            out
        }

        fn parse_attr_quoted(cmd: &str, key: &str) -> Option<String> {
            let needle = format!("{key}=\"");
            let start = cmd.find(&needle)? + needle.len();

            let mut out = String::new();
            let mut escaped = false;
            for ch in cmd[start..].chars() {
                if escaped {
                    out.push(match ch {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    return Some(out);
                }
                out.push(ch);
            }
            Some(out)
        }

	        fn parse_kv_i32(args: &str, key: &str) -> Option<i32> {
	            let needle = format!("{key}=");
	            let idx = args.find(&needle)? + needle.len();
	            let rest = &args[idx..];
	            let end = rest
	                .find(|c: char| c == ',' || c.is_ascii_whitespace())
	                .unwrap_or(rest.len());
	            rest[..end].trim().parse::<i32>().ok()
	        }

	        fn extract_first_http_url(html: &str) -> Option<String> {
	            let start = html
	                .find("https://")
	                .or_else(|| html.find("http://"))?;
	            let rest = &html[start..];
	            let end = rest
	                .find(|c: char| {
	                    c == '"' || c == '\'' || c.is_ascii_whitespace() || c == '<' || c == '>'
	                })
	                .unwrap_or(rest.len());
	            let url = rest[..end].trim().to_string();
	            if url.is_empty() { None } else { Some(url) }
	        }

	        fn url_label(url: &str) -> String {
	            let url = url.split('?').next().unwrap_or(url);
	            let url = url.split('#').next().unwrap_or(url);
	            let last = url.rsplit('/').next().unwrap_or(url).trim();
	            let mut label = if last.is_empty() { url } else { last }.to_string();
	            if label.len() > 32 {
	                label.truncate(29);
	                label.push_str("...");
	            }
	            label
	        }

	        fn write_link_chunk(
	            term: &mut Terminal,
	            chunk: &str,
	            fg: u8,
	            bg: u8,
            indent_cols: i32,
            target: &DocLinkTarget,
            links: &mut Vec<DocLink>,
        ) {
            term.set_colors(fg, bg);

            let mut cur: Option<(usize, usize, usize)> = None;
            let flush = |links: &mut Vec<DocLink>, cur: &mut Option<(usize, usize, usize)>| {
                if let Some((line, col_start, col_end)) = cur.take() {
                    links.push(DocLink {
                        line,
                        col_start,
                        col_end,
                        target: target.clone(),
                    });
                }
            };

            for line in chunk.split_inclusive('\n') {
                let (mut body, has_nl) = line
                    .strip_suffix('\n')
                    .map(|s| (s, true))
                    .unwrap_or((line, false));

                if let Some(pos) = body.find("//") {
                    if pos == 0
                        || body
                            .as_bytes()
                            .get(pos.saturating_sub(1))
                            .is_some_and(|b| b.is_ascii_whitespace())
                    {
                        body = body[..pos].trim_end();
                    }
                }

                for ch in body.chars() {
                    if term.cursor_col == 0 && ch != '\n' && indent_cols > 0 {
                        for _ in 0..(indent_cols as usize).min(TERM_COLS as usize) {
                            let line_idx = term.scrollback.len() + term.cursor_row as usize;
                            let col_idx = term.cursor_col as usize;
                            match &mut cur {
                                Some((cur_line, _start, end))
                                    if *cur_line == line_idx && col_idx == *end =>
                                {
                                    *end += 1;
                                }
                                Some(_) => {
                                    flush(links, &mut cur);
                                    cur = Some((line_idx, col_idx, col_idx + 1));
                                }
                                None => cur = Some((line_idx, col_idx, col_idx + 1)),
                            }
                            term.put_char(' ');
                        }
                    }

                    let line_idx = term.scrollback.len() + term.cursor_row as usize;
                    let col_idx = term.cursor_col as usize;
                    match &mut cur {
                        Some((cur_line, _start, end))
                            if *cur_line == line_idx && col_idx == *end =>
                        {
                            *end += 1;
                        }
                        Some(_) => {
                            flush(links, &mut cur);
                            cur = Some((line_idx, col_idx, col_idx + 1));
                        }
                        None => cur = Some((line_idx, col_idx, col_idx + 1)),
                    }
                    term.put_char(ch);
                }

                if has_nl {
                    term.put_char('\n');
                    flush(links, &mut cur);
                }
            }

            flush(links, &mut cur);
        }

        fn div_ceil_i32(v: i32, step: i32) -> i32 {
            debug_assert!(step > 0);
            let q = v.div_euclid(step);
            let r = v.rem_euclid(step);
            if r == 0 { q } else { q + 1 }
        }

        fn sprite_bbox_cells(
            anchor_line: usize,
            anchor_col: usize,
            bounds: temple_rt::sprite::SpriteBounds,
        ) -> (i32, i32, i32, i32) {
            let mut x0 = bounds.x0;
            let mut y0 = bounds.y0;
            let mut x1 = bounds.x1;
            let mut y1 = bounds.y1;
            if x1 <= x0 {
                x0 = 0;
                x1 = FONT_W as i32;
            }
            if y1 <= y0 {
                y0 = 0;
                y1 = FONT_H as i32;
            }

            let cell_w = FONT_W as i32;
            let cell_h = FONT_H as i32;
            let anchor_line = anchor_line as i32;
            let anchor_col = anchor_col as i32;

            let col0 = anchor_col.saturating_add(x0.div_euclid(cell_w));
            let col1 = anchor_col.saturating_add(div_ceil_i32(x1, cell_w));
            let line0 = anchor_line.saturating_add(y0.div_euclid(cell_h));
            let line1 = anchor_line.saturating_add(div_ceil_i32(y1, cell_h));

            let col1 = if col1 <= col0 { col0.saturating_add(1) } else { col1 };
            let line1 = if line1 <= line0 { line0.saturating_add(1) } else { line1 };
            (line0, col0, line1, col1)
        }

        let mut rest = text;
        while let Some(start) = rest.find('$') {
            let (before, after) = rest.split_at(start);
            write_text_chunk(term, before, fg, bg, indent_cols);

            // TempleOS DolDoc escaping:
            // - "$$" renders a literal '$'
            // - "$$$...$$$" renders a literal DolDoc cmd, like "$FG,RED$"
            //   (used heavily in docs like `Doc/DolDocOverview.DD` when describing DolDoc itself).
            if after.starts_with("$$") {
                if after.starts_with("$$$") {
                    if let Some(end_rel) = after[3..].find("$$$") {
                        let inner = &after[3..3 + end_rel];
                        write_text_chunk(term, "$", fg, bg, indent_cols);
                        write_text_chunk(term, inner, fg, bg, indent_cols);
                        write_text_chunk(term, "$", fg, bg, indent_cols);
                        rest = &after[3 + end_rel + 3..];
                        continue;
                    }
                }

                write_text_chunk(term, "$", fg, bg, indent_cols);
                rest = &after[2..];
                continue;
            }

            let after = &after[1..];
            let Some(end) = after.find('$') else {
                // Unterminated command; render the rest literally.
                write_text_chunk(term, "$", fg, bg, indent_cols);
                write_text_chunk(term, after, fg, bg, indent_cols);
                rest = "";
                break;
            };

            let (cmd, after_cmd) = after.split_at(end);
            rest = &after_cmd[1..];

            let cmd = cmd.trim();
            if cmd.is_empty() {
                write_text_chunk(term, "$", fg, bg, indent_cols);
                continue;
            }

            let (op_flags, args) = cmd.split_once(',').unwrap_or((cmd, ""));
            let mut parts = op_flags.split('+');
            let op = parts.next().unwrap_or("").trim();
            let flags: Vec<&str> = parts.collect();

            match op {
                "FG" => {
                    let arg = args.trim();
                    if arg.is_empty() {
                        fg = default_fg;
                    } else if let Ok(v) = arg.parse::<u8>() {
                        fg = v.min(15);
                    }
                }
                "BG" => {
                    let arg = args.trim();
                    if arg.is_empty() {
                        bg = default_bg;
                    } else if let Ok(v) = arg.parse::<u8>() {
                        bg = v.min(15);
                    }
                }
                "WW" => {
                    // Word-wrap control. We always wrap at TERM_COLS; ignore for now.
                }
                "CM-RE" => {
                    // Cursor move (X only). Used by docs like `Doc/Job.DD` for column layouts.
                    let x = parse_kv_i32(args, "LE").or_else(|| args.trim().parse::<i32>().ok());
                    if let Some(x) = x {
                        if x > 0 {
                            for _ in 0..x {
                                term.put_char(' ');
                            }
                        } else if x < 0 {
                            term.cursor_col = term.cursor_col.saturating_sub((-x) as u32);
                        }
                    }
                }
                "CM" => {
                    let x = parse_kv_i32(args, "LE").or_else(|| {
                        args.split(',')
                            .next()
                            .and_then(|s| s.trim().parse::<i32>().ok())
                    });
                    let y = parse_kv_i32(args, "RE").or_else(|| {
                        args.split(',')
                            .nth(1)
                            .and_then(|s| s.trim().parse::<i32>().ok())
                    });

                    if let Some(y) = y {
                        if y > 0 {
                            for _ in 0..y {
                                term.put_char('\n');
                            }
                        }
                    }

                    if let Some(x) = x {
                        let mut col = term.cursor_col as i32;
                        if flags.iter().any(|f| f.eq_ignore_ascii_case(&"LX")) {
                            col = x;
                        } else if flags.iter().any(|f| f.eq_ignore_ascii_case(&"RX")) {
                            col = TERM_COLS as i32 + x;
                        } else {
                            col += x;
                        }
                        term.cursor_col = col.clamp(0, TERM_COLS as i32 - 1) as u32;
                    }
                }
                "BK" => {
                    // Blink in TempleOS text mode; approximate by using a strong background.
                    let v = args.trim().parse::<i32>().unwrap_or(0);
                    if v == 0 {
                        if let Some(prev) = bk_saved_bg.take() {
                            bg = prev;
                        }
                    } else {
                        if bk_saved_bg.is_none() {
                            bk_saved_bg = Some(bg);
                        }
                        bg = COLOR_STATUS_BG;
                    }
                }
                "IV" => {
                    // Inverted video.
                    let v = args.trim().parse::<i32>().unwrap_or(0);
                    if v == 0 {
                        if let Some((prev_fg, prev_bg)) = iv_saved.take() {
                            fg = prev_fg;
                            bg = prev_bg;
                        }
                    } else if iv_saved.is_none() {
                        iv_saved = Some((fg, bg));
                        std::mem::swap(&mut fg, &mut bg);
                    }
                }
                "HL" => {
                    // Syntax highlighting toggle. We don't parse syntax yet; just tint code blocks.
                    let v = args.trim().parse::<i32>().unwrap_or(0);
                    if v == 0 {
                        if let Some(prev) = hl_saved_fg.take() {
                            fg = prev;
                        }
                    } else {
                        if hl_saved_fg.is_none() {
                            hl_saved_fg = Some(fg);
                        }
                        fg = 11;
                    }
                }
                "UL" => {
                    // Underline: approximate by tinting.
                    let v = args.trim().parse::<i32>().unwrap_or(0);
                    if v == 0 {
                        if let Some(prev) = ul_saved_fg.take() {
                            fg = prev;
                        }
                    } else {
                        if ul_saved_fg.is_none() {
                            ul_saved_fg = Some(fg);
                        }
                        fg = 14;
                    }
                }
                "ID" => {
                    if let Ok(delta) = args.trim().parse::<i32>() {
                        indent_cols = (indent_cols + delta).clamp(0, TERM_COLS as i32 - 1);
                    }
                }
                "AN" => {
                    // Anchor tag; doesn't render text by default.
                    if let Some(anchor) = parse_attr_quoted(cmd, "A") {
                        let anchor = anchor.trim().to_string();
                        if !anchor.is_empty() {
                            let line_idx = term.scrollback.len() + term.cursor_row as usize;
                            meta.anchors.insert(anchor, line_idx);
                        }
                    }
                }
                "TR" => {
                    if let Some(s) = parse_quoted_args(cmd, 1).into_iter().next() {
                        if term.cursor_col != 0 {
                            let _ = writeln!(term, "");
                        }
                        term.set_colors(14, default_bg);
                        let _ = writeln!(term, "{s}");
                        term.set_colors(fg, bg);
                    }
                }
                "LK" => {
                    let mut quoted = parse_quoted_args(cmd, 2);
                    let label = quoted.first().cloned();
                    let second = if quoted.len() >= 2 {
                        Some(quoted.remove(1))
                    } else {
                        None
                    };
                    let Some(label) = label else {
                        continue;
                    };

                    let target = parse_attr_quoted(cmd, "A")
                        .or(second)
                        .unwrap_or_else(|| label.clone());
                    let target = target.trim().to_string();
                    if target.is_empty() {
                        continue;
                    }

                    let target = DocLinkTarget::Doc(target);
                    write_link_chunk(term, &label, 10, bg, indent_cols, &target, &mut meta.links);
                    term.set_colors(fg, bg);
	                }
	                "TX" => {
	                    if let Some(s) = parse_quoted_args(cmd, 1).into_iter().next() {
	                        let cx = flags.iter().any(|f| f.eq_ignore_ascii_case(&"CX"));
	                        if cx {
                            let row = term.cursor_row.min(PROMPT_ROW.saturating_sub(1));
                            let len = s.chars().count().min(TERM_COLS as usize);
                            let col = TERM_COLS.saturating_sub(len as u32) / 2;
                            term.write_at(col, row, fg, bg, &s);
                            term.cursor_row = row;
                            term.cursor_col = (col + len as u32).min(TERM_COLS);
                        } else {
                            write_text_chunk(term, &s, fg, bg, indent_cols);
                        }
	                    }
	                }
	                // HTML blocks (often images/embeds). TempleOS uses these primarily for HTML export;
	                // show a small placeholder and optionally expose the first URL as a clickable link.
	                "HC" => {
	                    let html = parse_quoted_args(cmd, 1).into_iter().next().unwrap_or_default();
	                    if let Some(url) = extract_first_http_url(&html) {
	                        let label = url_label(&url);
	                        let target = DocLinkTarget::Action(format!("templelinux:browse:{url}"));
	                        write_link_chunk(
	                            term,
	                            &label,
	                            10,
	                            bg,
	                            indent_cols,
	                            &target,
	                            &mut meta.links,
	                        );
	                        term.set_colors(fg, bg);
	                    } else {
	                        write_text_chunk(term, "[html]", fg, bg, indent_cols);
	                        term.set_colors(fg, bg);
	                    }
	                }
	                // Psalmody songs embedded in docs.
	                "SO" => {
	                    let label = parse_quoted_args(cmd, 1).into_iter().next();
	                    let action = parse_attr_quoted(cmd, "A")
	                        .or_else(|| parse_attr_quoted(cmd, "LM"))
	                        .map(|s| s.trim().to_string())
	                        .filter(|s| !s.is_empty());

	                    let shown = label.unwrap_or_default();
	                    let shown = shown.trim();
	                    if shown.is_empty() {
	                        continue;
	                    }

	                    if let Some(song) = action {
	                        let target = DocLinkTarget::Action(format!("templelinux:song:{song}"));
	                        write_link_chunk(
	                            term,
	                            shown,
	                            10,
	                            bg,
	                            indent_cols,
	                            &target,
	                            &mut meta.links,
	                        );
	                    } else {
	                        write_text_chunk(term, shown, fg, bg, indent_cols);
	                    }
	                    term.set_colors(fg, bg);
	                }
                "SP" => {
                    // Sprite: `BI=<n>` refers to binary data stored after the NUL terminator
                    // in `.DD` files (TempleOS `CDocBin` tail).
                    let tag = parse_quoted_args(cmd, 1).into_iter().next().unwrap_or_default();
                    let action = parse_attr_quoted(cmd, "LM")
                        .or_else(|| parse_attr_quoted(cmd, "A"))
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());

                    let start_line = term.scrollback.len() + term.cursor_row as usize;
                    let start_col = term.cursor_col as usize;

                    if !tag.is_empty() {
                        if let Some(action) = action.as_ref() {
                            let target = DocLinkTarget::Action(action.clone());
                            write_link_chunk(
                                term,
                                &tag,
                                10,
                                bg,
                                indent_cols,
                                &target,
                                &mut meta.links,
                            );
                        } else {
                            write_text_chunk(term, &tag, fg, bg, indent_cols);
                        }
                        term.set_colors(fg, bg);
                    }

                    let end_line = term.scrollback.len() + term.cursor_row as usize;
                    let end_col = term.cursor_col as usize;

                    let from_start = flags
                        .iter()
                        .any(|f| f.eq_ignore_ascii_case(&"FST"));
                    let (anchor_line, anchor_col) = if from_start {
                        (start_line, start_col)
                    } else {
                        (end_line, end_col)
                    };

                    let bi = parse_kv_i32(args, "BI").and_then(|v| u32::try_from(v).ok());
                    let data = bi.and_then(|n| bins.get(&n).map(|b| (n, b)));
                    if let Some((bin_num, data)) = data {
                        let bounds = temple_rt::sprite::sprite_bounds(data).unwrap_or_default();
                        let (bbox_line0, bbox_col0, bbox_line1, bbox_col1) =
                            sprite_bbox_cells(anchor_line, anchor_col, bounds);
                        meta.sprites.push(DocSprite {
                            anchor_line,
                            anchor_col,
                            bbox_line0,
                            bbox_col0,
                            bbox_line1,
                            bbox_col1,
                            bin_num,
                            action,
                        });
                    } else if tag.is_empty() {
                        // Fallback: show a small placeholder tag so docs don't look like they're missing content.
                        write_text_chunk(term, "[sprite]", fg, bg, indent_cols);
                        term.set_colors(fg, bg);
                    }
                }
                // Insert Binary (pointer): not visual; show a minimal placeholder.
                "IB" => {
                    let tag = parse_quoted_args(cmd, 1).into_iter().next().unwrap_or_default();
                    let bi = parse_kv_i32(args, "BI").and_then(|v| u32::try_from(v).ok());
                    let data = bi.and_then(|n| bins.get(&n).map(|b| (n, b)));

                    if tag.trim().is_empty() {
                        if let Some((bin_num, data)) = data {
                            let line_idx = term.scrollback.len() + term.cursor_row as usize;
                            let col_idx = term.cursor_col as usize;
                            let action = parse_attr_quoted(cmd, "LM")
                                .or_else(|| parse_attr_quoted(cmd, "A"))
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty());
                            let bounds = temple_rt::sprite::sprite_bounds(data).unwrap_or_default();
                            let (bbox_line0, bbox_col0, bbox_line1, bbox_col1) =
                                sprite_bbox_cells(line_idx, col_idx, bounds);
                            meta.sprites.push(DocSprite {
                                anchor_line: line_idx,
                                anchor_col: col_idx,
                                bbox_line0,
                                bbox_col0,
                                bbox_line1,
                                bbox_col1,
                                bin_num,
                                action,
                            });

                            // Reserve horizontal space so inline binaries don't overlap following text.
                            let reserve_cols = {
                                let px_right = bounds.x1.max(0) as u32;
                                (px_right.saturating_add(FONT_W - 1) / FONT_W).clamp(1, TERM_COLS)
                            };

                            let avail = TERM_COLS.saturating_sub(term.cursor_col);
                            for _ in 0..reserve_cols.min(avail) {
                                term.put_char(' ');
                            }

                            term.set_colors(fg, bg);
                        } else {
                            write_text_chunk(term, "[bin]", fg, bg, indent_cols);
                            term.set_colors(fg, bg);
                        }
                    } else {
                        write_text_chunk(term, &tag, fg, bg, indent_cols);
                        term.set_colors(fg, bg);
                    }
                }
                // Menu anchors are often directory links in DemoIndex.
                "MA-X" | "MA" => {
                    let label = parse_quoted_args(cmd, 1).into_iter().next();
                    let action = parse_attr_quoted(cmd, "LM")
                        .or_else(|| parse_attr_quoted(cmd, "A"))
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());

                    let shown = label
                        .clone()
                        .or_else(|| action.clone())
                        .unwrap_or_default();
                    let shown = shown.trim();
                    if shown.is_empty() {
                        continue;
                    }

                    let target = if let Some(action) = action {
                        DocLinkTarget::Action(action)
                    } else {
                        DocLinkTarget::Doc(shown.to_string())
                    };

                    write_link_chunk(term, shown, 10, bg, indent_cols, &target, &mut meta.links);
                    term.set_colors(fg, bg);
                }
                other => {
                    // Best-effort: if the command carries a quoted label/path, show it.
                    if let Some(s) = parse_quoted_args(cmd, 1).into_iter().next() {
                        let color = match other {
                            "MA-X" | "MA" => 10,
                            _ => fg,
                        };
                        write_text_chunk(term, &s, color, bg, indent_cols);
                        term.set_colors(fg, bg);
                    }
                }
            }
        }

        write_text_chunk(term, rest, fg, bg, indent_cols);
        term.set_colors(default_fg, default_bg);
        meta
    }

    fn cmd_pwd(&self, term: &mut Terminal) {
        use fmt::Write as _;
        let _ = writeln!(term, "{}", self.cwd.display());
    }

    fn cmd_cd(&mut self, args: &[&str], term: &mut Terminal) {
        let target = args.first().copied().unwrap_or("/");
        let next = self.cwd.resolve(target);
        let host = next.to_host_path(&self.root_dir);

        match std::fs::metadata(&host) {
            Ok(meta) if meta.is_dir() => {
                self.cwd = next;
            }
            Ok(_) => {
                use fmt::Write as _;
                let _ = writeln!(term, "cd: not a directory: {target}");
            }
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "cd: {target}: {err}");
            }
        }
    }

    fn cmd_ls(&self, args: &[&str], term: &mut Terminal) {
        let target = args.first().copied().unwrap_or(".");
        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);

        let meta = match std::fs::metadata(&host) {
            Ok(m) => m,
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "ls: {target}: {err}");
                return;
            }
        };

        if !meta.is_dir() {
            use fmt::Write as _;
            let _ = writeln!(term, "{target}");
            return;
        }

        let entries = match std::fs::read_dir(&host) {
            Ok(e) => e,
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "ls: {target}: {err}");
                return;
            }
        };

        let mut names = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let suffix = match entry.file_type() {
                Ok(ft) if ft.is_dir() => "/",
                _ => "",
            };
            names.push(format!("{name}{suffix}"));
        }
        names.sort_unstable();

        use fmt::Write as _;
        for name in names {
            let _ = writeln!(term, "{name}");
        }
    }

    fn cmd_cat(&self, args: &[&str], term: &mut Terminal) {
        const CAT_MAX_BYTES: u64 = 256 * 1024;

        let Some(target) = args.first().copied() else {
            use fmt::Write as _;
            let _ = writeln!(term, "cat: missing path");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let file = match std::fs::File::open(&host) {
            Ok(f) => f,
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "cat: {target}: {err}");
                return;
            }
        };

        let meta = match file.metadata() {
            Ok(m) => m,
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "cat: {target}: {err}");
                return;
            }
        };

        if meta.is_dir() {
            use fmt::Write as _;
            let _ = writeln!(term, "cat: {target}: is a directory");
            return;
        }

        if meta.len() > CAT_MAX_BYTES && self.env_bool_default("TEMPLE_CAT_AUTO_PAGER", true) {
            self.cmd_more(&[target], term);
            return;
        }

        use std::io::Read as _;
        let mut buf = Vec::new();
        if let Err(err) = file.take(CAT_MAX_BYTES).read_to_end(&mut buf) {
            use fmt::Write as _;
            let _ = writeln!(term, "cat: {target}: {err}");
            return;
        }

        use fmt::Write as _;
        let text = String::from_utf8_lossy(&buf);
        let _ = write!(term, "{text}");
        if !text.ends_with('\n') {
            let _ = writeln!(term, "");
        }
        if meta.len() > CAT_MAX_BYTES {
            let _ = writeln!(term, "[truncated: {} bytes total]", meta.len());
        }
    }

    fn cmd_cp(&self, args: &[&str], term: &mut Terminal) {
        let Some((src, dst)) = args
            .split_first()
            .and_then(|(a, rest)| rest.first().map(|b| (*a, *b)))
        else {
            use fmt::Write as _;
            let _ = writeln!(term, "cp: expected: cp <src> <dst>");
            return;
        };

        let src_path = self.cwd.resolve(src);
        let host_src = src_path.to_host_path(&self.root_dir);
        let dst_path = self.cwd.resolve(dst);
        let mut host_dst = dst_path.to_host_path(&self.root_dir);

        let src_meta = match std::fs::metadata(&host_src) {
            Ok(m) => m,
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "cp: {src}: {err}");
                return;
            }
        };
        if src_meta.is_dir() {
            use fmt::Write as _;
            let _ = writeln!(term, "cp: {src}: is a directory");
            return;
        }

        if let Ok(meta) = std::fs::metadata(&host_dst) {
            if meta.is_dir() {
                if let Some(name) = host_src.file_name() {
                    host_dst.push(name);
                }
            }
        }

        match std::fs::copy(&host_src, &host_dst) {
            Ok(bytes) => {
                use fmt::Write as _;
                let _ = writeln!(term, "cp: copied {bytes} bytes");
            }
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "cp: {err}");
            }
        }
    }

    fn cmd_mv(&self, args: &[&str], term: &mut Terminal) {
        let Some((src, dst)) = args
            .split_first()
            .and_then(|(a, rest)| rest.first().map(|b| (*a, *b)))
        else {
            use fmt::Write as _;
            let _ = writeln!(term, "mv: expected: mv <src> <dst>");
            return;
        };

        let src_path = self.cwd.resolve(src);
        let host_src = src_path.to_host_path(&self.root_dir);
        let dst_path = self.cwd.resolve(dst);
        let mut host_dst = dst_path.to_host_path(&self.root_dir);

        if let Ok(meta) = std::fs::metadata(&host_dst) {
            if meta.is_dir() {
                if let Some(name) = host_src.file_name() {
                    host_dst.push(name);
                }
            }
        }

        match std::fs::rename(&host_src, &host_dst) {
            Ok(()) => {
                use fmt::Write as _;
                let _ = writeln!(term, "mv: ok");
            }
            Err(err) => {
                // Cross-device rename fallback.
                if err.kind() == std::io::ErrorKind::CrossesDevices {
                    match std::fs::copy(&host_src, &host_dst) {
                        Ok(_) => match std::fs::remove_file(&host_src) {
                            Ok(()) => {
                                use fmt::Write as _;
                                let _ = writeln!(term, "mv: ok (copy+remove)");
                            }
                            Err(err) => {
                                use fmt::Write as _;
                                let _ = writeln!(term, "mv: remove src: {err}");
                            }
                        },
                        Err(err) => {
                            use fmt::Write as _;
                            let _ = writeln!(term, "mv: copy fallback failed: {err}");
                        }
                    }
                } else {
                    use fmt::Write as _;
                    let _ = writeln!(term, "mv: {err}");
                }
            }
        }
    }

    fn cmd_rm(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let mut recursive = false;
        let mut rest = args;
        if let Some((&flag, tail)) = rest.split_first() {
            if flag == "-r" || flag == "-rf" || flag == "-fr" {
                recursive = true;
                rest = tail;
            }
        }

        let Some(target) = rest.first().copied() else {
            let _ = writeln!(term, "rm: expected: rm [-r] <path>");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let meta = match std::fs::metadata(&host) {
            Ok(m) => m,
            Err(err) => {
                let _ = writeln!(term, "rm: {target}: {err}");
                return;
            }
        };

        if meta.is_dir() {
            if !recursive {
                let _ = writeln!(term, "rm: {target}: is a directory (use rm -r)");
                return;
            }
            match std::fs::remove_dir_all(&host) {
                Ok(()) => {
                    let _ = writeln!(term, "rm: ok");
                }
                Err(err) => {
                    let _ = writeln!(term, "rm: {target}: {err}");
                }
            }
            return;
        }

        match std::fs::remove_file(&host) {
            Ok(()) => {
                let _ = writeln!(term, "rm: ok");
            }
            Err(err) => {
                let _ = writeln!(term, "rm: {target}: {err}");
            }
        }
    }

    fn cmd_mkdir(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let Some(target) = args.first().copied() else {
            let _ = writeln!(term, "mkdir: expected: mkdir <path>");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        match std::fs::create_dir_all(&host) {
            Ok(()) => {
                let _ = writeln!(term, "mkdir: ok");
            }
            Err(err) => {
                let _ = writeln!(term, "mkdir: {target}: {err}");
            }
        }
    }

    fn cmd_touch(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let Some(target) = args.first().copied() else {
            let _ = writeln!(term, "touch: expected: touch <path>");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&host)
        {
            Ok(_) => {
                let _ = writeln!(term, "touch: ok");
            }
            Err(err) => {
                let _ = writeln!(term, "touch: {target}: {err}");
            }
        }
    }

    fn cmd_grep(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        const GREP_MAX_BYTES: u64 = 1024 * 1024;

        let Some(needle) = args.first().copied() else {
            let _ = writeln!(term, "grep: expected: grep <needle> <path>");
            return;
        };
        let Some(target) = args.get(1).copied() else {
            let _ = writeln!(term, "grep: expected: grep <needle> <path>");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let file = match std::fs::File::open(&host) {
            Ok(f) => f,
            Err(err) => {
                let _ = writeln!(term, "grep: {target}: {err}");
                return;
            }
        };
        let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

        use std::io::Read as _;
        let mut buf = Vec::new();
        if let Err(err) = file.take(GREP_MAX_BYTES).read_to_end(&mut buf) {
            let _ = writeln!(term, "grep: {target}: {err}");
            return;
        }

        let text = String::from_utf8_lossy(&buf);
        let mut hits = 0usize;
        for (idx, line) in text.lines().enumerate() {
            if line.contains(needle) {
                hits += 1;
                let _ = writeln!(term, "{}:{}", idx + 1, line);
            }
        }
        if meta_len > GREP_MAX_BYTES {
            let _ = writeln!(term, "[truncated: {} bytes total]", meta_len);
        }
        if hits == 0 {
            let _ = writeln!(term, "[no matches]");
        }
    }

    fn cmd_find(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let (start, needle) = match args {
            [] => (self.cwd.clone(), None),
            [path] => (self.cwd.resolve(path), None),
            [path, needle] => (self.cwd.resolve(path), Some(*needle)),
            _ => {
                let _ = writeln!(term, "find: expected: find [path] [name-substring]");
                return;
            }
        };

        let mut stack = vec![start.clone()];
        let mut any = false;

        while let Some(dir) = stack.pop() {
            let host = dir.to_host_path(&self.root_dir);
            let entries = match std::fs::read_dir(&host) {
                Ok(e) => e,
                Err(err) => {
                    let _ = writeln!(term, "find: {}: {err}", dir.display());
                    continue;
                }
            };

            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let mut child = dir.clone();
                child.components.push(name.clone());

                let ty = match entry.file_type() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let is_dir = ty.is_dir();

                if needle.is_none() || needle.is_some_and(|n| name.contains(n)) {
                    any = true;
                    let suffix = if is_dir { "/" } else { "" };
                    let _ = writeln!(term, "{}{suffix}", child.display());
                }

                if is_dir {
                    stack.push(child);
                }
            }
        }

        if !any {
            let _ = writeln!(term, "[no matches]");
        }
    }

    fn cmd_head(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        const MAX_BYTES: u64 = 1024 * 1024;
        let (n, target) = match args {
            [path] => (10usize, *path),
            ["-n", n, path] => match n.parse::<usize>() {
                Ok(v) => (v, *path),
                Err(_) => {
                    let _ = writeln!(term, "head: bad -n: {n}");
                    return;
                }
            },
            _ => {
                let _ = writeln!(term, "head: expected: head [-n N] <path>");
                return;
            }
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let file = match std::fs::File::open(&host) {
            Ok(f) => f,
            Err(err) => {
                let _ = writeln!(term, "head: {target}: {err}");
                return;
            }
        };
        let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

        use std::io::Read as _;
        let mut buf = Vec::new();
        if let Err(err) = file.take(MAX_BYTES).read_to_end(&mut buf) {
            let _ = writeln!(term, "head: {target}: {err}");
            return;
        }
        let text = String::from_utf8_lossy(&buf);
        for line in text.lines().take(n) {
            let _ = writeln!(term, "{line}");
        }
        if meta_len > MAX_BYTES {
            let _ = writeln!(term, "[truncated: {} bytes total]", meta_len);
        }
    }

    fn cmd_tail(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        const MAX_BYTES: u64 = 1024 * 1024;
        let (n, target) = match args {
            [path] => (10usize, *path),
            ["-n", n, path] => match n.parse::<usize>() {
                Ok(v) => (v, *path),
                Err(_) => {
                    let _ = writeln!(term, "tail: bad -n: {n}");
                    return;
                }
            },
            _ => {
                let _ = writeln!(term, "tail: expected: tail [-n N] <path>");
                return;
            }
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let file = match std::fs::File::open(&host) {
            Ok(f) => f,
            Err(err) => {
                let _ = writeln!(term, "tail: {target}: {err}");
                return;
            }
        };
        let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

        use std::io::Read as _;
        let mut buf = Vec::new();
        if let Err(err) = file.take(MAX_BYTES).read_to_end(&mut buf) {
            let _ = writeln!(term, "tail: {target}: {err}");
            return;
        }
        let text = String::from_utf8_lossy(&buf);
        let mut last: std::collections::VecDeque<&str> = std::collections::VecDeque::new();
        for line in text.lines() {
            if n == 0 {
                break;
            }
            last.push_back(line);
            while last.len() > n {
                last.pop_front();
            }
        }
        for line in last {
            let _ = writeln!(term, "{line}");
        }
        if meta_len > MAX_BYTES {
            let _ = writeln!(term, "[truncated: {} bytes total]", meta_len);
        }
    }

    fn cmd_wc(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        const MAX_BYTES: u64 = 1024 * 1024;

        let Some(target) = args.first().copied() else {
            let _ = writeln!(term, "wc: expected: wc <path>");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let file = match std::fs::File::open(&host) {
            Ok(f) => f,
            Err(err) => {
                let _ = writeln!(term, "wc: {target}: {err}");
                return;
            }
        };
        let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

        use std::io::Read as _;
        let mut buf = Vec::new();
        if let Err(err) = file.take(MAX_BYTES).read_to_end(&mut buf) {
            let _ = writeln!(term, "wc: {target}: {err}");
            return;
        }
        let text = String::from_utf8_lossy(&buf);

        let bytes = buf.len();
        let lines = text.lines().count();
        let words = text.split_whitespace().count();
        let _ = writeln!(term, "{lines}\t{words}\t{bytes}\t{target}");
        if meta_len > MAX_BYTES {
            let _ = writeln!(term, "[truncated: {} bytes total]", meta_len);
        }
    }

    fn cmd_more(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        const MAX_BYTES: u64 = 4 * 1024 * 1024;

        let Some(target) = args.first().copied() else {
            let _ = writeln!(term, "more: expected: more <path>");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let file = match std::fs::File::open(&host) {
            Ok(f) => f,
            Err(err) => {
                let _ = writeln!(term, "more: {target}: {err}");
                return;
            }
        };
        let meta_len = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

        term.clear_output();
        let _ = writeln!(term, "[more] {target}  (PgUp/PgDn scroll, Ctrl+End bottom)");
        let _ = writeln!(term, "");

        use std::io::Read as _;
        let mut buf = Vec::new();
        if let Err(err) = file.take(MAX_BYTES).read_to_end(&mut buf) {
            let _ = writeln!(term, "more: {target}: {err}");
            return;
        }

        let text = String::from_utf8_lossy(&buf);
        let _ = write!(term, "{text}");
        if !text.ends_with('\n') {
            let _ = writeln!(term, "");
        }
        if meta_len > MAX_BYTES {
            let _ = writeln!(term, "[truncated: {} bytes total]", meta_len);
        }

        term.scroll_view_to_top();
    }

    fn cmd_clip(&mut self, line: &str, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let sub = args.first().copied().unwrap_or("help");
        match sub {
            "get" => match self.clipboard.get_text() {
                Ok(text) => {
                    const MAX_PRINT: usize = 16 * 1024;

                    let _ = writeln!(term, "[clipboard: {} bytes]", text.len());
                    if text.len() <= MAX_PRINT {
                        let _ = writeln!(term, "{text}");
                    } else {
                        let mut end = MAX_PRINT;
                        while end > 0 && !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        let _ = writeln!(term, "{}\n[truncated]", &text[..end]);
                    }
                }
                Err(err) => {
                    let _ = writeln!(term, "clip: get: {err}");
                }
            },
            "set" => {
                let Some(rest) = line.strip_prefix("clip") else {
                    let _ = writeln!(term, "clip: internal parse error");
                    return;
                };
                let Some(rest) = rest.trim_start().strip_prefix("set") else {
                    let _ = writeln!(term, "clip: internal parse error");
                    return;
                };
                let text = rest.trim_start();
                if text.is_empty() {
                    let _ = writeln!(term, "clip: set: missing text");
                    return;
                }
                match self.clipboard.set_text(text) {
                    Ok(()) => {
                        let _ = writeln!(term, "[clipboard set: {} bytes]", text.len());
                    }
                    Err(err) => {
                        let _ = writeln!(term, "clip: set: {err}");
                    }
                }
            }
            "clear" => match self.clipboard.set_text("") {
                Ok(()) => {
                    let _ = writeln!(term, "[clipboard cleared]");
                }
                Err(err) => {
                    let _ = writeln!(term, "clip: clear: {err}");
                }
            },
            "help" | "-h" | "--help" => {
                let _ = writeln!(term, "clip get");
                let _ = writeln!(term, "clip set <text...>");
                let _ = writeln!(term, "clip clear");
                let _ = writeln!(term, "hotkey: Ctrl+V (or Shift+Ins) to paste");
            }
            other => {
                let _ = writeln!(term, "clip: unknown subcommand: {other}");
                let _ = writeln!(term, "clip help");
            }
        }
    }

    fn cmd_env(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        if args.is_empty() {
            let _ = writeln!(term, "TempleShell vars (override host env):");
            if self.vars.is_empty() {
                let _ = writeln!(term, "  (none)");
                return;
            }
            for (k, v) in &self.vars {
                let _ = writeln!(term, "  {k}={v}");
            }
            return;
        }

        let name = args[0];
        if let Some(v) = self.vars.get(name) {
            let _ = writeln!(term, "{name}={v} (shell)");
            return;
        }
        if let Ok(v) = std::env::var(name) {
            let _ = writeln!(term, "{name}={v} (host)");
            return;
        }
        let _ = writeln!(term, "{name} is not set");
    }

    fn cmd_set(&mut self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let Some(first) = args.first().copied() else {
            let _ = writeln!(term, "set <name>=<value>");
            let _ = writeln!(term, "set <name> <value...>");
            let _ = writeln!(term, "set <name> (clears)");
            return;
        };

        let (name, value) = if let Some((k, v)) = first.split_once('=') {
            (k, Some(v.to_string()))
        } else if args.len() >= 2 {
            (first, Some(args[1..].join(" ")))
        } else {
            (first, None)
        };

        if name.is_empty() {
            let _ = writeln!(term, "set: missing name");
            return;
        }

        if let Some(value) = value {
            self.vars.insert(name.to_string(), value.clone());
            self.save_vars();
            let _ = writeln!(term, "[set {name}={value}]");
            return;
        }

        self.vars.remove(name);
        self.save_vars();
        let _ = writeln!(term, "[cleared {name}]");
    }

    fn cmd_open(&self, args: &[&str], term: &mut Terminal) {
        let Some(target) = args.first().copied() else {
            use fmt::Write as _;
            let _ = writeln!(term, "open: missing path");
            return;
        };

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let switched = self.maybe_auto_linux_ws(term);
        let res = std::process::Command::new("xdg-open").arg(&host).spawn();

        use fmt::Write as _;
        match res {
            Ok(child) => {
                let _ = writeln!(term, "open: launched pid {}", child.id());
            }
            Err(err) => {
                if switched {
                    self.maybe_auto_temple_ws(term);
                }
                let _ = writeln!(term, "open: xdg-open: {err}");
            }
        }
    }

    fn cmd_browse(&self, args: &[&str], term: &mut Terminal) {
        let Some(url) = args.first().copied() else {
            use fmt::Write as _;
            let _ = writeln!(term, "browse: missing url");
            return;
        };

        let switched = self.maybe_auto_linux_ws(term);
        let res = std::process::Command::new("xdg-open").arg(url).spawn();

        use fmt::Write as _;
        match res {
            Ok(child) => {
                let _ = writeln!(term, "browse: launched pid {}", child.id());
            }
            Err(err) => {
                if switched {
                    self.maybe_auto_temple_ws(term);
                }
                let _ = writeln!(term, "browse: xdg-open: {err}");
            }
        }
    }

    fn cmd_ws(&self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let temple_ws = self.env_u32("TEMPLE_WS_TEMPLE", 1);
        let linux_ws = self.env_u32("TEMPLE_WS_LINUX", 2);

        let Some(target) = args.first().copied() else {
            let _ = writeln!(term, "ws <temple|linux|num>");
            let _ = writeln!(
                term,
                "ws: TEMPLE_WS_TEMPLE={temple_ws} TEMPLE_WS_LINUX={linux_ws} TEMPLE_AUTO_LINUX_WS={}",
                self.var("TEMPLE_AUTO_LINUX_WS")
                    .unwrap_or_else(|| "0".to_string())
            );
            return;
        };

        let number = match target {
            "temple" => temple_ws,
            "linux" => linux_ws,
            other => match other.parse::<u32>() {
                Ok(n) => n,
                Err(_) => {
                    let _ = writeln!(term, "ws: expected temple|linux|num, got: {other}");
                    return;
                }
            },
        };

        match self.sway_workspace_number(number) {
            Ok(()) => {
                let _ = writeln!(term, "ws: switched to workspace {number}");
            }
            Err(err) => {
                let _ = writeln!(term, "ws: {err}");
            }
        }
    }

    fn maybe_auto_linux_ws(&self, term: &mut Terminal) -> bool {
        if !self.env_bool("TEMPLE_AUTO_LINUX_WS") {
            return false;
        }
        let linux_ws = self.env_u32("TEMPLE_WS_LINUX", 2);
        match self.sway_workspace_number(linux_ws) {
            Ok(()) => true,
            Err(err) => {
                use fmt::Write as _;
                let _ = writeln!(term, "ws: {err}");
                false
            }
        }
    }

    fn maybe_auto_temple_ws(&self, term: &mut Terminal) {
        if !self.env_bool("TEMPLE_AUTO_LINUX_WS") {
            return;
        }
        let temple_ws = self.env_u32("TEMPLE_WS_TEMPLE", 1);
        if let Err(err) = self.sway_workspace_number(temple_ws) {
            use fmt::Write as _;
            let _ = writeln!(term, "ws: {err}");
        }
    }

    fn env_u32(&self, name: &str, default: u32) -> u32 {
        self.var(name)
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(default)
    }

    fn env_bool(&self, name: &str) -> bool {
        let Some(v) = self.var(name) else {
            return false;
        };
        matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    }

    fn env_bool_default(&self, name: &str, default: bool) -> bool {
        let Some(v) = self.var(name) else {
            return default;
        };
        matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
    }

    fn var(&self, name: &str) -> Option<String> {
        if let Some(v) = self.vars.get(name) {
            return Some(v.clone());
        }
        std::env::var(name).ok()
    }

    fn sway_workspace_number(&self, number: u32) -> Result<(), String> {
        if std::env::var_os("SWAYSOCK").is_none() {
            return Err("SWAYSOCK is not set (are you running under sway?)".to_string());
        }

        let output = std::process::Command::new("swaymsg")
            .arg(format!("workspace number {number}"))
            .output()
            .map_err(|err| format!("swaymsg: {err}"))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut msg = String::new();
        if !stderr.trim().is_empty() {
            msg.push_str(stderr.trim());
        }
        if msg.is_empty() && !stdout.trim().is_empty() {
            msg.push_str(stdout.trim());
        }
        if msg.is_empty() {
            msg.push_str("swaymsg failed");
        }
        Err(msg)
    }

    fn cmd_run(&self, args: &[&str], term: &mut Terminal) {
        let Some(program) = args.first().copied() else {
            use fmt::Write as _;
            let _ = writeln!(term, "run: missing command");
            return;
        };

        let switched = self.maybe_auto_linux_ws(term);
        let host_cwd = self.cwd.to_host_path(&self.root_dir);
        let res = std::process::Command::new(program)
            .args(&args[1..])
            .current_dir(host_cwd)
            .spawn();

        use fmt::Write as _;
        match res {
            Ok(child) => {
                let _ = writeln!(term, "run: launched pid {}", child.id());
            }
            Err(err) => {
                if switched {
                    self.maybe_auto_temple_ws(term);
                }
                let _ = writeln!(term, "run: {program}: {err}");
            }
        }
    }

    fn cmd_shutdown(&mut self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        if !args.is_empty() {
            let _ = writeln!(term, "shutdown: takes no args");
            return;
        }

        self.exit_requested = true;
        let _ = writeln!(term, "shutdown: requested");
    }

    fn cmd_screenshot(&mut self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        if args.len() > 1 {
            let _ = writeln!(term, "screenshot: expected: screenshot [path.png]");
            return;
        }

        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut input = args
            .first()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("/Home/screenshot-{uniq}.png"));
        if !input.to_ascii_lowercase().ends_with(".png") {
            input.push_str(".png");
        }

        let temple_path = self.cwd.resolve(&input);
        let host_path = temple_path.to_host_path(&self.root_dir);
        if let Some(parent) = host_path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                let _ = writeln!(term, "screenshot: {}: {err}", temple_path.display());
                return;
            }
        }

        self.pending_screenshot = Some((temple_path.display(), host_path));
    }

    fn cmd_tapp(&mut self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        match args.split_first() {
            Some((&"tree", rest)) => {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: tree takes no args");
                    return;
                }

                match discover_templeos_root() {
                    Some(root) => {
                        let _ = writeln!(term, "TempleOS tree: {}", root.display());
                    }
                    None => {
                        let _ = writeln!(term, "TempleOS tree: (not found)");
                        let _ = writeln!(
                            term,
                            "Hint: ensure third_party/TempleOS is near the binary, or set TEMPLEOS_ROOT."
                        );
                    }
                }
                return;
            }
            Some((&"search", rest)) => {
                let query = rest.join(" ");
                if query.trim().is_empty() {
                    let _ = writeln!(term, "tapp: expected: tapp search <text>");
                    return;
                }

                let Some(root) = discover_templeos_root() else {
                    let _ = writeln!(term, "tapp: TempleOS tree not found.");
                    let _ = writeln!(
                        term,
                        "Hint: ensure third_party/TempleOS is near the binary, or set TEMPLEOS_ROOT."
                    );
                    return;
                };

                let programs = discover_templeos_programs(&root);
                let q_lower = query.trim().to_ascii_lowercase();
                let mut hits: Vec<&TempleOsProgram> = programs
                    .iter()
                    .filter(|p| {
                        p.alias.to_ascii_lowercase().contains(&q_lower)
                            || p.rel_no_ext.to_ascii_lowercase().contains(&q_lower)
                    })
                    .collect();
                hits.sort_by(|a, b| a.rel_no_ext.cmp(&b.rel_no_ext));

                let _ = writeln!(term, "TempleOS hits: {}", hits.len());
                for p in hits.iter().take(200) {
                    let _ = writeln!(term, "  {:<18}  {}", p.alias, p.rel_no_ext);
                }
                if hits.len() > 200 {
                    let _ = writeln!(term, "  ... (truncated)");
                }
                let _ = writeln!(term, "Run: tapp run <alias|path>");
                return;
            }
            Some((&"run", rest)) => {
                let query = rest.join(" ");
                if query.trim().is_empty() {
                    let _ = writeln!(term, "tapp: expected: tapp run <alias|path>");
                    let _ = writeln!(term, "Example: tapp run Print");
                    let _ = writeln!(term, "Example: tapp run Demo/Graphics/NetOfDots");
                    let _ = writeln!(term, "Example: tapp run ::/Demo/PullDownMenu.HC");
                    return;
                }

                let Some(root) = discover_templeos_root() else {
                    let _ = writeln!(term, "tapp: TempleOS tree not found.");
                    let _ = writeln!(
                        term,
                        "Hint: ensure third_party/TempleOS is near the binary, or set TEMPLEOS_ROOT."
                    );
                    return;
                };
                let programs = discover_templeos_programs(&root);

                match resolve_templeos_program_spec(&programs, &query) {
                    Ok(spec) => {
                        let spec_owned = spec.clone();
                        self.cmd_tapp(&["hc", spec_owned.as_str()], term);
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: run: {err}");
                    }
                }
                return;
            }
            Some((&"list", rest)) => {
                let _ = writeln!(
                    term,
                    "tapp: connected: {}",
                    if self.tapp_connected { "yes" } else { "no" }
                );
                if let Some(last) = &self.tapp_last {
                    let args = if last.args.is_empty() {
                        "".to_string()
                    } else {
                        format!(" {}", last.args.join(" "))
                    };
                    let _ = writeln!(term, "tapp: last: {}{args}", last.program);
                } else {
                    let _ = writeln!(term, "tapp: last: (none)");
                }

                if let Some(child) = self.tapp_child.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let _ = writeln!(term, "tapp: child exited: {status}");
                            self.tapp_child = None;
                        }
                        Ok(None) => {
                            let _ = writeln!(term, "tapp: child pid {}", child.id());
                        }
                        Err(err) => {
                            let _ = writeln!(term, "tapp: child status: {err}");
                        }
                    }
                } else {
                    let _ = writeln!(term, "tapp: child: (none)");
                }

                if !rest.is_empty() {
                    let _ = writeln!(term, "");
                    let _ = writeln!(term, "tapp: note: 'tapp list' takes no args; ignoring.");
                }

                let _ = writeln!(term, "");
                let _ = writeln!(term, "Temple apps:");
                let _ = writeln!(term, "  tapp demo                 Graphics demo app");
                let _ = writeln!(term, "  tapp paint                Pixel paint app");
                let _ = writeln!(term, "  tapp linuxbridge           Linux integration app");
                let _ = writeln!(term, "  tapp timeclock             TimeClock wrapper app");
                let _ = writeln!(term, "  tapp sounddemo             Sound output demo");
                let _ = writeln!(term, "  tapp logic                 Digital logic app (upstream)");
                let _ = writeln!(term, "  tapp keepaway              KeepAway game (upstream)");
                let _ = writeln!(term, "  tapp wallpaperctrl         WallPaperCtrl wrapper demo");
                let _ = writeln!(term, "  tapp wallpaperfish         WallPaperFish wallpaper (background)");
                let _ = writeln!(term, "  tapp hc [file.hc]          HolyC runner");
                let _ = writeln!(
                    term,
                    "  tapp run <alias|path>      Run a program from TempleOS tree"
                );
                let _ = writeln!(term, "  tapp search <text>         Search TempleOS tree");
                let _ = writeln!(term, "");

                let Some(root) = discover_templeos_root() else {
                    let _ = writeln!(term, "TempleOS tree: (not found)");
                    let _ = writeln!(
                        term,
                        "Hint: ensure third_party/TempleOS is near the binary, or set TEMPLEOS_ROOT."
                    );
                    return;
                };

                let programs = discover_templeos_programs(&root);
                let _ = writeln!(term, "TempleOS programs ({}):", programs.len());
                for p in programs.iter().take(400) {
                    let _ = writeln!(term, "  {:<18}  {}", p.alias, p.rel_no_ext);
                }
                if programs.len() > 400 {
                    let _ = writeln!(term, "  ... (truncated; use tapp search <text>)");
                }
                return;
            }
            Some((&"kill", _)) => {
                if let Some(child) = self.tapp_child.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let _ = writeln!(term, "tapp: already exited: {status}");
                            self.tapp_child = None;
                            self.tapp_connected = false;
                            return;
                        }
                        Ok(None) => {}
                        Err(err) => {
                            let _ = writeln!(term, "tapp: child status: {err}");
                        }
                    }

                    if let Err(err) = child.kill() {
                        let _ = writeln!(term, "tapp: kill: {err}");
                    } else {
                        let _ = writeln!(term, "tapp: kill: sent");
                    }
                    let _ = child.wait();
                    self.tapp_child = None;
                    self.tapp_connected = false;
                    return;
                }

                if self.tapp_connected {
                    let _ = writeln!(term, "tapp: no tracked child pid to kill");
                    let _ = writeln!(term, "tapp: (app may have been launched externally)");
                } else {
                    let _ = writeln!(term, "tapp: no running app");
                }
                return;
            }
            Some((&"restart", rest)) => {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: restart takes no args");
                    return;
                }
                // Best-effort: kill tracked child first.
                if self.tapp_child.is_some() {
                    self.cmd_tapp(&["kill"], term);
                }
                let last = self.tapp_last.clone().unwrap_or_else(|| TappLaunch {
                    program: "demo".to_string(),
                    args: Vec::new(),
                });
                let args: Vec<&str> = std::iter::once(last.program.as_str())
                    .chain(last.args.iter().map(|s| s.as_str()))
                    .collect();
                self.cmd_tapp(&args, term);
                return;
            }
            _ => {}
        }

        let sock = match std::env::var("TEMPLE_SOCK") {
            Ok(v) => v,
            Err(_) => {
                let _ = writeln!(term, "tapp: TEMPLE_SOCK is not set");
                return;
            }
        };

        let demo_program = || {
            std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    let candidate = exe.with_file_name("temple-demo");
                    candidate.exists().then_some(candidate)
                })
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "temple-demo".to_string())
        };

        let hc_program = || {
            std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    let candidate = exe.with_file_name("temple-hc");
                    candidate.exists().then_some(candidate)
                })
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "temple-hc".to_string())
        };

        let paint_program = || {
            std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    let candidate = exe.with_file_name("temple-paint");
                    candidate.exists().then_some(candidate)
                })
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "temple-paint".to_string())
        };

        let edit_program = || {
            std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    let candidate = exe.with_file_name("temple-edit");
                    candidate.exists().then_some(candidate)
                })
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "temple-edit".to_string())
        };

        if let Some((&sub, rest)) = args.split_first() {
            if sub == "linuxbridge" || sub == "bridge" {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: {sub} takes no args");
                    return;
                }

                let apps_dir = self.root_dir.join("Apps");
                let _ = std::fs::create_dir_all(&apps_dir);
                let linuxbridge_path = apps_dir.join("LinuxBridge.HC");
                if !linuxbridge_path.exists() {
                    let _ = std::fs::write(&linuxbridge_path, TEMPLELINUX_LINUXBRIDGE_HC);
                }
                if !linuxbridge_path.exists() {
                    let _ = writeln!(
                        term,
                        "tapp: linuxbridge: failed to create {}",
                        linuxbridge_path.display()
                    );
                    return;
                }

                let host_path_str = linuxbridge_path.to_string_lossy().to_string();
                let host_cwd = self.cwd.to_host_path(&self.root_dir);

                let program = hc_program();
                let mut cmd = std::process::Command::new(&program);
                cmd.arg(&host_path_str);
                for (k, v) in &self.vars {
                    cmd.env(k, v);
                }
                cmd.env("TEMPLE_SOCK", &sock)
                    .env("TEMPLE_ROOT", self.root_dir.as_os_str())
                    .current_dir(host_cwd);
                if let Some(root) = discover_templeos_root() {
                    cmd.env("TEMPLEOS_ROOT", root.as_os_str());
                }

                match cmd.spawn() {
                    Ok(child) => {
                        self.queue_window_title("LinuxBridge".to_string());
                        self.tapp_last = Some(TappLaunch {
                            program: "linuxbridge".to_string(),
                            args: Vec::new(),
                        });
                        self.tapp_child = Some(child);
                        let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                        let _ = writeln!(term, "tapp: launched pid {pid}");
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: {program}: {err}");
                    }
                }
                return;
            }

            if sub == "timeclock" || sub == "clock" {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: {sub} takes no args");
                    return;
                }

                let apps_dir = self.root_dir.join("Apps");
                let _ = std::fs::create_dir_all(&apps_dir);
                let timeclock_path = apps_dir.join("TimeClock.HC");
                if !timeclock_path.exists() {
                    let _ = std::fs::write(&timeclock_path, TEMPLELINUX_TIMECLOCK_HC);
                }
                if !timeclock_path.exists() {
                    let _ = writeln!(
                        term,
                        "tapp: timeclock: failed to create {}",
                        timeclock_path.display()
                    );
                    return;
                }

                let host_path_str = timeclock_path.to_string_lossy().to_string();
                let host_cwd = self.cwd.to_host_path(&self.root_dir);

                let program = hc_program();
                let mut cmd = std::process::Command::new(&program);
                cmd.arg(&host_path_str);
                for (k, v) in &self.vars {
                    cmd.env(k, v);
                }
                cmd.env("TEMPLE_SOCK", &sock)
                    .env("TEMPLE_ROOT", self.root_dir.as_os_str())
                    .current_dir(host_cwd);
                if let Some(root) = discover_templeos_root() {
                    cmd.env("TEMPLEOS_ROOT", root.as_os_str());
                }

                match cmd.spawn() {
                    Ok(child) => {
                        self.queue_window_title("TimeClock".to_string());
                        self.tapp_last = Some(TappLaunch {
                            program: "timeclock".to_string(),
                            args: Vec::new(),
                        });
                        self.tapp_child = Some(child);
                        let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                        let _ = writeln!(term, "tapp: launched pid {pid}");
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: {program}: {err}");
                    }
                }
                return;
            }

            if sub == "sounddemo" || sub == "sound" || sub == "snddemo" {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: {sub} takes no args");
                    return;
                }

                let apps_dir = self.root_dir.join("Apps");
                let _ = std::fs::create_dir_all(&apps_dir);
                let sounddemo_path = apps_dir.join("SoundDemo.HC");
                if !sounddemo_path.exists() {
                    let _ = std::fs::write(&sounddemo_path, TEMPLELINUX_SOUNDDEMO_HC);
                }
                if !sounddemo_path.exists() {
                    let _ = writeln!(
                        term,
                        "tapp: sounddemo: failed to create {}",
                        sounddemo_path.display()
                    );
                    return;
                }

                let host_path_str = sounddemo_path.to_string_lossy().to_string();
                let host_cwd = self.cwd.to_host_path(&self.root_dir);

                let program = hc_program();
                let mut cmd = std::process::Command::new(&program);
                cmd.arg(&host_path_str);
                for (k, v) in &self.vars {
                    cmd.env(k, v);
                }
                cmd.env("TEMPLE_SOCK", &sock)
                    .env("TEMPLE_ROOT", self.root_dir.as_os_str())
                    .current_dir(host_cwd);
                if let Some(root) = discover_templeos_root() {
                    cmd.env("TEMPLEOS_ROOT", root.as_os_str());
                }

                match cmd.spawn() {
                    Ok(child) => {
                        self.queue_window_title("SoundDemo".to_string());
                        self.tapp_last = Some(TappLaunch {
                            program: "sounddemo".to_string(),
                            args: Vec::new(),
                        });
                        self.tapp_child = Some(child);
                        let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                        let _ = writeln!(term, "tapp: launched pid {pid}");
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: {program}: {err}");
                    }
                }
                return;
            }

            if sub == "logic" {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: {sub} takes no args");
                    return;
                }

                let host_cwd = self.cwd.to_host_path(&self.root_dir);
                let program = hc_program();
                let mut cmd = std::process::Command::new(&program);
                cmd.arg("::/Apps/Logic/Run.HC");
                for (k, v) in &self.vars {
                    cmd.env(k, v);
                }
                cmd.env("TEMPLE_SOCK", &sock)
                    .env("TEMPLE_ROOT", self.root_dir.as_os_str())
                    .current_dir(host_cwd);
                if let Some(root) = discover_templeos_root() {
                    cmd.env("TEMPLEOS_ROOT", root.as_os_str());
                }

                match cmd.spawn() {
                    Ok(child) => {
                        self.queue_window_title("Logic".to_string());
                        self.tapp_last = Some(TappLaunch {
                            program: "logic".to_string(),
                            args: Vec::new(),
                        });
                        self.tapp_child = Some(child);
                        let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                        let _ = writeln!(term, "tapp: launched pid {pid}");
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: {program}: {err}");
                    }
                }
                return;
            }

            if sub == "keepaway" || sub == "ka" {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: {sub} takes no args");
                    return;
                }

                let host_cwd = self.cwd.to_host_path(&self.root_dir);
                let program = hc_program();
                let mut cmd = std::process::Command::new(&program);
                cmd.arg("::/Apps/KeepAway/Run.HC");
                for (k, v) in &self.vars {
                    cmd.env(k, v);
                }
                cmd.env("TEMPLE_SOCK", &sock)
                    .env("TEMPLE_ROOT", self.root_dir.as_os_str())
                    .current_dir(host_cwd);
                if let Some(root) = discover_templeos_root() {
                    cmd.env("TEMPLEOS_ROOT", root.as_os_str());
                }

                match cmd.spawn() {
                    Ok(child) => {
                        self.queue_window_title("KeepAway".to_string());
                        self.tapp_last = Some(TappLaunch {
                            program: "keepaway".to_string(),
                            args: Vec::new(),
                        });
                        self.tapp_child = Some(child);
                        let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                        let _ = writeln!(term, "tapp: launched pid {pid}");
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: {program}: {err}");
                    }
                }
                return;
            }

            if sub == "wallpaperctrl" || sub == "wallctrl" {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: {sub} takes no args");
                    return;
                }

                let apps_dir = self.root_dir.join("Apps");
                let _ = std::fs::create_dir_all(&apps_dir);
                let wallctrl_path = apps_dir.join("WallPaperCtrl.HC");
                if !wallctrl_path.exists() {
                    let _ = std::fs::write(&wallctrl_path, TEMPLELINUX_WALLPAPERCTRL_HC);
                }
                if !wallctrl_path.exists() {
                    let _ = writeln!(
                        term,
                        "tapp: wallpaperctrl: failed to create {}",
                        wallctrl_path.display()
                    );
                    return;
                }

                let host_path_str = wallctrl_path.to_string_lossy().to_string();
                let host_cwd = self.cwd.to_host_path(&self.root_dir);

                let program = hc_program();
                let mut cmd = std::process::Command::new(&program);
                cmd.arg(&host_path_str);
                for (k, v) in &self.vars {
                    cmd.env(k, v);
                }
                cmd.env("TEMPLE_SOCK", &sock)
                    .env("TEMPLE_ROOT", self.root_dir.as_os_str())
                    .current_dir(host_cwd);
                if let Some(root) = discover_templeos_root() {
                    cmd.env("TEMPLEOS_ROOT", root.as_os_str());
                }

                match cmd.spawn() {
                    Ok(child) => {
                        self.queue_window_title("WallPaperCtrl".to_string());
                        self.tapp_last = Some(TappLaunch {
                            program: "wallpaperctrl".to_string(),
                            args: Vec::new(),
                        });
                        self.tapp_child = Some(child);
                        let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                        let _ = writeln!(term, "tapp: launched pid {pid}");
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: {program}: {err}");
                    }
                }
                return;
            }

            if sub == "wallpaperfish" || sub == "wallfish" {
                if !rest.is_empty() {
                    let _ = writeln!(term, "tapp: {sub} takes no args");
                    return;
                }

                let apps_dir = self.root_dir.join("Apps");
                let _ = std::fs::create_dir_all(&apps_dir);
                let wallfish_path = apps_dir.join("WallPaperFish.HC");
                if !wallfish_path.exists() {
                    let _ = std::fs::write(&wallfish_path, TEMPLELINUX_WALLPAPERFISH_HC);
                }
                if !wallfish_path.exists() {
                    let _ = writeln!(
                        term,
                        "tapp: wallpaperfish: failed to create {}",
                        wallfish_path.display()
                    );
                    return;
                }

                let host_path_str = wallfish_path.to_string_lossy().to_string();
                let host_cwd = self.cwd.to_host_path(&self.root_dir);

                let program = hc_program();
                let mut cmd = std::process::Command::new(&program);
                cmd.arg(&host_path_str);
                for (k, v) in &self.vars {
                    cmd.env(k, v);
                }
                cmd.env("TEMPLE_SOCK", &sock)
                    .env("TEMPLE_ROOT", self.root_dir.as_os_str())
                    .current_dir(host_cwd);
                if let Some(root) = discover_templeos_root() {
                    cmd.env("TEMPLEOS_ROOT", root.as_os_str());
                }

                match cmd.spawn() {
                    Ok(child) => {
                        self.queue_wallpaper_title("WallPaperFish".to_string());
                        self.tapp_last = Some(TappLaunch {
                            program: "wallpaperfish".to_string(),
                            args: Vec::new(),
                        });
                        self.tapp_child = Some(child);
                        let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                        let _ = writeln!(term, "tapp: launched pid {pid}");
                    }
                    Err(err) => {
                        let _ = writeln!(term, "tapp: {program}: {err}");
                    }
                }
                return;
            }
        }

        let (program, extra_args) = match args.split_first() {
            None => (demo_program(), &[][..]),
            Some((&"demo", rest)) => (demo_program(), rest),
            Some((&"hc", rest)) => (hc_program(), rest),
            Some((&"holyc", rest)) => (hc_program(), rest),
            Some((&"paint", rest)) => (paint_program(), rest),
            Some((&"edit", rest)) => (edit_program(), rest),
            Some((&"editor", rest)) => (edit_program(), rest),
            Some((&program, rest)) => (program.to_string(), rest),
        };

        let title = match args.split_first() {
            None => "Demo".to_string(),
            Some((&"demo", _)) => "Demo".to_string(),
            Some((&"paint", _)) => "Paint".to_string(),
            Some((&"hc", rest)) | Some((&"holyc", rest)) => rest
                .first()
                .map(|p| format!("HolyC {}", Path::new(p).display()))
                .unwrap_or_else(|| "HolyC".to_string()),
            Some((&"edit", rest)) | Some((&"editor", rest)) => rest
                .first()
                .map(|p| format!("Edit {}", Path::new(p).display()))
                .unwrap_or_else(|| "Edit".to_string()),
            Some((&other, _)) => Path::new(other)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| other.to_string()),
        };

        let host_cwd = self.cwd.to_host_path(&self.root_dir);
        let mut cmd = std::process::Command::new(&program);
        cmd.args(extra_args);
        for (k, v) in &self.vars {
            cmd.env(k, v);
        }
        cmd.env("TEMPLE_SOCK", sock)
            .env("TEMPLE_ROOT", self.root_dir.as_os_str())
            .current_dir(host_cwd);
        if let Some(root) = discover_templeos_root() {
            cmd.env("TEMPLEOS_ROOT", root.as_os_str());
        }
        let res = cmd.spawn();

        match res {
            Ok(child) => {
                self.queue_window_title(title);
                self.tapp_last = Some(TappLaunch {
                    program: program.clone(),
                    args: extra_args.iter().map(|s| s.to_string()).collect(),
                });
                self.tapp_child = Some(child);
                let pid = self.tapp_child.as_ref().map(|c| c.id()).unwrap_or(0);
                let _ = writeln!(term, "tapp: launched pid {pid}");
            }
            Err(err) => {
                let _ = writeln!(term, "tapp: {program}: {err}");
            }
        }
    }

    fn cmd_edit(&mut self, args: &[&str], term: &mut Terminal) {
        use fmt::Write as _;

        let Some(target) = args.first().copied() else {
            let _ = writeln!(term, "edit: expected: edit <path>");
            return;
        };

        if target.starts_with("::/") {
            let Some(root) = discover_templeos_root() else {
                let _ = writeln!(term, "edit: TempleOS tree not found (needed for ::/ paths).");
                let _ = writeln!(
                    term,
                    "Hint: ensure third_party/TempleOS is near the binary, or set TEMPLEOS_ROOT."
                );
                return;
            };

            let mut host = root.join(target.trim_start_matches("::/"));
            if !host.exists() && host.extension().is_none() {
                for ext in ["HC", "HH", "H", "DD"] {
                    let candidate = host.with_extension(ext);
                    if candidate.exists() {
                        host = candidate;
                        break;
                    }
                }
            }
            if !host.exists() {
                let _ = writeln!(term, "edit: not found: {target}");
                return;
            }
            let host_str = host.to_string_lossy().to_string();
            self.cmd_tapp(&["edit", host_str.as_str()], term);
            return;
        }

        let path = self.cwd.resolve(target);
        let host = path.to_host_path(&self.root_dir);
        let host_str = host.to_string_lossy().to_string();

        self.cmd_tapp(&["edit", host_str.as_str()], term);
    }
}
