mod prelude;

#[path = "00_values.rs"]
mod values;
pub(super) use values::Value;
use values::{ArrayRef, ArrayValue, Obj, ObjRef, ScalarKind, VarType};

#[path = "01_env.rs"]
mod env;
use env::{ControlFlow, Env, EnvScopeGuard, VmPanic};

#[path = "02_ui_types.rs"]
mod ui_types;
use ui_types::{MenuAction, MenuGroup, MenuItem, MenuState, MenuUnderlay, TempleMsg};

#[path = "03_vm_struct.rs"]
mod vm_struct;
pub(super) use vm_struct::Vm;

#[path = "04_init.rs"]
mod init;

#[path = "05_heap_doldoc_rng.rs"]
mod heap_doldoc_rng;

#[path = "06_linux_bridge.rs"]
mod linux_bridge;

#[path = "07_exec.rs"]
mod exec;

#[path = "08_eval.rs"]
mod eval;

mod builtins;
#[path = "09_ui.rs"]
mod ui;

#[path = "11_call.rs"]
mod call;

#[path = "10_text.rs"]
mod text;
