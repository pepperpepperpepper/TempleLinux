use super::*;
use super::{
    fmt::{CDATE_FREQ_HZ, format_temple_fmt},
    preprocess::{
        SourceSegment, builtin_defines, compile_segments, preprocess_entry, resolve_templeos_path,
    },
    vm,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        ffi::CString,
        os::unix::{io::AsRawFd as _, net::UnixListener},
        path::PathBuf,
        sync::{Arc, Mutex, OnceLock},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use temple_rt::protocol;
    use temple_rt::rt::TempleRt;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    struct FakeShellResult {
        fb: Vec<u8>,
        presents: u32,
        first_present_fb: Option<Vec<u8>>,
        captured_present_fb: Option<Vec<u8>>,
        snd_msgs: u32,
        last_snd_ona: Option<u8>,
        mute_msgs: u32,
        is_muted: bool,
    }

    fn spawn_fake_shell(
        socket_path: PathBuf,
        width: u32,
        height: u32,
        outgoing: Vec<protocol::Msg>,
        send_after_first_present: bool,
        capture_present: Option<u32>,
    ) -> thread::JoinHandle<FakeShellResult> {
        thread::spawn(move || {
            use std::io::Read as _;

            let _ = std::fs::remove_file(&socket_path);
            let _ = std::fs::remove_dir_all(&socket_path);
            let listener = UnixListener::bind(&socket_path).expect("bind fake TEMPLE_SOCK");
            let (mut stream, _) = listener.accept().expect("accept fake temple client");

            let hello = protocol::read_msg(&mut stream).expect("read hello");
            assert_eq!(hello.kind, protocol::MSG_HELLO);

            let shm = (|| -> std::io::Result<std::fs::File> {
                use nix::sys::memfd::{MemFdCreateFlag, memfd_create};

                let name = CString::new("temple-fb-test").expect("CString");
                let fd = memfd_create(name.as_c_str(), MemFdCreateFlag::MFD_CLOEXEC)
                    .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
                let file: std::fs::File = fd.into();
                file.set_len((width * height) as u64)?;
                Ok(file)
            })()
            .expect("create memfd");

            protocol::send_msg_with_fd(
                &stream,
                protocol::Msg::hello_ack(width, height),
                shm.as_raw_fd(),
            )
            .expect("send hello_ack");

            let len = (width * height) as usize;
            let map = unsafe { memmap2::MmapOptions::new().len(len).map(&shm) }.expect("mmap shm");

            let mut presents = 0u32;
            let mut first_present_fb: Option<Vec<u8>> = None;
            let mut captured_present_fb: Option<Vec<u8>> = None;
            let mut snd_msgs = 0u32;
            let mut last_snd_ona: Option<u8> = None;
            let mut mute_msgs = 0u32;
            let mut is_muted = false;
            let mut outgoing = Some(outgoing);

            if !send_after_first_present {
                for msg in outgoing.take().unwrap_or_default() {
                    protocol::write_msg(&mut stream, msg).expect("send fake event");
                }
                protocol::write_msg(&mut stream, protocol::Msg::shutdown()).expect("send shutdown");
            }

            loop {
                match protocol::read_msg(&mut stream) {
                    Ok(msg) => {
                        if msg.kind == protocol::MSG_PRESENT {
                            presents = presents.wrapping_add(1);
                            if first_present_fb.is_none() {
                                first_present_fb = Some(map.to_vec());
                            }
                            if captured_present_fb.is_none()
                                && capture_present.is_some_and(|want| want == presents)
                            {
                                captured_present_fb = Some(map.to_vec());
                            }

                            if send_after_first_present && outgoing.is_some() {
                                for msg in outgoing.take().unwrap_or_default() {
                                    protocol::write_msg(&mut stream, msg).expect("send fake event");
                                }
                                protocol::write_msg(&mut stream, protocol::Msg::shutdown())
                                    .expect("send shutdown");
                            }
                        } else if msg.kind == protocol::MSG_SND {
                            snd_msgs = snd_msgs.wrapping_add(1);
                            last_snd_ona = Some(msg.a as u8);
                        } else if msg.kind == protocol::MSG_MUTE {
                            mute_msgs = mute_msgs.wrapping_add(1);
                            is_muted = msg.a != 0;
                        } else if msg.kind == protocol::MSG_CLIPBOARD_SET {
                            let mut remaining = msg.a as usize;
                            let mut buf = [0u8; 4096];
                            while remaining > 0 {
                                let to_read = remaining.min(buf.len());
                                if stream.read_exact(&mut buf[..to_read]).is_err() {
                                    break;
                                }
                                remaining = remaining.saturating_sub(to_read);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            FakeShellResult {
                fb: map.to_vec(),
                presents,
                first_present_fb,
                captured_present_fb,
                snd_msgs,
                last_snd_ona,
                mute_msgs,
                is_muted,
            }
        })
    }

    fn run_over_fake_shell_capture_with_events_impl(
        spec: &str,
        outgoing: Vec<protocol::Msg>,
        send_after_first_present: bool,
        capture_present: Option<u32>,
    ) -> (String, FakeShellResult) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let templeos_root = root.join("third_party/TempleOS");

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let socket_path =
            std::env::temp_dir().join(format!("templesock-test-{uniq}-{}", std::process::id()));
        let server = spawn_fake_shell(
            socket_path.clone(),
            640,
            480,
            outgoing,
            send_after_first_present,
            capture_present,
        );

        let old_sock = std::env::var("TEMPLE_SOCK").ok();
        let old_root = std::env::var("TEMPLE_ROOT").ok();
        let old_tos = std::env::var("TEMPLEOS_ROOT").ok();
        let mut created_root: Option<PathBuf> = None;
        unsafe {
            std::env::set_var("TEMPLE_SOCK", &socket_path);
        }

        if old_root.is_none() {
            let dir =
                std::env::temp_dir().join(format!("templeroot-test-{uniq}-{}", std::process::id()));
            std::fs::create_dir_all(dir.join("Home")).expect("create temp TEMPLE_ROOT/Home");
            unsafe {
                std::env::set_var("TEMPLE_ROOT", &dir);
            }
            created_root = Some(dir);
        }

        if old_tos.is_none() {
            unsafe {
                std::env::set_var("TEMPLEOS_ROOT", &templeos_root);
            }
        }

        let entry =
            resolve_templeos_path(spec, &root, Some(&templeos_root)).expect("resolve entry");
        let (segments, defines, bins_by_file) =
            preprocess_entry(&entry, Some(&templeos_root)).expect("preprocess");
        let mut macros = builtin_defines();
        macros.extend(defines);
        let macros = Arc::new(macros);
        let program = compile_segments(segments, macros.clone(), bins_by_file).expect("compile");

        let rt = (|| {
            let mut last_err: Option<std::io::Error> = None;
            for _ in 0..2000 {
                match TempleRt::connect() {
                    Ok(rt) => return rt,
                    Err(err) => {
                        last_err = Some(err);
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            }
            panic!("connect to fake TEMPLE_SOCK: {last_err:?}");
        })();
        let mut vm = vm::Vm::new(rt, program, macros);
        vm.enable_capture();
        vm.run().expect("run program");
        let out = vm.captured_output().expect("capture enabled").to_string();
        drop(vm);

        match old_sock {
            Some(v) => unsafe { std::env::set_var("TEMPLE_SOCK", v) },
            None => unsafe { std::env::remove_var("TEMPLE_SOCK") },
        }
        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }
        match old_tos {
            Some(v) => unsafe { std::env::set_var("TEMPLEOS_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLEOS_ROOT") },
        }

        let res = server.join().expect("join fake shell thread");
        let _ = std::fs::remove_file(&socket_path);
        if let Some(dir) = created_root {
            let _ = std::fs::remove_dir_all(dir);
        }

        (out, res)
    }

    fn run_over_fake_shell_capture_with_events(
        spec: &str,
        outgoing: Vec<protocol::Msg>,
    ) -> (String, FakeShellResult) {
        run_over_fake_shell_capture_with_events_impl(spec, outgoing, false, None)
    }

    fn run_over_fake_shell_capture_with_events_after_first_present(
        spec: &str,
        outgoing: Vec<protocol::Msg>,
    ) -> (String, FakeShellResult) {
        run_over_fake_shell_capture_with_events_impl(spec, outgoing, true, None)
    }

    fn run_over_fake_shell_capture(spec: &str) -> (String, FakeShellResult) {
        run_over_fake_shell_capture_with_events(spec, vec![])
    }

    #[test]
    fn parse_templeos_demo_print_hc() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let path = root.join("third_party/TempleOS/Demo/Print.HC");
        let bytes = std::fs::read(&path).expect("read Print.HC");
        let cutoff = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let text = bytes[..cutoff].to_vec();
        let segments = vec![SourceSegment {
            file: path.display().to_string().into(),
            start_line: 1,
            bytes: text,
        }];
        let program = compile_segments(segments, Arc::new(builtin_defines()), HashMap::new())
            .expect("parse Print.HC");
        assert!(!program.top_level.is_empty());
    }

    #[test]
    fn preprocess_preserves_cp437_bytes_in_char_literals() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let pid = std::process::id();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        let path = std::env::temp_dir().join(format!("templelinux-cp437-{pid}-{ts}.HC"));

        // CP437 0xC4 is '─' (single-line horizontal box drawing). Many TempleOS sources use raw
        // CP437 bytes in char literals, e.g. `::/Kernel/KMain.HC` border chars.
        let mut bytes = b"U0 Main(){I64 x='".to_vec();
        bytes.push(0xC4);
        bytes.extend_from_slice(b"';}\nMain;\n");

        std::fs::write(&path, &bytes).expect("write temp HolyC file");
        let (segments, _defines, _bins_by_file) =
            preprocess_entry(&path, None).expect("preprocess temp HolyC file");

        let mut found: Option<u64> = None;
        for seg in segments {
            let mut lex = Lexer::new(
                seg.file.clone(),
                seg.bytes.as_slice(),
                seg.start_line,
                Arc::new(HashMap::new()),
            );
            loop {
                let t = lex.next_token().expect("lex token");
                match t.kind {
                    TokenKind::Char(v) => {
                        found = Some(v);
                        break;
                    }
                    TokenKind::Eof => break,
                    _ => {}
                }
            }
            if found.is_some() {
                break;
            }
        }

        let _ = std::fs::remove_file(&path);
        assert_eq!(found, Some(0xC4));
    }

    #[test]
    fn format_repeat_char_with_aux_number() {
        let out = format_temple_fmt("%h5c", &[vm::Value::Char('x' as u64)]).unwrap();
        assert_eq!(out, "xxxxx");
    }

    #[test]
    fn format_repeat_char_with_aux_from_arg() {
        let out =
            format_temple_fmt("%h*c", &[vm::Value::Int(3), vm::Value::Char('y' as u64)]).unwrap();
        assert_eq!(out, "yyy");
    }

    #[test]
    fn format_commas_via_aux_or_flag() {
        let out_aux = format_temple_fmt("%h?d", &[vm::Value::Int(123456789)]).unwrap();
        let out_flag = format_temple_fmt("%,d", &[vm::Value::Int(123456789)]).unwrap();
        assert_eq!(out_aux, "123,456,789");
        assert_eq!(out_flag, "123,456,789");
    }

    #[test]
    fn format_list_item_with_z() {
        let list = "NULL\0OUTPUT\0INPUT\0NOT\0AND\0".to_string();
        let out = format_temple_fmt("%z", &[vm::Value::Int(3), vm::Value::Str(list)]).unwrap();
        assert_eq!(out, "NOT");
    }

    #[test]
    fn format_engineering_suffix() {
        let out_milli = format_temple_fmt("%h?n", &[vm::Value::Float(0.0012)]).unwrap();
        assert!(out_milli.ends_with('m'));

        let out_kilo = format_temple_fmt("%h?n", &[vm::Value::Float(12_345.0)]).unwrap();
        assert!(out_kilo.ends_with('k'));
    }

    #[test]
    fn run_enum_smoke_defines_values() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("templehc-enum-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("enum.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  enum { A, B=5, C, D=(C+10), E };
  "A=%d B=%d C=%d D=%d E=%d\n", A, B, C, D, E;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "A=0 B=5 C=6 D=16 E=17\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preprocess_includes_from_templeos_tree() {
        let _guard = env_guard();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let templeos_root = root.join("third_party/TempleOS");

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("templehc-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let entry = dir.join("main.HC");
        std::fs::write(&entry, "#include \"::/Kernel/FontStd.HC\"\n\"ok\\n\";\n").unwrap();

        let (segments, _defines, _bins_by_file) =
            preprocess_entry(&entry, Some(&templeos_root)).unwrap();
        assert!(
            segments
                .iter()
                .any(|seg| seg.file.as_ref().ends_with("Kernel/FontStd.HC")),
            "expected included FontStd.HC segment"
        );
        assert!(
            segments
                .iter()
                .any(|seg| seg.file.as_ref().ends_with("main.HC")),
            "expected entry file segment"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lexer_builtin_dir_and_file_macros_expand_to_temple_paths() {
        let _guard = env_guard();

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let templeos_root = root.join("third_party/TempleOS");

        let old_tos = std::env::var("TEMPLEOS_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLEOS_ROOT", &templeos_root);
        }

        let file = templeos_root.join("Apps/Logic/Run.HC");
        let file = std::fs::canonicalize(&file).unwrap_or(file);
        let file_label: Arc<str> = file.display().to_string().into();

        let macros = Arc::new(builtin_defines());

        let mut lex_dir = Lexer::new(file_label.clone(), b"__DIR__", 1, macros.clone());
        let t = lex_dir.next_token().expect("next_token");
        match t.kind {
            TokenKind::Str(s) => assert_eq!(s, "/Apps/Logic"),
            other => panic!("expected __DIR__ to lex as Str, got {other:?}"),
        }

        let mut lex_file = Lexer::new(file_label.clone(), b"__FILE__", 1, macros);
        let t = lex_file.next_token().expect("next_token");
        match t.kind {
            TokenKind::Str(s) => assert_eq!(s, "/Apps/Logic/Run.HC"),
            other => panic!("expected __FILE__ to lex as Str, got {other:?}"),
        }

        match old_tos {
            Some(v) => unsafe { std::env::set_var("TEMPLEOS_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLEOS_ROOT") },
        }
    }

    #[test]
    fn cd_dir_macro_sets_cwd_and_relative_filefind_uses_it() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("templehc-cd-dir-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Home/DirTest")).unwrap();
        std::fs::write(dir.join("Home/DirTest/data.txt"), b"ok\n").unwrap();

        let entry = dir.join("Home/DirTest/Main.HC");
        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 ok_cd = Cd(__DIR__);
  I64 ok = FileFind("data.txt");
  "%d %d\n", ok_cd, ok;
}
Main;
"#,
        )
        .unwrap();

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let templeos_root = root.join("third_party/TempleOS");

        let old_root = std::env::var("TEMPLE_ROOT").ok();
        let old_tos = std::env::var("TEMPLEOS_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLE_ROOT", &dir);
            std::env::set_var("TEMPLEOS_ROOT", &templeos_root);
        }

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "1 1\n");

        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }
        match old_tos {
            Some(v) => unsafe { std::env::set_var("TEMPLEOS_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLEOS_ROOT") },
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cd_can_enter_templeos_dirs_and_relative_filefind_works() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("templehc-cd-tos-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Home")).unwrap();

        let entry = dir.join("Home/cd_tos.HC");
        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 ok_cd = Cd("/Apps/Logic");
  I64 ok = FileFind("Run.HC");
  "%d %d\n", ok_cd, ok;
}
Main;
"#,
        )
        .unwrap();

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let templeos_root = root.join("third_party/TempleOS");

        let old_root = std::env::var("TEMPLE_ROOT").ok();
        let old_tos = std::env::var("TEMPLEOS_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLE_ROOT", &dir);
            std::env::set_var("TEMPLEOS_ROOT", &templeos_root);
        }

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "1 1\n");

        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }
        match old_tos {
            Some(v) => unsafe { std::env::set_var("TEMPLEOS_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLEOS_ROOT") },
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_templeos_demo_print_hc_end_to_end_over_ipc() {
        let _guard = env_guard();

        let (_out, res) = run_over_fake_shell_capture("::/Demo/Print.HC");
        assert!(res.presents > 0, "expected at least one Present()");
        assert!(
            res.fb.iter().any(|&b| b != 0),
            "expected framebuffer to contain drawn pixels"
        );
        let first = res
            .first_present_fb
            .as_ref()
            .expect("expected first present snapshot");
        assert!(
            first.iter().any(|&b| b != 0),
            "expected first present framebuffer to contain drawn pixels"
        );
    }

    #[test]
    fn run_templeos_demo_nullcase_hc_end_to_end() {
        let _guard = env_guard();

        let (out, _res) = run_over_fake_shell_capture("::/Demo/NullCase.HC");
        assert_eq!(out, "Zero\nOne\nTwo\nThree\nTen\nEleven\n");
    }

    #[test]
    fn run_templeos_demo_subswitch_hc_end_to_end() {
        let _guard = env_guard();

        let (out, _res) = run_over_fake_shell_capture("::/Demo/SubSwitch.HC");
        assert_eq!(out, "Zero [One] Two [Three] Four [Five] \n");
    }

    #[test]
    fn run_templeos_demo_graphics_mousedemo_hc_end_to_end() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture_with_events(
            "::/Demo/Graphics/MouseDemo.HC",
            vec![protocol::Msg::mouse_button(
                protocol::MOUSE_BUTTON_LEFT,
                true,
            )],
        );

        assert_eq!(out, "Press left mouse bttn to exit.\n");
        assert!(
            res.presents >= 3,
            "expected print + Refresh + DCFill presents, got {}",
            res.presents
        );
    }

    #[test]
    fn run_templeos_demo_graphics_netofdots_hc_end_to_end() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture_with_events_after_first_present(
            "::/Demo/Graphics/NetOfDots.HC",
            vec![protocol::Msg::key(b'a' as u32, true)],
        );

        assert_eq!(out, "");
        assert!(
            res.presents >= 2,
            "expected PressAKey + DCFill presents, got {}",
            res.presents
        );
        let first = res
            .first_present_fb
            .as_ref()
            .expect("expected first present snapshot");
        assert!(
            first.iter().any(|&b| b != 0),
            "expected first present framebuffer to contain drawn pixels"
        );
    }

    #[test]
    fn run_templeos_demo_graphics_spriteplot_hc_end_to_end() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture_with_events_impl(
            "::/Demo/Graphics/SpritePlot.HC",
            vec![protocol::Msg::key(b'a' as u32, true)],
            false,
            Some(2),
        );

        assert!(
            out.contains("Image size:"),
            "expected SpritePlot to print image size, got: {out:?}"
        );
        let size = out
            .lines()
            .find_map(|line| line.strip_prefix("Image size:"))
            .and_then(|rest| rest.trim().parse::<i64>().ok())
            .unwrap_or(0);
        assert!(
            size > 0,
            "expected a non-zero sprite bin size, got: {out:?}"
        );

        let fb = res
            .captured_present_fb
            .as_ref()
            .expect("expected captured present framebuffer");
        assert!(
            fb.iter().any(|&b| b != 0),
            "expected sprite frame to contain drawn pixels"
        );
    }

    #[test]
    fn run_templeos_demo_extchars_hc_patches_font_glyph() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture("::/Demo/ExtChars.HC");
        assert!(
            out.contains("Face:"),
            "expected ExtChars to print Face:, got: {out:?}"
        );
        assert!(
            out.contains('\u{00A0}'),
            "expected ExtChars to contain a CP437 0xFF NBSP char (font glyph 255), got: {out:?}"
        );

        let fb = res
            .first_present_fb
            .as_ref()
            .expect("expected first present snapshot");

        // "Face:" is 5 chars wide; the next char is CP437 0xFF (NBSP). Ensure it isn't blank by
        // checking for any non-background pixel inside its 8×8 cell.
        let w = 640usize;
        let x0 = 5usize * 8;
        let y0 = 0usize;
        let mut any = false;
        for dy in 0..8usize {
            for dx in 0..8usize {
                if fb[(y0 + dy) * w + (x0 + dx)] != 0 {
                    any = true;
                    break;
                }
            }
        }
        assert!(any, "expected ExtChars to draw a custom glyph at code 0xFF");
    }

    #[test]
    fn run_templeos_demo_graphics_palette_hc_end_to_end() {
        let _guard = env_guard();

        // PaletteDemo blocks on PressAKey twice; queue two keys up front so it can complete.
        let (out, res) = run_over_fake_shell_capture_with_events(
            "::/Demo/Graphics/Palette.HC",
            vec![
                protocol::Msg::key(b'a' as u32, true),
                protocol::Msg::key(b'b' as u32, true),
            ],
        );

        assert!(
            out.contains("__BLACK"),
            "expected PaletteDemo to print __BLACK__, got: {out:?}"
        );
        assert!(
            out.contains("__WHITE"),
            "expected PaletteDemo to print __WHITE__, got: {out:?}"
        );
        assert!(
            res.presents >= 18,
            "expected PaletteDemo to Present() at least 18 times (16 lines + 2 PressAKey), got {}",
            res.presents
        );
    }

    #[test]
    fn run_templeos_demo_msgloop_hc_end_to_end() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture_with_events_after_first_present(
            "::/Demo/MsgLoop.HC",
            vec![
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
                protocol::Msg::key(protocol::KEY_ESCAPE, false),
            ],
        );

        assert!(
            res.presents >= 1,
            "expected at least one Present() from initial prints"
        );
        assert!(
            out.contains("CMD:"),
            "expected MsgLoop output to include decoded CMD lines, got: {out:?}"
        );
    }

    #[test]
    fn run_templeos_demo_pulldownmenu_hc_menu_cmd() {
        let _guard = env_guard();

        let (out, _res) = run_over_fake_shell_capture_with_events_after_first_present(
            "::/Demo/PullDownMenu.HC",
            vec![
                protocol::Msg::mouse_move(100, 0),  // hover Misc on the bar
                protocol::Msg::mouse_move(100, 16), // hover Opt1 in dropdown
                protocol::Msg::mouse_button(protocol::MOUSE_BUTTON_LEFT, true),
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
            ],
        );

        assert!(
            out.contains("Option # 1"),
            "expected menu selection to trigger MSG_CMD and print Option # 1, got: {out:?}"
        );
    }

    #[test]
    fn run_gr_rect_and_circle_smoke() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("templehc-prim-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("prims.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  CDC *dc = DCAlias;
  dc->color = 12;
  dc->thick = 2;
  GrRect(dc, 10, 20, 120, 80);
  GrCircle(dc, 220, 140, 40);
  Present();
}
Main;
"#,
        )
        .unwrap();

        let (out, res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "");
        assert!(res.presents > 0, "expected at least one Present()");
        assert!(
            res.fb.iter().any(|&b| b != 0),
            "expected framebuffer to contain drawn pixels"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_snd_smoke_over_ipc() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("templehc-snd-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("snd.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  Snd(62);
  Sleep(1);
  Snd;
}
Main;
"#,
        )
        .unwrap();

        let (out, res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "");
        assert!(
            res.snd_msgs >= 2,
            "expected at least two Snd messages (start+stop), got {}",
            res.snd_msgs
        );
        assert_eq!(res.last_snd_ona, Some(0), "expected last Snd to be rest");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_mute_smoke_over_ipc() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("templehc-mute-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("mute.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  Mute(TRUE);
  Mute(FALSE);
}
Main;
"#,
        )
        .unwrap();

        let (out, res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "");
        assert!(
            res.mute_msgs >= 2,
            "expected at least two Mute messages (on+off), got {}",
            res.mute_msgs
        );
        assert!(!res.is_muted, "expected final mute state to be FALSE");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_getchar_keycode_mapping_over_ipc() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("templehc-getchar-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("getchar.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 a = GetChar(,FALSE);
  I64 b = GetChar(,FALSE);
  I64 c = GetChar(,FALSE);
  "%d %d %d\n", a, b, c;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture_with_events(
            entry.to_str().unwrap(),
            vec![
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
                protocol::Msg::key(protocol::KEY_SHIFT, true),
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
                protocol::Msg::key(protocol::KEY_SHIFT, false),
                protocol::Msg::key(protocol::KEY_CONTROL, true),
                protocol::Msg::key(b'c' as u32, true),
                protocol::Msg::key(protocol::KEY_CONTROL, false),
            ],
        );
        assert_eq!(out, "27 28 3\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_templelinux_linuxbridge_smoke_over_ipc() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture_with_events_after_first_present(
            "holyc/LinuxBridge.HC",
            vec![protocol::Msg::key(protocol::KEY_ESCAPE, true)],
        );

        assert_eq!(out, "");
        assert!(
            res.presents >= 1,
            "expected at least one Present from LinuxBridge, got {}",
            res.presents
        );
    }

    #[test]
    fn filefind_absolute_paths_falls_back_to_templeos_tree_for_reads() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-filefind-abs-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("Home")).unwrap();

        let entry = dir.join("abs_filefind.HC");
        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 ok = FileFind("/Adam/AutoComplete/ACDefs.DATA");
  "%d\n", ok;
}
Main;
"#,
        )
        .unwrap();

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let templeos_root = root.join("third_party/TempleOS");

        let old_root = std::env::var("TEMPLE_ROOT").ok();
        let old_tos = std::env::var("TEMPLEOS_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLE_ROOT", &dir);
            std::env::set_var("TEMPLEOS_ROOT", &templeos_root);
        }

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "1\n");

        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }
        match old_tos {
            Some(v) => unsafe { std::env::set_var("TEMPLEOS_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLEOS_ROOT") },
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_templeos_apps_timeclock_load_hc_installer_runs() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("templehc-timeclock-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Home")).unwrap();

        let old_root = std::env::var("TEMPLE_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLE_ROOT", &dir);
        }

        let (out, _res) = run_over_fake_shell_capture("::/Apps/TimeClock/Load.HC");
        assert!(
            out.contains("After Loading"),
            "expected installer text to be printed, got: {out:?}"
        );
        assert!(
            dir.join("Home/TimeClock").is_dir(),
            "expected DirMk to create ~/TimeClock under TEMPLE_ROOT"
        );

        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_templeos_apps_timeclock_timerep_empty_file_smoke() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-timeclock-timerep-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let entry = dir.join("timerep.HC");
        std::fs::write(
            &entry,
            r#"
#include "::/Apps/TimeClock/TimeClk.HC"
U0 Main() {
  TimeRep(NULL);
}
Main;
"#,
        )
        .unwrap();

        let old_root = std::env::var("TEMPLE_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLE_ROOT", &dir);
        }

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert!(
            out.contains("Week Total:"),
            "expected TimeRep to print a week total line, got: {out:?}"
        );
        assert!(
            out.contains("00:00:00"),
            "expected empty TimeClock file to produce 00:00:00 total, got: {out:?}"
        );

        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_templeos_apps_timeclock_timerep_reads_two_entries() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-timeclock-timerep2-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("Home/TimeClock")).unwrap();

        // Write a deterministic TimeClock file:
        //   [type][CDate][desc\0] ... [type=0]
        let cdt = |hour: i64, min: i64, sec: i64| -> i64 {
            let secs = hour * 3600 + min * 60 + sec;
            let ticks = secs * CDATE_FREQ_HZ;
            (0i64 << 32) | (ticks as u32 as i64)
        };
        let in_cdt = cdt(1, 0, 0);
        let out_cdt = cdt(9, 30, 0);

        let mut bytes: Vec<u8> = Vec::new();
        bytes.push(1u8); // TET_PUNCH_IN
        bytes.extend_from_slice(&in_cdt.to_le_bytes());
        bytes.extend_from_slice(b"Start\0");
        bytes.push(2u8); // TET_PUNCH_OUT
        bytes.extend_from_slice(&out_cdt.to_le_bytes());
        bytes.extend_from_slice(b"End\0");
        bytes.push(0u8); // EOF

        let data_path = dir.join("Home/TimeClock/TimeFile.DATA.Z");
        std::fs::write(&data_path, bytes).unwrap();

        let entry = dir.join("timerep2.HC");
        std::fs::write(
            &entry,
            r#"
#include "::/Apps/TimeClock/TimeClk.HC"
U0 Main() {
  TimeRep(NULL);
}
Main;
"#,
        )
        .unwrap();

        let old_root = std::env::var("TEMPLE_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLE_ROOT", &dir);
        }

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert!(
            out.contains("Start") && out.contains("End"),
            "expected output to contain both descriptions, got: {out:?}"
        );
        assert!(
            out.contains("01/01/70"),
            "expected output to contain the epoch date, got: {out:?}"
        );
        assert!(
            out.contains("Week Total:08:30:00"),
            "expected a deterministic 8.5 hour total, got: {out:?}"
        );

        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_templelinux_timeclock_wrapper_menu_strips_doldoc_markup() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-timeclock-wrapper-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let old_root = std::env::var("TEMPLE_ROOT").ok();
        unsafe {
            std::env::set_var("TEMPLE_ROOT", &dir);
        }

        let (out, res) = run_over_fake_shell_capture_with_events_after_first_present(
            "holyc/TimeClock.HC",
            vec![protocol::Msg::key(protocol::KEY_ESCAPE, true)],
        );
        assert!(
            res.presents >= 1,
            "expected at least one Present() from TimeClock wrapper"
        );
        assert!(
            out.contains("TimeClock"),
            "expected menu output, got: {out:?}"
        );
        assert!(
            !out.contains("$$"),
            "expected DolDoc markup to be stripped from output, got: {out:?}"
        );

        match old_root {
            Some(v) => unsafe { std::env::set_var("TEMPLE_ROOT", v) },
            None => unsafe { std::env::remove_var("TEMPLE_ROOT") },
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn getstr_with_new_line_accepts_esc_and_inserts_newlines() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("templehc-getstr-nl-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("getstr_nl.HC");
        std::fs::write(
            &entry,
            r#"
U0 Main() {
  U8 *s=GetStr(,,GSF_WITH_NEW_LINE);
  if (!s)
    "NULL\n";
  else
    "%s\n",s;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture_with_events(
            entry.to_str().unwrap(),
            vec![
                protocol::Msg::key(b'H' as u32, true),
                protocol::Msg::key(b'e' as u32, true),
                protocol::Msg::key(b'l' as u32, true),
                protocol::Msg::key(b'l' as u32, true),
                protocol::Msg::key(b'o' as u32, true),
                protocol::Msg::key(protocol::KEY_ENTER, true),
                protocol::Msg::key(b'W' as u32, true),
                protocol::Msg::key(b'o' as u32, true),
                protocol::Msg::key(b'r' as u32, true),
                protocol::Msg::key(b'l' as u32, true),
                protocol::Msg::key(b'd' as u32, true),
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
            ],
        );
        assert_eq!(out, "Hello\nWorld\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn getstr_with_new_line_shift_esc_returns_empty_string() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-getstr-shiftesc-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("getstr_shiftesc.HC");
        std::fs::write(
            &entry,
            r#"
U0 Main() {
  U8 *s=GetStr(,,GSF_WITH_NEW_LINE);
  if (!s)
    "NULL\n";
  else
    "%s\n",s;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture_with_events(
            entry.to_str().unwrap(),
            vec![
                protocol::Msg::key(protocol::KEY_SHIFT, true),
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
                protocol::Msg::key(protocol::KEY_SHIFT, false),
            ],
        );
        assert_eq!(out, "\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_compare_chain_semantics_over_ipc() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("templehc-chaincmp-{uniq}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("chaincmp.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 x=0;
  if (100<=x<400) "in\n"; else "out\n";
  x=200;
  if (100<=x<400) "in\n"; else "out\n";

  I64 a=1,b=1,c=1,d=2;
  if (a==b==c) "eq\n"; else "neq\n";
  if (a==b==d) "eq\n"; else "neq\n";
}
Main;
"#,
        )
        .unwrap();

        let (out, res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert!(res.presents >= 1, "expected at least one Present()");
        assert_eq!(out, "out\nin\neq\nneq\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_logical_short_circuit_semantics_over_ipc() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-shortcircuit-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("shortcircuit.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 a=0;
  if (TRUE || (a=1)) ;
  if (FALSE && (a=2)) ;
  "%d\n", a;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "0\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_char_literal_can_contain_newline_byte() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-charlit-newline-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("charlit_newline.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 x = '
';
  "%d\n", x;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "10\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_operator_precedence_shift_binds_tighter_than_mul() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-precedence-shift-mul-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("precedence_shift_mul.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 x = 80>>1*8;
  "%d\n", x;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "320\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_operator_precedence_bitand_binds_tighter_than_add() {
        let _guard = env_guard();

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "templehc-precedence-bitand-add-{uniq}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("precedence_bitand_add.HC");

        std::fs::write(
            &entry,
            r#"
U0 Main() {
  I64 x = 1+2&4;
  "%d\n", x;
}
Main;
"#,
        )
        .unwrap();

        let (out, _res) = run_over_fake_shell_capture(entry.to_str().unwrap());
        assert_eq!(out, "1\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_templeos_demo_games_tictactoe_hc_ctrl_alt_c_exit() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture_with_events_after_first_present(
            "::/Demo/Games/TicTacToe.HC",
            vec![
                protocol::Msg::key(protocol::KEY_CONTROL, true),
                protocol::Msg::key(protocol::KEY_ALT, true),
                protocol::Msg::key(b'c' as u32, true),
            ],
        );

        assert!(
            res.presents >= 1,
            "expected TicTacToe to Present() at least once"
        );
        assert!(
            out.contains("CTRL-ALT-c"),
            "expected TicTacToe to print exit hint, got: {out:?}"
        );
    }

    #[test]
    fn run_templeos_demo_graphics_slider_hc_draws_ctrls() {
        let _guard = env_guard();

        let (out, res) = run_over_fake_shell_capture_with_events_impl(
            "::/Demo/Graphics/Slider.HC",
            vec![protocol::Msg::key(protocol::KEY_ENTER, true)],
            true,
            Some(2),
        );

        assert!(
            res.presents >= 2,
            "expected Slider to Present() at least twice (print + PressAKey), got {}",
            res.presents
        );
        assert!(
            out.contains("demo ctrls"),
            "expected Slider to print intro text, got: {out:?}"
        );
        assert!(
            out.contains("Left:"),
            "expected Slider to print Left/Right summary, got: {out:?}"
        );

        let fb = res
            .captured_present_fb
            .expect("expected a captured frame for present #2");
        let yellow = fb.iter().filter(|&&b| b == 14).count();
        assert!(
            yellow > 10,
            "expected Slider to draw at least some YELLOW pixels for the handles, got {yellow}"
        );
    }

    #[test]
    fn run_templelinux_wallpaperctrl_wrapper_ctrl_click_changes_frame() {
        let _guard = env_guard();

        // The wrapper keeps presenting frames until it sees Esc; click inside the ctrl between
        // present #1 and present #2 so the second frame should differ.
        let (_out, res) = run_over_fake_shell_capture_with_events_impl(
            "holyc/WallPaperCtrl.HC",
            vec![
                protocol::Msg::mouse_move(309, 225),
                protocol::Msg::mouse_button(protocol::MOUSE_BUTTON_LEFT, true),
                protocol::Msg::mouse_button(protocol::MOUSE_BUTTON_LEFT, false),
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
                protocol::Msg::key(protocol::KEY_ESCAPE, false),
            ],
            true,
            Some(2),
        );

        assert!(
            res.presents >= 2,
            "expected WallPaperCtrl wrapper to Present() at least twice, got {}",
            res.presents
        );

        let fb1 = res
            .first_present_fb
            .as_ref()
            .expect("expected first present snapshot");
        let fb2 = res
            .captured_present_fb
            .as_ref()
            .expect("expected captured present snapshot");
        assert_ne!(fb1, fb2, "expected click to change rendered ctrl state");
    }

    #[test]
    fn run_templelinux_wallpaperfish_wrapper_smoke() {
        let _guard = env_guard();

        let old_seed = std::env::var("TEMPLE_HC_SEED").ok();
        let old_fixed_ts = std::env::var("TEMPLE_HC_FIXED_TS").ok();
        unsafe {
            std::env::set_var("TEMPLE_HC_SEED", "1");
            std::env::set_var("TEMPLE_HC_FIXED_TS", "0");
        }

        let (_out, res) = run_over_fake_shell_capture_with_events_impl(
            "holyc/WallPaperFish.HC",
            vec![
                protocol::Msg::key(protocol::KEY_ESCAPE, true),
                protocol::Msg::key(protocol::KEY_ESCAPE, false),
            ],
            true,
            Some(1),
        );

        match old_seed {
            Some(v) => unsafe { std::env::set_var("TEMPLE_HC_SEED", v) },
            None => unsafe { std::env::remove_var("TEMPLE_HC_SEED") },
        }
        match old_fixed_ts {
            Some(v) => unsafe { std::env::set_var("TEMPLE_HC_FIXED_TS", v) },
            None => unsafe { std::env::remove_var("TEMPLE_HC_FIXED_TS") },
        }

        assert!(
            res.presents >= 1,
            "expected WallPaperFish wrapper to Present() at least once, got {}",
            res.presents
        );

        let fb = res
            .captured_present_fb
            .or(res.first_present_fb)
            .expect("expected a captured frame for present #1");
        assert!(fb.iter().any(|&b| b != 0), "expected non-black pixels");
    }

    #[test]
    fn run_templeos_demo_graphics_scrollbars_hc_smoke() {
        let _guard = env_guard();

        let (_out, res) = run_over_fake_shell_capture_with_events_impl(
            "::/Demo/Graphics/ScrollBars.HC",
            vec![protocol::Msg::key(protocol::KEY_ENTER, true)],
            true,
            Some(1),
        );

        assert!(
            res.presents >= 1,
            "expected ScrollBars to Present() at least once, got {}",
            res.presents
        );

        let fb = res
            .captured_present_fb
            .expect("expected a captured frame for present #1");
        let any = fb.iter().any(|&b| b != 0);
        assert!(any, "expected ScrollBars to draw non-black pixels");
    }

    #[test]
    fn run_templeos_demo_graphics_grid_hc_mouse_exit() {
        let _guard = env_guard();

        let (_out, res) = run_over_fake_shell_capture_with_events_impl(
            "::/Demo/Graphics/Grid.HC",
            vec![protocol::Msg::mouse_button(
                protocol::MOUSE_BUTTON_LEFT,
                true,
            )],
            true,
            Some(1),
        );

        assert!(
            res.presents >= 1,
            "expected Grid to Present() at least once, got {}",
            res.presents
        );

        let fb = res
            .captured_present_fb
            .expect("expected a captured frame for present #1");
        let any = fb.iter().any(|&b| b != 0);
        assert!(any, "expected Grid to draw non-black pixels");
    }
}
