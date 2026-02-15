use super::prelude::*;
use super::{ControlFlow, EnvScopeGuard, Value, Vm};

impl Vm {
    pub(super) fn is_builtin(name: &str) -> bool {
        matches!(
            name,
            "Clear"
                | "MAlloc"
                | "CAlloc"
                | "ACAlloc"
                | "Free"
                | "FileRead"
                | "FileWrite"
                | "StrLen"
                | "StrNew"
                | "StrCpy"
                | "QueInit"
                | "QueIns"
                | "QueRem"
                | "Now"
                | "tS"
                | "Tri"
                | "MemSet"
                | "MemSetU16"
                | "MemCpy"
                | "TaskDerivedValsUpdate"
                | "GridInit"
                | "SetPixel"
                | "FillRect"
                | "Text"
                | "TextChar"
                | "Present"
                | "Sleep"
                | "Seed"
                | "RandI16"
                | "RandU16"
                | "Rand"
                | "SignI64"
                | "ClampI64"
                | "Abs"
                | "Max"
                | "Sqr"
                | "Cos"
                | "Sin"
                | "Sqrt"
                | "Exp"
                | "Arg"
                | "ToI64"
                | "Snd"
                | "SndRst"
                | "Beep"
                | "Mute"
                | "IsMute"
                | "Ona2Freq"
                | "Freq2Ona"
                | "GetChar"
                | "GetKey"
                | "GetMsg"
                | "ScanMsg"
                | "MenuPush"
                | "MenuPop"
                | "MenuEntryFind"
                | "NextKey"
                | "Refresh"
                | "Yield"
                | "GrPlot"
                | "GrLine"
                | "GrLine3"
                | "GrBorder"
                | "GrRect"
                | "GrCircle"
                | "GrCircle3"
                | "GrEllipse"
                | "GrFloodFill"
                | "GrPrint"
                | "GrPaletteColorSet"
                | "Sprite3"
                | "Sprite3YB"
                | "SpriteInterpolate"
                | "DCDepthBufAlloc"
                | "D3I32Norm"
                | "Noise"
                | "QSortI64"
                | "DCFill"
                | "DCAlias"
                | "DCSymmetrySet"
                | "DCDel"
                | "DocClear"
                | "DocCursor"
                | "DocBottom"
                | "DocScroll"
                | "Cd"
                | "DefineLstLoad"
                | "DefineSub"
                | "FileFind"
                | "DirMk"
                | "WinMax"
                | "WinBorder"
                | "RegDft"
                | "RegExe"
                | "RegWrite"
                | "PopUpOk"
                | "SettingsPush"
                | "SettingsPop"
                | "AutoComplete"
                | "Spawn"
                | "PutExcept"
                | "PressAKey"
                | "ClipPutS"
                | "GetStr"
                | "LinuxBrowse"
                | "LinuxOpen"
                | "LinuxRun"
                | "LinuxLastErr"
        )
    }

    pub(super) fn call(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        if name == "Main" || name == "main" {
            self.main_called = true;
        }

        if Self::is_builtin(name) {
            return self.call_builtin(name, args);
        }

        if let Some(func) = self.program.functions.get(name).cloned() {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                if matches!(arg, Expr::DefaultArg) {
                    values.push(Value::Int(0));
                } else {
                    values.push(self.eval_expr(arg)?);
                }
            }

            if values.len() != func.params.len() {
                return Err(format!(
                    "function {name} expects {} args (got {})",
                    func.params.len(),
                    values.len()
                ));
            }

            let flow = {
                let _scope = EnvScopeGuard::new(&mut self.env);
                for (param, value) in func.params.iter().cloned().zip(values) {
                    self.env.define(param, value);
                }
                self.exec_block_unscoped(&func.body)
            }
            .map_err(|err| format!("{err}\nwhile calling {name}()"))?;

            match flow {
                ControlFlow::Continue => Ok(Value::Void),
                ControlFlow::Return(v) => Ok(v),
                ControlFlow::Break => Err("break used outside of a loop/switch".to_string()),
                ControlFlow::LoopContinue => Err("continue used outside of a loop".to_string()),
                ControlFlow::Goto(label) => Err(format!("unknown label: {label}")),
            }
        } else {
            Err(format!("unknown function: {name}"))
        }
    }
}
