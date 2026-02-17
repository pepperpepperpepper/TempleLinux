fn default_temple_root() -> PathBuf {
    if let Ok(v) = std::env::var("TEMPLE_ROOT") {
        let v = v.trim();
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".templelinux")
    } else {
        PathBuf::from(".templelinux")
    }
}

fn pick_temple_root(test_mode: bool) -> PathBuf {
    if test_mode {
        if let Ok(v) = std::env::var("TEMPLE_ROOT") {
            let v = v.trim();
            if !v.is_empty() {
                return PathBuf::from(v);
            }
        }

        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        return std::env::temp_dir().join(format!(
            "templelinux-test-root-{uniq}-{}",
            std::process::id()
        ));
    }

    default_temple_root()
}

#[derive(Clone, Debug, Default)]
struct TemplePath {
    components: Vec<String>,
}

impl TemplePath {
    fn root() -> Self {
        Self::default()
    }

    fn resolve(&self, input: &str) -> Self {
        let mut components = if input.starts_with('/') {
            Vec::new()
        } else {
            self.components.clone()
        };

        for part in input.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    components.pop();
                }
                other => components.push(other.to_string()),
            }
        }

        Self { components }
    }

    fn display(&self) -> String {
        if self.components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.components.join("/"))
        }
    }

    fn to_host_path(&self, root_dir: &Path) -> PathBuf {
        let mut path = root_dir.to_path_buf();
        for part in &self.components {
            path.push(part);
        }
        path
    }
}

#[derive(Clone, Debug)]
struct TappLaunch {
    program: String,
    args: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserTab {
    Files,
    Apps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserEntryKind {
    Parent,
    Dir,
    File,
}

#[derive(Clone, Debug)]
struct BrowserEntry {
    name: String,
    kind: BrowserEntryKind,
}

#[derive(Clone, Debug)]
struct FileBrowserState {
    tab: BrowserTab,
    entries: Vec<BrowserEntry>,
    selected: usize,
    scroll: usize,
    msg: String,
}

impl FileBrowserState {
    fn new() -> Self {
        Self {
            tab: BrowserTab::Files,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            msg: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocKind {
    TempleDoc,
    DolDoc,
    PlainText,
}

#[derive(Clone, Debug)]
enum DocLinkTarget {
    Doc(String),
    Action(String),
}

#[derive(Clone, Debug)]
struct DocLink {
    line: usize,
    col_start: usize,
    col_end: usize,
    target: DocLinkTarget,
}

#[derive(Clone, Debug)]
struct DocSprite {
    anchor_line: usize,
    anchor_col: usize,
    bbox_line0: i32,
    bbox_col0: i32,
    bbox_line1: i32,
    bbox_col1: i32,
    bin_num: u32,
    action: Option<String>,
}

#[derive(Clone, Debug)]
struct DocViewerState {
    spec: String,
    kind: DocKind,
    lines: Vec<Vec<Cell>>,
    scroll: usize,
    links: Vec<DocLink>,
    selected_link: Option<usize>,
    selected_sprite: Option<usize>,
    anchors: std::collections::BTreeMap<String, usize>,
    sprites: Vec<DocSprite>,
    bins: std::collections::BTreeMap<u32, Vec<u8>>,
    msg: String,
}

#[derive(Clone, Copy, Debug)]
struct BrowserApp {
    name: &'static str,
    command: &'static str,
    hint: &'static str,
}

const TEMPLELINUX_LINUXBRIDGE_HC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/holyc/LinuxBridge.HC"));
const TEMPLELINUX_TIMECLOCK_HC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/holyc/TimeClock.HC"));
const TEMPLELINUX_WALLPAPERCTRL_HC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/holyc/WallPaperCtrl.HC"));
const TEMPLELINUX_WALLPAPERFISH_HC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/holyc/WallPaperFish.HC"));
const TEMPLELINUX_SOUNDDEMO_HC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/holyc/SoundDemo.HC"));

const TEMPLELINUX_DEFAULT_AUTOSTART_TL: &str = r#"# TempleLinux AutoStart
#
# This file is executed line-by-line on TempleShell startup.
# Edit or delete it to customize your boot experience.
#
# Hotkeys:
#   - F2 opens the launcher (Apps tab) at any time.

# Open TempleOS PersonalMenu icons on boot (TempleOS-like “desktop”).
menu

# Optional: open the launcher on boot (Apps tab).
# apps

# Optional: start a TempleOS wallpaper (requires a discoverable TempleOS tree).
# tapp wallpaperfish
"#;

const BROWSER_APPS: &[BrowserApp] = &[
    BrowserApp {
        name: "LinuxBridge",
        command: "tapp linuxbridge",
        hint: "Open URLs/files/commands",
    },
    BrowserApp {
        name: "TimeClock",
        command: "tapp timeclock",
        hint: "Punch in/out + report",
    },
    BrowserApp {
        name: "SoundDemo",
        command: "tapp sounddemo",
        hint: "Sound output demo",
    },
    BrowserApp {
        name: "WallPaperCtrl",
        command: "tapp wallpaperctrl",
        hint: "Controls demo (wallpaper-style)",
    },
    BrowserApp {
        name: "WallPaperFish",
        command: "tapp wallpaperfish",
        hint: "Animated wallpaper demo",
    },
    BrowserApp {
        name: "Menu",
        command: "menu",
        hint: "TempleOS PersonalMenu icons",
    },
    BrowserApp {
        name: "TempleOS",
        command: "tapp list",
        hint: "List/search (needs TempleOS tree)",
    },
    BrowserApp {
        name: "NetOfDots",
        command: "tapp hc ::/Demo/Graphics/NetOfDots.HC",
        hint: "Graphics demo (needs TempleOS tree)",
    },
    BrowserApp {
        name: "PullDown",
        command: "tapp hc ::/Demo/PullDownMenu.HC",
        hint: "Menu demo (needs TempleOS tree)",
    },
    BrowserApp {
        name: "KeepAway",
        command: "tapp keepaway",
        hint: "Game (needs TempleOS tree)",
    },
    BrowserApp {
        name: "Logic",
        command: "tapp logic",
        hint: "App (needs TempleOS tree)",
    },
    BrowserApp {
        name: "TicTac",
        command: "tapp hc ::/Demo/Games/TicTacToe.HC",
        hint: "Game (needs TempleOS tree)",
    },
    BrowserApp {
        name: "CharDemo",
        command: "tapp hc ::/Demo/Games/CharDemo.HC",
        hint: "Game (needs TempleOS tree)",
    },
    BrowserApp {
        name: "Demo",
        command: "tapp demo",
        hint: "Graphics demo",
    },
    BrowserApp {
        name: "HolyC",
        command: "tapp hc",
        hint: "HolyC runner",
    },
    BrowserApp {
        name: "Paint",
        command: "tapp paint",
        hint: "Pixel paint",
    },
    BrowserApp {
        name: "Editor",
        command: "edit /Home/Notes.txt",
        hint: "Text editor",
    },
];
