use super::{
    ParseError, Program,
    preprocess::{
        SourceSegment, builtin_defines, compile_segments, discover_templeos_root, preprocess_entry,
        resolve_templeos_path,
    },
    vm,
};
use std::{collections::HashMap, env, error, fmt, io, path::PathBuf, process, sync::Arc};

fn demo_source() -> &'static str {
    r#"
// Temple HolyC subset demo
U0 Main() {
  I64 x = SCR_W/2 - 20;
  I64 y = SCR_H/2 - 15;

  while (1) {
    Clear(0);
    FillRect(0, 0, SCR_W, 16, 4);
    Text(4, 4, 15, 4, "temple-hc demo - arrows move - Esc exits");

    FillRect(x, y, 40, 30, 10);
    FillRect(x+2, y+2, 36, 26, 12);
    Text(x+6, y+10, 0, 12, "HC");
    Present();

	    I64 k = NextKey();
	    if (k == CH_ESC || k == CH_SHIFT_ESC) { return; }
	    if (k == KEY_LEFT) { x = x - 4; }
	    if (k == KEY_RIGHT) { x = x + 4; }
	    if (k == KEY_UP) { y = y - 4; }
	    if (k == KEY_DOWN) { y = y + 4; }

    Sleep(16);
  }
}
"#
}

pub(super) fn run() -> io::Result<()> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Mode {
        Run,
        Check,
    }

    fn print_usage() {
        eprintln!("temple-hc [--check] [program]");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  temple-hc");
        eprintln!("  temple-hc Hello.HC");
        eprintln!("  temple-hc ::/Demo/Graphics/NetOfDots.HC");
        eprintln!("  temple-hc --check Hello.HC");
    }

    #[derive(Debug)]
    enum TempleHcError {
        Io(io::Error),
        Parse(ParseError),
    }

    impl fmt::Display for TempleHcError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                TempleHcError::Io(err) => write!(f, "{err}"),
                TempleHcError::Parse(err) => write!(f, "{err}"),
            }
        }
    }

    impl error::Error for TempleHcError {}

    impl From<io::Error> for TempleHcError {
        fn from(value: io::Error) -> Self {
            Self::Io(value)
        }
    }

    impl From<ParseError> for TempleHcError {
        fn from(value: ParseError) -> Self {
            Self::Parse(value)
        }
    }

    fn compile_program(
        spec: Option<&str>,
    ) -> Result<(Program, Arc<HashMap<String, String>>), TempleHcError> {
        let (segments, defines, bins_by_file) = match spec {
            Some(spec) => {
                let templeos_root = discover_templeos_root();
                let base_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let entry_path = resolve_templeos_path(spec, &base_dir, templeos_root.as_deref())?;
                preprocess_entry(&entry_path, templeos_root.as_deref())?
            }
            None => (
                vec![SourceSegment {
                    file: "<demo>".into(),
                    start_line: 1,
                    bytes: demo_source().as_bytes().to_vec(),
                }],
                HashMap::new(),
                HashMap::new(),
            ),
        };

        let mut macros = builtin_defines();
        macros.extend(defines);
        let macros = Arc::new(macros);
        let program = compile_segments(segments, macros.clone(), bins_by_file)?;
        Ok((program, macros))
    }

    let mut args = env::args().skip(1);
    let mut mode = Mode::Run;
    let mut spec: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            "--check" | "-c" => {
                mode = Mode::Check;
            }
            _ if spec.is_none() => {
                spec = Some(arg);
            }
            _ => {
                eprintln!("temple-hc: unexpected arg: {arg}");
                print_usage();
                return Ok(());
            }
        }
    }

    let res = compile_program(spec.as_deref());

    match (mode, res) {
        (Mode::Check, Ok((_program, _macros))) => Ok(()),
        (Mode::Check, Err(err)) => match &err {
            TempleHcError::Parse(_) => {
                eprintln!("{err}");
                process::exit(2);
            }
            TempleHcError::Io(_) => {
                eprintln!("temple-hc: {err}");
                process::exit(1);
            }
        },
        (Mode::Run, Ok((program, macros))) => {
            let rt = temple_rt::rt::TempleRt::connect()?;
            let mut vm = vm::Vm::new(rt, program, macros);
            match vm.run() {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
                Err(err) => Err(err),
            }
        }
        (Mode::Run, Err(err)) => match &err {
            TempleHcError::Parse(_) => {
                eprintln!("{err}");
                process::exit(2);
            }
            TempleHcError::Io(_) => {
                eprintln!("temple-hc: {err}");
                process::exit(1);
            }
        },
    }
}
