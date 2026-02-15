use std::{
    fs,
    os::unix::fs::FileTypeExt as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, MutexGuard, OnceLock},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const GOLDEN_X11_INITIAL_SHA256: &str =
    "dfaa72d70e1959c0aa91d94918d04c7d3cf6a109feaaba7d78945571ae9c119b";
const GOLDEN_X11_FILEBROWSER_SHA256: &str =
    "cddff240d6ab92f08d75664e84c25908dfb6f515b22f783042c46fc92e754aa0";
const GOLDEN_X11_DOLDOC_DEMOINDEX_SHA256: &str =
    "c671403c450fd1c9a8f1ab6beb0bdb84ba8069362f428d36c0a045a59520aa40";
const GOLDEN_X11_DOLDOC_PERSONALMENU_XCALIBER_SHA256: &str =
    "6d5ca3072b6e6211dbd15db2fa08bb5c083b41b2ad4b442a58382cf8752c3b21";
const GOLDEN_X11_NETOFDOTS_SHA256: &str =
    "9ffc8f1c713528d73e64df35acd09a7855251021e653983de11909244b8e6360";
const GOLDEN_X11_LINUXBRIDGE_SHA256: &str =
    "66ca81b67d276b623199350e151bf0205119e42cf739fc7a2a8abb5b10d26d98";
const GOLDEN_X11_EDITOR_SHA256: &str =
    "77810e3eff1cccf296310fdfd9ad83743f8730cd2cd9cd380672475e148bd752";
const GOLDEN_X11_PULLDOWNMENU_SHA256: &str =
    "2f5712b6d302b7d175c00872dbe0fc0ff2446fea1f384926a3fcac624b95c3d0";
const GOLDEN_X11_MULTIWINDOW_SHA256: &str =
    "3527fec04c62af1c378cb3499fe2cc9cc6c48d3a7adb1c65127da1ac57de5c12";

fn should_run_gui_tests() -> bool {
    std::env::var_os("TEMPLE_GUI_TESTS").is_some()
}

fn gui_test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn cargo_profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(|p| p.parent())
        .expect("test exe should live in target/<profile>/deps/")
        .to_path_buf()
}

fn cargo_bin_path(bin_filename: &str) -> PathBuf {
    cargo_profile_dir().join(bin_filename)
}

fn ensure_cargo_bin(manifest_dir: &Path, bin_target: &str, bin_path: &Path) {
    if bin_path.exists() {
        return;
    }

    let status = Command::new("cargo")
        .arg("build")
        .arg("-q")
        .arg("--bin")
        .arg(bin_target)
        .current_dir(manifest_dir)
        .status()
        .expect("spawn cargo build");

    assert!(
        status.success(),
        "cargo build --bin {bin_target} failed: {status}"
    );
    assert!(
        bin_path.exists(),
        "expected cargo bin at {}",
        bin_path.display()
    );
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{now}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn wait_for_unix_socket(path: &Path, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.file_type().is_socket() {
                return;
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for unix socket: {}", path.display());
}

fn wait_for_exit(child: &mut std::process::Child, timeout: Duration) -> std::process::ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return status;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            panic!("timed out waiting for child to exit");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

const XVFB_SCREEN_SPEC: &str = "-screen 0 1280x720x24 -nolisten tcp";

fn xvfb_run_templeshell(manifest_dir: &Path, templeshell: &Path, temple_root: &Path) -> Command {
    let mut cmd = Command::new("xvfb-run");
    cmd.arg("-a")
        .arg("-s")
        .arg(XVFB_SCREEN_SPEC)
        .arg(templeshell)
        .arg("--no-fullscreen")
        .current_dir(manifest_dir)
        .env(
            "__EGL_VENDOR_LIBRARY_FILENAMES",
            "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
        )
        .env("__GLX_VENDOR_LIBRARY_NAME", "mesa")
        .env("LIBGL_ALWAYS_SOFTWARE", "1")
        .env("WGPU_BACKEND", "gl")
        .env("WINIT_UNIX_BACKEND", "x11")
        .env("TEMPLE_SYNC_PRESENT", "1")
        .env("TEMPLE_SYNC_PRESENT_TIMEOUT_MS", "2000")
        .env("TEMPLE_ROOT", temple_root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    cmd
}

fn assert_png_640x480_matches_sha256(png_path: &Path, expected_sha256: &str) {
    let bytes = fs::read(png_path).expect("read dumped png");
    assert!(bytes.len() > 1024, "expected PNG output to be non-trivial");

    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    let reader = decoder.read_info().expect("read png info");
    let info = reader.info();
    assert_eq!(info.width, 640);
    assert_eq!(info.height, 480);

    let actual = sha256_hex(&bytes);
    assert_eq!(
        actual,
        expected_sha256,
        "golden mismatch for {}; update constant if intentional",
        png_path.display()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_initial_frame_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temp = unique_temp_dir("templelinux-gui-golden-initial");
    let temple_root = temp.join("root");
    fs::create_dir_all(&temple_root).expect("create TEMPLE_ROOT");
    let png_path = temp.join("initial.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell.arg("--test-dump-initial-png").arg(&png_path);

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");

    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_INITIAL_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_filebrowser_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temp = unique_temp_dir("templelinux-gui-golden-filebrowser");
    let temple_root = temp.join("root");
    let demo_dir = temple_root.join("Home/Demo");
    fs::create_dir_all(&demo_dir).expect("create /Home/Demo");
    fs::write(demo_dir.join("hello.txt"), "").expect("write hello.txt");
    fs::write(demo_dir.join("Notes.txt"), "").expect("write Notes.txt");
    let png_path = temp.join("filebrowser.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-run-shell")
        .arg("files /Home/Demo")
        .arg("--test-dump-initial-png")
        .arg(&png_path);

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");
    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_FILEBROWSER_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_doldoc_demoindex_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temp = unique_temp_dir("templelinux-gui-golden-doldoc-demoindex");
    let temple_root = temp.join("root");
    fs::create_dir_all(&temple_root).expect("create TEMPLE_ROOT");
    let png_path = temp.join("doldoc-demoindex.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-run-shell")
        .arg("help DemoIndex")
        .arg("--test-dump-initial-png")
        .arg(&png_path);

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");
    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_DOLDOC_DEMOINDEX_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_doldoc_personalmenu_xcaliber_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temp = unique_temp_dir("templelinux-gui-golden-doldoc-personalmenu-xcaliber");
    let temple_root = temp.join("root");
    fs::create_dir_all(&temple_root).expect("create TEMPLE_ROOT");
    let png_path = temp.join("doldoc-personalmenu-xcaliber.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-run-shell")
        .arg("help FF:::/PersonalMenu.DD,X-Caliber")
        .arg("--test-dump-initial-png")
        .arg(&png_path);

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");
    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_DOLDOC_PERSONALMENU_XCALIBER_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_linuxbridge_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temple_hc = cargo_bin_path("temple-hc");
    ensure_cargo_bin(&manifest_dir, "temple-hc", &temple_hc);
    let temp = unique_temp_dir("templelinux-gui-golden-linuxbridge");
    let temple_root = temp.join("root");
    fs::create_dir_all(&temple_root).expect("create TEMPLE_ROOT");
    let png_path = temp.join("linuxbridge.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-run-shell")
        .arg("tapp linuxbridge")
        .arg("--test-dump-app-png")
        .arg(&png_path)
        .arg("--test-app-exit")
        .arg("esc");

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");
    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_LINUXBRIDGE_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_editor_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temple_edit = cargo_bin_path("temple-edit");
    ensure_cargo_bin(&manifest_dir, "temple-edit", &temple_edit);
    let temp = unique_temp_dir("templelinux-gui-golden-editor");
    let temple_root = temp.join("root");
    let home_dir = temple_root.join("Home");
    fs::create_dir_all(&home_dir).expect("create /Home");
    fs::write(
        home_dir.join("EditorDemo.txt"),
        "// Editor golden test\nGrLine\n",
    )
    .expect("write EditorDemo.txt");
    let png_path = temp.join("editor.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-run-shell")
        .arg("tapp edit /Home/EditorDemo.txt")
        .arg("--test-dump-app-png")
        .arg(&png_path)
        .arg("--test-app-exit")
        .arg("ctrlq");

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");
    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_EDITOR_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_netofdots_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    let temple_hc = cargo_bin_path("temple-hc");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    ensure_cargo_bin(&manifest_dir, "temple-hc", &temple_hc);
    let temp = unique_temp_dir("templelinux-gui-smoke");
    let sock = temp.join("templeshell.sock");
    let temple_root = temp.join("root");
    fs::create_dir_all(&temple_root).expect("create TEMPLE_ROOT");
    let png_path = temp.join("frame.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-dump-app-png")
        .arg(&png_path)
        .env("TEMPLE_SOCK", &sock);

    let mut shell_child = match shell.spawn() {
        Ok(child) => child,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI smoke test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };

    wait_for_unix_socket(&sock, Duration::from_secs(10));

    let status = Command::new(&temple_hc)
        .arg("::/Demo/Graphics/NetOfDots.HC")
        .current_dir(&manifest_dir)
        .env("TEMPLE_SOCK", &sock)
        .env("TEMPLE_ROOT", &temple_root)
        .env("TEMPLE_SYNC_PRESENT", "1")
        .env("TEMPLE_SYNC_PRESENT_TIMEOUT_MS", "2000")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("run temple-hc");
    assert!(status.success(), "temple-hc failed: {status}");

    let shell_status = wait_for_exit(&mut shell_child, Duration::from_secs(10));
    assert!(shell_status.success(), "templeshell failed: {shell_status}");

    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_NETOFDOTS_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_pulldownmenu_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temp = unique_temp_dir("templelinux-gui-golden-pulldownmenu");
    let temple_root = temp.join("root");
    fs::create_dir_all(&temple_root).expect("create TEMPLE_ROOT");
    let png_path = temp.join("pulldownmenu.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-run-shell")
        .arg("tapp hc ::/Demo/PullDownMenu.HC")
        .arg("--test-send-after-first-app-present")
        .arg("mouse_move:1,0")
        .arg("--test-dump-after-n-presents-png")
        .arg("20")
        .arg(&png_path)
        .arg("--test-app-exit")
        .arg("esc");

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");
    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_PULLDOWNMENU_SHA256);
}

#[test]
#[cfg(target_os = "linux")]
fn gui_smoke_x11_multiwindow_matches_golden_sha() {
    if !should_run_gui_tests() {
        eprintln!("skipping GUI golden test (set TEMPLE_GUI_TESTS=1 to enable)");
        return;
    }
    let _lock = gui_test_lock();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let templeshell = cargo_bin_path("templeshell");
    ensure_cargo_bin(&manifest_dir, "templeshell", &templeshell);
    let temp = unique_temp_dir("templelinux-gui-golden-multiwindow");
    let temple_root = temp.join("root");
    fs::create_dir_all(&temple_root).expect("create TEMPLE_ROOT");
    let png_path = temp.join("multiwindow.png");

    let mut shell = xvfb_run_templeshell(&manifest_dir, &templeshell, &temple_root);
    shell
        .arg("--test-run-shell")
        .arg("tapp paint")
        .arg("--test-run-shell")
        .arg("tapp demo")
        .arg("--test-dump-after-n-apps-present-png")
        .arg("2")
        .arg(&png_path);

    let status = match shell.status() {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping GUI golden test (xvfb-run not found)");
            return;
        }
        Err(err) => panic!("spawn xvfb-run: {err}"),
    };
    assert!(status.success(), "templeshell failed: {status}");
    assert_png_640x480_matches_sha256(&png_path, GOLDEN_X11_MULTIWINDOW_SHA256);
}
