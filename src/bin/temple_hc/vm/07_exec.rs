use super::prelude::*;
use super::{ControlFlow, EnvScopeGuard, ScalarKind, Value, VarType, Vm, VmPanic};

impl Vm {
    pub(super) fn exec_snippet(&mut self, file: Arc<str>, src: &str) -> Result<(), String> {
        let mut lex = Lexer::new(file, src.as_bytes(), 1, Lexer::empty_macros());
        let mut tokens: Vec<Token> = Vec::new();
        loop {
            let t = lex.next_token().map_err(|e| e.to_string())?;
            let done = matches!(t.kind, TokenKind::Eof);
            tokens.push(t);
            if done {
                break;
            }
        }

        let mut p = Parser::new(tokens);
        let program = p.parse_program().map_err(|e| e.to_string())?;
        let flow = self.exec_block_unscoped(&program.top_level)?;
        match flow {
            ControlFlow::Continue => Ok(()),
            ControlFlow::Return(_) => Ok(()),
            ControlFlow::Break => Err("break used outside of a loop/switch".to_string()),
            ControlFlow::LoopContinue => Err("continue used outside of a loop".to_string()),
            ControlFlow::Goto(label) => Err(format!("unknown label: {label}")),
        }
    }

    pub(super) fn capture_push(&mut self, ch: char) {
        if let Some(buf) = self.capture.as_mut() {
            buf.push(ch);
        }
    }

    pub(crate) fn run(&mut self) -> io::Result<()> {
        self.main_called = false;

        fn vm_err_to_io(err: String) -> io::Error {
            if err.contains("Broken pipe") {
                io::Error::new(io::ErrorKind::BrokenPipe, err)
            } else {
                io::Error::other(err)
            }
        }

        let top_level = self.program.top_level.clone();
        let _ = self.exec_block_unscoped(&top_level).map_err(vm_err_to_io)?;

        if self.main_called {
            return Ok(());
        }

        if let Some(body) = self
            .program
            .functions
            .get("Main")
            .or_else(|| self.program.functions.get("main"))
            .map(|f| f.body.clone())
        {
            let _ = self.exec_block(&body).map_err(vm_err_to_io)?;
        }
        Ok(())
    }

    fn exec_stmts_with_goto(&mut self, stmts: &[Stmt]) -> Result<ControlFlow, String> {
        fn stmt_summary(stmt: &Stmt) -> String {
            match stmt {
                Stmt::Empty => "empty stmt".to_string(),
                Stmt::Print { .. } => "print stmt".to_string(),
                Stmt::Label(name) => format!("label: {name}"),
                Stmt::Goto(label) => format!("goto: {label}"),
                Stmt::VarDecl { decl } => format!("var decl: {} {}", decl.ty, decl.name),
                Stmt::VarDecls { decls } => {
                    let mut names = String::new();
                    for (i, decl) in decls.iter().enumerate() {
                        if i > 0 {
                            names.push_str(", ");
                        }
                        names.push_str(&decl.name);
                    }
                    format!("var decls: {names}")
                }
                Stmt::Assign { lhs, expr } => format!("assignment stmt: {lhs:?} = {expr:?}"),
                Stmt::ExprStmt(expr) => format!("expr stmt: {expr:?}"),
                Stmt::TryCatch { .. } => "try/catch stmt".to_string(),
                Stmt::Throw => "throw stmt".to_string(),
                Stmt::Break => "break stmt".to_string(),
                Stmt::Continue => "continue stmt".to_string(),
                Stmt::If { .. } => "if stmt".to_string(),
                Stmt::While { .. } => "while stmt".to_string(),
                Stmt::DoWhile { .. } => "do/while stmt".to_string(),
                Stmt::For { .. } => "for stmt".to_string(),
                Stmt::Switch { .. } => "switch stmt".to_string(),
                Stmt::Return(_) => "return stmt".to_string(),
            }
        }

        let mut labels: HashMap<String, usize> = HashMap::new();
        for (i, stmt) in stmts.iter().enumerate() {
            if let Stmt::Label(name) = stmt {
                labels.insert(name.clone(), i);
            }
        }

        let mut ip = 0usize;
        while ip < stmts.len() {
            let flow = self
                .exec_stmt(&stmts[ip])
                .map_err(|err| format!("{err}\nwhile executing {}", stmt_summary(&stmts[ip])))?;
            match flow {
                ControlFlow::Continue => {
                    ip += 1;
                }
                ControlFlow::Goto(label) => {
                    if let Some(&target) = labels.get(&label) {
                        ip = target.saturating_add(1);
                    } else {
                        return Ok(ControlFlow::Goto(label));
                    }
                }
                other => return Ok(other),
            }
        }
        Ok(ControlFlow::Continue)
    }

    pub(super) fn exec_block_unscoped(&mut self, stmts: &[Stmt]) -> Result<ControlFlow, String> {
        self.exec_stmts_with_goto(stmts)
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<ControlFlow, String> {
        let _scope = EnvScopeGuard::new(&mut self.env);
        self.exec_stmts_with_goto(stmts)
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<ControlFlow, String> {
        match stmt {
            Stmt::Empty => Ok(ControlFlow::Continue),
            Stmt::Print { parts } => {
                self.exec_print(parts)?;
                Ok(ControlFlow::Continue)
            }
            Stmt::Label(_) => Ok(ControlFlow::Continue),
            Stmt::Goto(label) => Ok(ControlFlow::Goto(label.clone())),
            Stmt::Break => Ok(ControlFlow::Break),
            Stmt::Continue => Ok(ControlFlow::LoopContinue),
            Stmt::VarDecl { decl } => {
                let v = self.eval_decl_value(decl)?;
                let mut ty = VarType::default();
                if decl.pointer && decl.array_lens.is_empty() {
                    ty.pointer_elem_bytes = Some(Self::type_size_bytes(&decl.ty, false).max(1));
                }
                if decl.array_lens.is_empty() && !decl.pointer {
                    ty.scalar = Some(if matches!(decl.ty.as_str(), "F32" | "F64") {
                        ScalarKind::Float
                    } else {
                        ScalarKind::Int
                    });
                }
                self.env.define_typed(decl.name.clone(), ty, v);
                Ok(ControlFlow::Continue)
            }
            Stmt::VarDecls { decls } => {
                for decl in decls {
                    let v = self.eval_decl_value(decl)?;
                    let mut ty = VarType::default();
                    if decl.pointer && decl.array_lens.is_empty() {
                        ty.pointer_elem_bytes = Some(Self::type_size_bytes(&decl.ty, false).max(1));
                    }
                    if decl.array_lens.is_empty() && !decl.pointer {
                        ty.scalar = Some(if matches!(decl.ty.as_str(), "F32" | "F64") {
                            ScalarKind::Float
                        } else {
                            ScalarKind::Int
                        });
                    }
                    self.env.define_typed(decl.name.clone(), ty, v);
                }
                Ok(ControlFlow::Continue)
            }
            Stmt::Assign { lhs, expr } => {
                let v = self.eval_expr(expr)?;
                self.assign_lhs(lhs, v)?;
                Ok(ControlFlow::Continue)
            }
            Stmt::ExprStmt(expr) => {
                if let Expr::Var(name) = expr {
                    if self.program.functions.contains_key(name) || Self::is_builtin(name) {
                        let _ = self.call(name, &[])?;
                        return Ok(ControlFlow::Continue);
                    }
                }
                let _ = self.eval_expr(expr)?;
                Ok(ControlFlow::Continue)
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
            } => {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.exec_block(try_block)
                }));
                match res {
                    Ok(flow) => flow,
                    Err(payload) => {
                        if let Some(p) = payload.downcast_ref::<VmPanic>() {
                            if matches!(p, VmPanic::Throw) {
                                return self.exec_block(catch_block);
                            }
                        }
                        std::panic::resume_unwind(payload);
                    }
                }
            }
            Stmt::Throw => {
                std::panic::panic_any(VmPanic::Throw);
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                if self.eval_expr(cond)?.truthy() {
                    self.exec_block(then_block)
                } else if let Some(els) = else_block {
                    self.exec_block(els)
                } else {
                    Ok(ControlFlow::Continue)
                }
            }
            Stmt::While { cond, body } => {
                loop {
                    if !self.eval_expr(cond)?.truthy() {
                        break;
                    }
                    match self.exec_block(body)? {
                        ControlFlow::Continue => {}
                        ControlFlow::Break => break,
                        ControlFlow::LoopContinue => continue,
                        ControlFlow::Goto(label) => return Ok(ControlFlow::Goto(label)),
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                    }
                }
                Ok(ControlFlow::Continue)
            }
            Stmt::DoWhile { body, cond } => {
                loop {
                    match self.exec_block(body)? {
                        ControlFlow::Continue | ControlFlow::LoopContinue => {}
                        ControlFlow::Break => break,
                        ControlFlow::Goto(label) => return Ok(ControlFlow::Goto(label)),
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                    }
                    if !self.eval_expr(cond)?.truthy() {
                        break;
                    }
                }
                Ok(ControlFlow::Continue)
            }
            Stmt::For {
                init,
                cond,
                post,
                body,
            } => {
                let _scope = EnvScopeGuard::new(&mut self.env);

                if let Some(init) = init.as_deref() {
                    match self.exec_stmt(init)? {
                        ControlFlow::Continue => {}
                        ControlFlow::Break => {
                            return Err("break is not allowed in for-loop initializer".to_string());
                        }
                        ControlFlow::LoopContinue => {
                            return Err(
                                "continue is not allowed in for-loop initializer".to_string()
                            );
                        }
                        ControlFlow::Goto(label) => return Ok(ControlFlow::Goto(label)),
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                    }
                }

                loop {
                    if let Some(cond) = cond {
                        if !self.eval_expr(cond)?.truthy() {
                            break;
                        }
                    }

                    match self.exec_block(body)? {
                        ControlFlow::Continue => {}
                        ControlFlow::Break => break,
                        ControlFlow::LoopContinue => {}
                        ControlFlow::Goto(label) => return Ok(ControlFlow::Goto(label)),
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                    }

                    if let Some(post) = post {
                        let _ = self.eval_expr(post)?;
                    }
                }

                Ok(ControlFlow::Continue)
            }
            Stmt::Switch { expr, arms } => self.exec_switch_stmt(expr, arms),
            Stmt::Return(expr) => {
                let v = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Void,
                };
                Ok(ControlFlow::Return(v))
            }
        }
    }

    fn exec_switch_stmt(&mut self, expr: &Expr, arms: &[SwitchArm]) -> Result<ControlFlow, String> {
        let value = self.eval_expr(expr)?.as_i64()?;
        let Some(arm) = arms
            .iter()
            .find(|arm| switch_arm_contains_value(arm, value))
        else {
            return Ok(ControlFlow::Continue);
        };

        let flow = self.exec_switch_arm(value, arm)?;
        match flow {
            ControlFlow::Break => Ok(ControlFlow::Continue),
            other => Ok(other),
        }
    }

    fn exec_switch_arm(&mut self, value: i64, arm: &SwitchArm) -> Result<ControlFlow, String> {
        match arm {
            SwitchArm::Case { body, .. } => self.exec_block(body),
            SwitchArm::Group {
                prefix,
                arms,
                suffix,
            } => {
                let flow = self.exec_block(prefix)?;
                match flow {
                    ControlFlow::Continue => {}
                    ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                    ControlFlow::LoopContinue => return Ok(ControlFlow::LoopContinue),
                    ControlFlow::Goto(label) => return Ok(ControlFlow::Goto(label)),
                    ControlFlow::Break => return Ok(ControlFlow::Break),
                }

                if let Some(inner) = arms
                    .iter()
                    .find(|arm| switch_arm_contains_value(arm, value))
                {
                    let flow = self.exec_switch_arm(value, inner)?;
                    match flow {
                        ControlFlow::Continue => {}
                        ControlFlow::Break => {}
                        ControlFlow::LoopContinue => return Ok(ControlFlow::LoopContinue),
                        ControlFlow::Goto(label) => return Ok(ControlFlow::Goto(label)),
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                    }
                }

                self.exec_block(suffix)
            }
        }
    }
}
