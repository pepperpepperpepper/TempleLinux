use super::prelude::*;
use super::{Value, Vm};

mod core;
mod doc_fs_settings;
mod gfx;
mod linux;
mod ui_input_sound;

impl Vm {
    pub(super) fn call_builtin(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        if name.starts_with("Linux") {
            return self.call_builtin_linux(name, args);
        }

        if matches!(
            name,
            "GridInit"
                | "SetPixel"
                | "FillRect"
                | "Text"
                | "TextChar"
                | "Present"
                | "Refresh"
                | "Yield"
                | "Sleep"
                | "Seed"
                | "RandI16"
                | "RandU16"
                | "Rand"
                | "SignI64"
                | "ClampI64"
                | "GetChar"
                | "GetKey"
                | "ScanMsg"
                | "GetMsg"
                | "MenuPush"
                | "MenuPop"
                | "MenuEntryFind"
                | "Snd"
                | "SndRst"
                | "Beep"
                | "Mute"
                | "IsMute"
                | "Ona2Freq"
                | "Freq2Ona"
                | "NextKey"
        ) {
            return self.call_builtin_ui_input_sound(name, args);
        }

        if name.starts_with("Gr")
            || name.starts_with("Sprite")
            || matches!(name, "DCDepthBufAlloc" | "D3I32Norm")
        {
            return self.call_builtin_gfx(name, args);
        }

        if name.starts_with("Doc")
            || name.starts_with("Win")
            || name.starts_with("Reg")
            || name.starts_with("Define")
            || name.starts_with("Settings")
            || matches!(
                name,
                "Cd" | "FileFind"
                    | "DirMk"
                    | "PopUpOk"
                    | "AutoComplete"
                    | "Spawn"
                    | "PutExcept"
                    | "PressAKey"
                    | "GetStr"
                    | "ClipPutS"
                    | "DCFill"
                    | "DCAlias"
                    | "DCSymmetrySet"
                    | "DCDel"
            )
        {
            return self.call_builtin_doc_fs_settings(name, args);
        }

        self.call_builtin_core(name, args)
    }
}
