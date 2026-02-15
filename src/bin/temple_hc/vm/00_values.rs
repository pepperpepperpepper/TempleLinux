use super::prelude::*;

#[derive(Debug)]
pub(super) struct Obj {
    pub(super) fields: HashMap<String, Value>,
}

pub(super) type ObjRef = Rc<RefCell<Obj>>;

#[derive(Debug)]
pub(super) struct ArrayValue {
    pub(super) elems: Vec<Value>,
    pub(super) elem_bytes: usize,
}

pub(super) type ArrayRef = Rc<RefCell<ArrayValue>>;

#[derive(Clone, Debug)]
pub(crate) enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Char(u64),
    VarRef(String),
    FuncRef(String),
    Ptr {
        addr: i64,
        elem_bytes: usize,
    },
    ArrayPtr {
        arr: ArrayRef,
        index: i64,
    },
    ObjFieldRef {
        obj: ObjRef,
        field: String,
    },
    Obj(ObjRef),
    Array(ArrayRef),
    IntView {
        value: u64,
        elem_bytes: u8,
        signed: bool,
    },
    Void,
}

impl Value {
    pub(super) fn truthy(&self) -> bool {
        match self {
            Value::Int(v) => *v != 0,
            Value::Float(v) => *v != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Char(v) => *v != 0,
            Value::VarRef(_) => true,
            Value::FuncRef(_) => true,
            Value::Ptr { addr, .. } => *addr != 0,
            Value::ArrayPtr { .. } => true,
            Value::ObjFieldRef { .. } => true,
            Value::Obj(_) => true,
            Value::Array(_) => true,
            Value::IntView { .. } => true,
            Value::Void => false,
        }
    }

    pub(crate) fn as_i64(&self) -> Result<i64, String> {
        match self {
            Value::Int(v) => Ok(*v),
            Value::Float(_) => Err("expected int, got float".to_string()),
            Value::Str(_) => Err("expected int, got string".to_string()),
            Value::Char(v) => Ok(*v as i64),
            Value::VarRef(_) => Err("expected int, got pointer".to_string()),
            Value::FuncRef(_) => Err("expected int, got function pointer".to_string()),
            Value::Ptr { addr, .. } => Ok(*addr),
            Value::ArrayPtr { .. } => {
                if std::env::var_os("TEMPLE_HC_TRACE_ARRAYPTR_AS_I64").is_some() {
                    let bt = std::backtrace::Backtrace::force_capture();
                    eprintln!("temple-hc: as_i64 on array pointer: {self:?}\n{bt}");
                }
                Err("expected int, got array pointer".to_string())
            }
            Value::ObjFieldRef { .. } => Err("expected int, got object field pointer".to_string()),
            Value::Obj(_) => Err("expected int, got object".to_string()),
            Value::Array(_) => Err("expected int, got array".to_string()),
            Value::IntView { .. } => Err("expected int, got sub-integer view".to_string()),
            Value::Void => Err("expected int, got void".to_string()),
        }
    }

    pub(crate) fn as_f64(&self) -> Result<f64, String> {
        match self {
            Value::Int(v) => Ok(*v as f64),
            Value::Float(v) => Ok(*v),
            Value::Str(_) => Err("expected number, got string".to_string()),
            Value::Char(v) => Ok(*v as f64),
            Value::VarRef(_) => Err("expected number, got pointer".to_string()),
            Value::FuncRef(_) => Err("expected number, got function pointer".to_string()),
            Value::Ptr { .. } => Err("expected number, got pointer".to_string()),
            Value::ArrayPtr { .. } => Err("expected number, got pointer".to_string()),
            Value::ObjFieldRef { .. } => Err("expected number, got pointer".to_string()),
            Value::Obj(_) => Err("expected number, got object".to_string()),
            Value::Array(_) => Err("expected number, got array".to_string()),
            Value::IntView { .. } => Err("expected number, got sub-integer view".to_string()),
            Value::Void => Err("expected number, got void".to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct VarType {
    pub(super) pointer_elem_bytes: Option<usize>,
    pub(super) scalar: Option<ScalarKind>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ScalarKind {
    Int,
    Float,
}

impl VarType {
    pub(super) fn coerce_value(&self, value: Value) -> Value {
        if let Some(elem_bytes) = self.pointer_elem_bytes {
            return match value {
                Value::Int(addr) => Value::Ptr { addr, elem_bytes },
                Value::Ptr { addr, .. } => Value::Ptr { addr, elem_bytes },
                Value::Array(arr) => Value::ArrayPtr { arr, index: 0 },
                Value::ArrayPtr { arr, index } => Value::ArrayPtr { arr, index },
                other => other,
            };
        }

        match (self.scalar, value) {
            (Some(ScalarKind::Float), Value::Int(v)) => Value::Float(v as f64),
            (Some(ScalarKind::Float), Value::Char(v)) => Value::Float(v as f64),
            (Some(ScalarKind::Int), Value::Float(v)) => Value::Int(v as i64),
            (Some(ScalarKind::Int), Value::Char(v)) => Value::Int(v as i64),
            (_, other) => other,
        }
    }
}
