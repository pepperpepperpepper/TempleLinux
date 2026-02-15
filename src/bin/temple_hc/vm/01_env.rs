use super::prelude::*;
use super::{Value, VarType};

#[derive(Default)]
pub(super) struct Env {
    scopes: Vec<HashMap<String, Value>>,
    types: Vec<HashMap<String, VarType>>,
}

impl Env {
    pub(super) fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            types: vec![HashMap::new()],
        }
    }

    pub(super) fn push(&mut self) {
        self.scopes.push(HashMap::new());
        self.types.push(HashMap::new());
    }

    pub(super) fn pop(&mut self) {
        self.scopes.pop();
        self.types.pop();
        if self.scopes.is_empty() {
            self.scopes.push(HashMap::new());
            self.types.push(HashMap::new());
        }
    }

    pub(super) fn define(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    pub(super) fn define_typed(&mut self, name: String, ty: VarType, value: Value) {
        if let Some(types) = self.types.last_mut() {
            types.insert(name.clone(), ty);
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty.coerce_value(value));
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn get_type(&self, name: &str) -> Option<VarType> {
        for scope in self.types.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(*v);
            }
        }
        None
    }

    pub(super) fn assign(&mut self, name: &str, value: Value) -> Result<(), String> {
        for i in (0..self.scopes.len()).rev() {
            if self.scopes[i].contains_key(name) {
                let value = self
                    .types
                    .get(i)
                    .and_then(|scope| scope.get(name))
                    .copied()
                    .unwrap_or_default()
                    .coerce_value(value);
                self.scopes[i].insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(format!("unknown variable: {name}"))
    }
}

pub(super) struct EnvScopeGuard {
    env: *mut Env,
}

impl EnvScopeGuard {
    pub(super) fn new(env: &mut Env) -> Self {
        env.push();
        Self { env }
    }
}

impl Drop for EnvScopeGuard {
    fn drop(&mut self) {
        unsafe {
            (*self.env).pop();
        }
    }
}

#[derive(Debug)]
pub(super) enum ControlFlow {
    Continue,
    Break,
    LoopContinue,
    Goto(String),
    Return(Value),
}

#[derive(Debug)]
pub(super) enum VmPanic {
    Throw,
}
