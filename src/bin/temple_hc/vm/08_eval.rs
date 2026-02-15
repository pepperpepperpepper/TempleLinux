use super::prelude::*;

use super::{ArrayValue, Obj, Value, Vm};

impl Vm {
    fn eval_addr_of(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Var(name) => {
                if self.program.functions.contains_key(name) || Self::is_builtin(name) {
                    Ok(Value::FuncRef(name.clone()))
                } else {
                    Ok(Value::VarRef(name.clone()))
                }
            }
            Expr::Index { base, index } => {
                let base = self.eval_expr(base)?;
                let idx = self.eval_expr(index)?.as_i64()?;
                match base {
                    Value::Array(arr) => Ok(Value::ArrayPtr { arr, index: idx }),
                    Value::ArrayPtr {
                        arr,
                        index: base_idx,
                    } => Ok(Value::ArrayPtr {
                        arr,
                        index: base_idx.saturating_add(idx),
                    }),
                    Value::Ptr { addr, elem_bytes } => {
                        let scaled = (idx as i128)
                            .saturating_mul(elem_bytes as i128)
                            .clamp(i64::MIN as i128, i64::MAX as i128)
                            as i64;
                        Ok(Value::Ptr {
                            addr: addr.saturating_add(scaled),
                            elem_bytes,
                        })
                    }
                    Value::Int(addr) => Ok(Value::Ptr {
                        addr: addr.saturating_add(idx),
                        elem_bytes: 1,
                    }),
                    other => Err(format!("cannot take address of index on: {other:?}")),
                }
            }
            Expr::Member { base, field } | Expr::PtrMember { base, field } => {
                let base = self.eval_expr(base)?;
                let Value::Obj(obj) = base else {
                    return Err(format!(
                        "cannot take address of field {field} on non-object"
                    ));
                };
                Ok(Value::ObjFieldRef {
                    obj,
                    field: field.clone(),
                })
            }
            Expr::Deref(inner) => self.eval_expr(inner),
            _ => Err("address-of (&) expects an lvalue".to_string()),
        }
    }

    pub(super) fn eval_expr(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::DefaultArg => Ok(Value::Void),
            Expr::Int(v) => Ok(Value::Int(*v)),
            Expr::Float(v) => Ok(Value::Float(*v)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Char(v) => Ok(Value::Char(*v)),
            Expr::InitList(_) => {
                Err("initializer lists are only supported in array declarations".to_string())
            }
            Expr::Var(name) => {
                if name == "ScanChar" {
                    let v = self.scan_char;
                    self.scan_char = 0;
                    return Ok(Value::Int(v as i64));
                }
                if name == "Blink" {
                    let t = self
                        .fixed_ts
                        .unwrap_or_else(|| self.start_instant.elapsed().as_secs_f64());
                    let on = ((t * 2.0).floor() as i64) & 1 == 0;
                    return Ok(Value::Int(on as i64));
                }
                if let Some(v) = self.env.get(name) {
                    return Ok(v);
                }
                // TempleOS/HolyC frequently calls 0-arg functions without parentheses in
                // expression position (e.g. RandI16, Now, etc.). Treat an unknown identifier
                // as a 0-arg function call when possible.
                if self.program.functions.contains_key(name) || Self::is_builtin(name) {
                    return self.call(name, &[]);
                }
                Err(format!("unknown variable: {name}"))
            }
            Expr::AddrOf(inner) => self.eval_addr_of(inner),
            Expr::Deref(inner) => {
                let ptr = self.eval_expr(inner)?;
                match ptr {
                    Value::VarRef(name) => self
                        .env
                        .get(&name)
                        .ok_or_else(|| format!("dereferenced unknown variable: {name}")),
                    Value::FuncRef(name) => Ok(Value::FuncRef(name)),
                    Value::Ptr { addr, elem_bytes } => {
                        let v = self.heap_read_i64_le(addr, elem_bytes)?;
                        Ok(Value::Int(v))
                    }
                    Value::ArrayPtr { arr, index } => {
                        let idx: usize = index
                            .try_into()
                            .map_err(|_| "deref: negative array pointer".to_string())?;
                        let arr = arr.borrow();
                        arr.elems
                            .get(idx)
                            .cloned()
                            .ok_or_else(|| "deref: array pointer out of range".to_string())
                    }
                    Value::ObjFieldRef { obj, field } => obj
                        .borrow()
                        .fields
                        .get(&field)
                        .cloned()
                        .ok_or_else(|| format!("dereferenced unknown field: {field}")),
                    Value::Int(addr) => Ok(Value::Int(self.heap_read_u8(addr)? as i64)),
                    _ => Err("deref (*) expects a pointer".to_string()),
                }
            }
            Expr::Cast {
                expr,
                ty,
                pointer_depth,
            } => {
                if *pointer_depth > 0 {
                    let base = self.eval_expr(expr)?;
                    let addr = base.as_i64()?;
                    let elem_bytes = if *pointer_depth == 1 {
                        Self::type_size_bytes(ty, false).max(1)
                    } else {
                        8
                    };
                    return Ok(Value::Ptr { addr, elem_bytes });
                }

                let size = Self::type_size_bytes(ty, false);
                if size == 0 {
                    return Ok(Value::Int(0));
                }

                // HolyC supports a "reinterpret cast" style when casting array elements (and other
                // lvalues). For example, `face[0](U64)` loads 8 bytes starting at `face[0]`.
                if let Some(raw) = self.try_read_u64_from_lvalue_expr(expr, size)? {
                    return Ok(Self::cast_raw_bits_to_value(raw, ty, size));
                }

                // Fallback: numeric cast.
                let v = self.eval_expr(expr)?;
                let raw = match v {
                    Value::Float(f) => f as u64,
                    _ => v.as_i64()? as u64,
                };
                Ok(Self::cast_raw_bits_to_value(raw, ty, size))
            }
            Expr::SizeOf(inner) => Ok(Value::Int(self.sizeof_expr(inner)?)),
            Expr::Member { base, field } | Expr::PtrMember { base, field } => {
                let base = self.eval_expr(base)?;
                self.get_field(base, field)
            }
            Expr::Index { base, index } => {
                let base = self.eval_expr(base)?;
                let idx = self.eval_expr(index)?.as_i64()?;
                self.eval_index(base, idx)
            }
            Expr::Assign { op, lhs, rhs } => {
                let rhs_v = self.eval_expr(rhs)?;
                let out = match op {
                    AssignOp::Assign => rhs_v,
                    AssignOp::Add => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::Add, cur, rhs_v)?
                    }
                    AssignOp::Sub => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::Sub, cur, rhs_v)?
                    }
                    AssignOp::Mul => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::Mul, cur, rhs_v)?
                    }
                    AssignOp::Div => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::Div, cur, rhs_v)?
                    }
                    AssignOp::Rem => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::Rem, cur, rhs_v)?
                    }
                    AssignOp::BitAnd => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::BitAnd, cur, rhs_v)?
                    }
                    AssignOp::BitXor => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::BitXor, cur, rhs_v)?
                    }
                    AssignOp::BitOr => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::BitOr, cur, rhs_v)?
                    }
                    AssignOp::Shl => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::Shl, cur, rhs_v)?
                    }
                    AssignOp::Shr => {
                        let cur = self.eval_expr(lhs)?;
                        self.eval_bin(BinOp::Shr, cur, rhs_v)?
                    }
                };
                self.assign_lhs(lhs, out.clone())?;
                Ok(out)
            }
            Expr::PreInc(name) => {
                let cur = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {name}"))?;
                match cur {
                    Value::Ptr { addr, elem_bytes } => {
                        let new_addr = addr.saturating_add(elem_bytes as i64);
                        let out = Value::Ptr {
                            addr: new_addr,
                            elem_bytes,
                        };
                        self.env.assign(name, out.clone())?;
                        Ok(out)
                    }
                    Value::ArrayPtr { arr, index } => {
                        let new_index = index.saturating_add(1);
                        let out = Value::ArrayPtr {
                            arr,
                            index: new_index,
                        };
                        self.env.assign(name, out.clone())?;
                        Ok(out)
                    }
                    other => {
                        let v = other.as_i64()?;
                        let new_v = v.saturating_add(1);
                        self.env.assign(name, Value::Int(new_v))?;
                        Ok(Value::Int(new_v))
                    }
                }
            }
            Expr::PreDec(name) => {
                let cur = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {name}"))?;
                match cur {
                    Value::Ptr { addr, elem_bytes } => {
                        let new_addr = addr.saturating_sub(elem_bytes as i64);
                        let out = Value::Ptr {
                            addr: new_addr,
                            elem_bytes,
                        };
                        self.env.assign(name, out.clone())?;
                        Ok(out)
                    }
                    Value::ArrayPtr { arr, index } => {
                        let new_index = index.saturating_sub(1);
                        let out = Value::ArrayPtr {
                            arr,
                            index: new_index,
                        };
                        self.env.assign(name, out.clone())?;
                        Ok(out)
                    }
                    other => {
                        let v = other.as_i64()?;
                        let new_v = v.saturating_sub(1);
                        self.env.assign(name, Value::Int(new_v))?;
                        Ok(Value::Int(new_v))
                    }
                }
            }
            Expr::PostInc(name) => {
                let cur = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {name}"))?;
                match cur {
                    Value::Ptr { addr, elem_bytes } => {
                        let new_addr = addr.saturating_add(elem_bytes as i64);
                        let new_v = Value::Ptr {
                            addr: new_addr,
                            elem_bytes,
                        };
                        self.env.assign(name, new_v)?;
                        Ok(Value::Ptr { addr, elem_bytes })
                    }
                    Value::ArrayPtr { arr, index } => {
                        let new_index = index.saturating_add(1);
                        let new_v = Value::ArrayPtr {
                            arr: arr.clone(),
                            index: new_index,
                        };
                        self.env.assign(name, new_v)?;
                        Ok(Value::ArrayPtr { arr, index })
                    }
                    other => {
                        let v = other.as_i64()?;
                        let new_v = v.saturating_add(1);
                        self.env.assign(name, Value::Int(new_v))?;
                        Ok(Value::Int(v))
                    }
                }
            }
            Expr::PostDec(name) => {
                let cur = self
                    .env
                    .get(name)
                    .ok_or_else(|| format!("unknown variable: {name}"))?;
                match cur {
                    Value::Ptr { addr, elem_bytes } => {
                        let new_addr = addr.saturating_sub(elem_bytes as i64);
                        let new_v = Value::Ptr {
                            addr: new_addr,
                            elem_bytes,
                        };
                        self.env.assign(name, new_v)?;
                        Ok(Value::Ptr { addr, elem_bytes })
                    }
                    Value::ArrayPtr { arr, index } => {
                        let new_index = index.saturating_sub(1);
                        let new_v = Value::ArrayPtr {
                            arr: arr.clone(),
                            index: new_index,
                        };
                        self.env.assign(name, new_v)?;
                        Ok(Value::ArrayPtr { arr, index })
                    }
                    other => {
                        let v = other.as_i64()?;
                        let new_v = v.saturating_sub(1);
                        self.env.assign(name, Value::Int(new_v))?;
                        Ok(Value::Int(v))
                    }
                }
            }
            Expr::PostIncExpr(inner) => {
                if let Expr::Cast {
                    expr,
                    ty,
                    pointer_depth,
                } = inner.as_ref()
                {
                    let Expr::Var(name) = expr.as_ref() else {
                        return Err(
                            "postfix ++ on a cast currently requires a simple variable (e.g. ptr(U8 *)++)"
                                .to_string(),
                        );
                    };

                    let elem_bytes = if *pointer_depth <= 1 {
                        Self::type_size_bytes(ty, false).max(1)
                    } else {
                        8
                    };

                    let addr = self
                        .env
                        .get(name)
                        .ok_or_else(|| format!("unknown variable: {name}"))?
                        .as_i64()?;
                    let new_addr = addr.saturating_add(elem_bytes as i64);
                    self.env.assign(name, Value::Int(new_addr))?;
                    return Ok(Value::Ptr { addr, elem_bytes });
                }

                let cur = self.eval_expr(inner)?;
                let new_value = match &cur {
                    Value::Float(f) => Value::Float(f + 1.0),
                    Value::Ptr { addr, elem_bytes } => Value::Ptr {
                        addr: addr.saturating_add(*elem_bytes as i64),
                        elem_bytes: *elem_bytes,
                    },
                    Value::ArrayPtr { arr, index } => Value::ArrayPtr {
                        arr: arr.clone(),
                        index: index.saturating_add(1),
                    },
                    _ => {
                        let v = cur.as_i64()?;
                        Value::Int(v.saturating_add(1))
                    }
                };
                self.assign_lhs(inner, new_value)?;
                Ok(cur)
            }
            Expr::PostDecExpr(inner) => {
                if let Expr::Cast {
                    expr,
                    ty,
                    pointer_depth,
                } = inner.as_ref()
                {
                    let Expr::Var(name) = expr.as_ref() else {
                        return Err(
                            "postfix -- on a cast currently requires a simple variable (e.g. ptr(U8 *)--)"
                                .to_string(),
                        );
                    };

                    let elem_bytes = if *pointer_depth <= 1 {
                        Self::type_size_bytes(ty, false).max(1)
                    } else {
                        8
                    };

                    let addr = self
                        .env
                        .get(name)
                        .ok_or_else(|| format!("unknown variable: {name}"))?
                        .as_i64()?;
                    let new_addr = addr.saturating_sub(elem_bytes as i64);
                    self.env.assign(name, Value::Int(new_addr))?;
                    return Ok(Value::Ptr { addr, elem_bytes });
                }

                let cur = self.eval_expr(inner)?;
                let new_value = match &cur {
                    Value::Float(f) => Value::Float(f - 1.0),
                    Value::Ptr { addr, elem_bytes } => Value::Ptr {
                        addr: addr.saturating_sub(*elem_bytes as i64),
                        elem_bytes: *elem_bytes,
                    },
                    Value::ArrayPtr { arr, index } => Value::ArrayPtr {
                        arr: arr.clone(),
                        index: index.saturating_sub(1),
                    },
                    _ => {
                        let v = cur.as_i64()?;
                        Value::Int(v.saturating_sub(1))
                    }
                };
                self.assign_lhs(inner, new_value)?;
                Ok(cur)
            }
            Expr::Call { callee, args } => {
                if let Expr::Var(name) = callee.as_ref() {
                    return self.call(name, args);
                }

                let callee_v = self.eval_expr(callee)?;
                match callee_v {
                    Value::FuncRef(name) => self.call(&name, args),
                    other => Err(format!("cannot call non-function: {other:?}")),
                }
            }
            Expr::DolDocBinPtr { file, bin_num } => {
                let (addr, _len) = self.load_doldoc_bin(file, *bin_num)?;
                Ok(Value::Ptr {
                    addr,
                    elem_bytes: 1,
                })
            }
            Expr::DolDocBinSize { file, bin_num } => {
                let (_addr, len) = self.load_doldoc_bin(file, *bin_num)?;
                Ok(Value::Int(len as i64))
            }
            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => {
                        if matches!(v, Value::Float(_)) {
                            Ok(Value::Float(-v.as_f64()?))
                        } else {
                            Ok(Value::Int(-v.as_i64()?))
                        }
                    }
                    UnaryOp::Not => Ok(Value::Int((!v.truthy()) as i64)),
                    UnaryOp::BitNot => Ok(Value::Int(!v.as_i64()?)),
                }
            }
            Expr::CompareChain { first, rest } => {
                let mut prev = self.eval_expr(first)?;
                for (op, expr) in rest {
                    let next = self.eval_expr(expr)?;
                    if !self.eval_cmp_bool(*op, &prev, &next)? {
                        return Ok(Value::Int(0));
                    }
                    prev = next;
                }
                Ok(Value::Int(1))
            }
            Expr::Binary {
                op: BinOp::And,
                left,
                right,
            } => {
                let l = self.eval_expr(left)?;
                if !l.truthy() {
                    return Ok(Value::Int(0));
                }
                let r = self.eval_expr(right)?;
                Ok(Value::Int(r.truthy() as i64))
            }
            Expr::Binary {
                op: BinOp::Or,
                left,
                right,
            } => {
                let l = self.eval_expr(left)?;
                if l.truthy() {
                    return Ok(Value::Int(1));
                }
                let r = self.eval_expr(right)?;
                Ok(Value::Int(r.truthy() as i64))
            }
            Expr::Binary { op, left, right } => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.eval_bin(*op, l, r)
            }
        }
    }

    fn eval_cmp_bool(&self, op: BinOp, l: &Value, r: &Value) -> Result<bool, String> {
        match op {
            BinOp::Eq => Ok(match (l, r) {
                (Value::Obj(a), Value::Obj(b)) => Rc::ptr_eq(a, b),
                (Value::Obj(_), Value::Int(0)) | (Value::Int(0), Value::Obj(_)) => false,
                (Value::Str(a), Value::Str(b)) => a == b,
                (Value::Float(_), _) | (_, Value::Float(_)) => l.as_f64()? == r.as_f64()?,
                _ => l.as_i64()? == r.as_i64()?,
            }),
            BinOp::Ne => Ok(match (l, r) {
                (Value::Obj(a), Value::Obj(b)) => !Rc::ptr_eq(a, b),
                (Value::Obj(_), Value::Int(0)) | (Value::Int(0), Value::Obj(_)) => true,
                (Value::Str(a), Value::Str(b)) => a != b,
                (Value::Float(_), _) | (_, Value::Float(_)) => l.as_f64()? != r.as_f64()?,
                _ => l.as_i64()? != r.as_i64()?,
            }),
            BinOp::Lt => Ok(match (l, r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => l.as_f64()? < r.as_f64()?,
                _ => l.as_i64()? < r.as_i64()?,
            }),
            BinOp::Le => Ok(match (l, r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => l.as_f64()? <= r.as_f64()?,
                _ => l.as_i64()? <= r.as_i64()?,
            }),
            BinOp::Gt => Ok(match (l, r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => l.as_f64()? > r.as_f64()?,
                _ => l.as_i64()? > r.as_i64()?,
            }),
            BinOp::Ge => Ok(match (l, r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => l.as_f64()? >= r.as_f64()?,
                _ => l.as_i64()? >= r.as_i64()?,
            }),
            other => Err(format!("unsupported compare op in chain: {other:?}")),
        }
    }

    fn eval_index(&self, base: Value, idx: i64) -> Result<Value, String> {
        match base {
            Value::Array(arr) => {
                let idx: usize = idx
                    .try_into()
                    .map_err(|_| "index must be non-negative".to_string())?;
                let arr = arr.borrow();
                arr.elems
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| "index out of range".to_string())
            }
            Value::ArrayPtr { arr, index } => {
                let abs = index
                    .checked_add(idx)
                    .ok_or_else(|| "index overflow".to_string())?;
                let abs: usize = abs
                    .try_into()
                    .map_err(|_| "index out of range".to_string())?;
                let arr = arr.borrow();
                arr.elems
                    .get(abs)
                    .cloned()
                    .ok_or_else(|| "index out of range".to_string())
            }
            Value::Ptr { addr, elem_bytes } => {
                let elem_bytes = elem_bytes.max(1);
                let scaled = (idx as i128)
                    .saturating_mul(elem_bytes as i128)
                    .clamp(i64::MIN as i128, i64::MAX as i128) as i64;
                let target = addr.saturating_add(scaled);
                if elem_bytes == 1 {
                    Ok(Value::Int(self.heap_read_u8(target)? as i64))
                } else {
                    Ok(Value::Int(self.heap_read_i64_le(target, elem_bytes)?))
                }
            }
            Value::Int(addr) => {
                let target = addr.saturating_add(idx);
                Ok(Value::Int(self.heap_read_u8(target)? as i64))
            }
            Value::IntView {
                value,
                elem_bytes,
                signed,
            } => {
                let idx_u64: u64 = idx
                    .try_into()
                    .map_err(|_| "index must be non-negative".to_string())?;
                let elem_bits = (elem_bytes as u32) * 8;
                if elem_bits == 0 || elem_bits > 64 {
                    return Err("invalid sub-integer view".to_string());
                }
                let shift = idx_u64
                    .checked_mul(elem_bits as u64)
                    .ok_or_else(|| "index overflow".to_string())?;
                if shift >= 64 {
                    return Err("index out of range".to_string());
                }
                let mask = if elem_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let raw = (value >> shift) & mask;
                if signed {
                    let sh = 64u32 - elem_bits;
                    let signed_v = ((raw << sh) as i64) >> sh;
                    Ok(Value::Int(signed_v))
                } else {
                    Ok(Value::Int(raw as i64))
                }
            }
            _ => Err(
                "indexing expects an array, pointer, or sub-integer view (e.g. x.u8[i])"
                    .to_string(),
            ),
        }
    }

    pub(super) fn get_field(&self, base: Value, field: &str) -> Result<Value, String> {
        match base {
            Value::Obj(obj) => obj
                .borrow()
                .fields
                .get(field)
                .cloned()
                .ok_or_else(|| format!("unknown field: {field}")),
            Value::VarRef(name) => {
                let v = self
                    .env
                    .get(&name)
                    .ok_or_else(|| format!("unknown variable: {name}"))?;
                self.get_field(v, field)
            }
            Value::ArrayPtr { arr, index } => {
                let idx: usize = index
                    .try_into()
                    .map_err(|_| "array pointer: negative index".to_string())?;
                let elem = {
                    let arr = arr.borrow();
                    arr.elems
                        .get(idx)
                        .cloned()
                        .ok_or_else(|| "array pointer out of range".to_string())?
                };
                self.get_field(elem, field)
            }
            Value::Int(v) => self.get_subint_view(v as u64, field),
            Value::Char(v) => self.get_subint_view(v, field),
            _ => Err(format!("cannot access field {field} on non-object")),
        }
    }

    fn get_subint_view(&self, value: u64, field: &str) -> Result<Value, String> {
        // TempleOS-ish convenience fields for `CDate` (an `I64 class` with `U32 time; I32 date;`).
        // We expose them as if they were universally available on integers; this matches a common
        // HolyC pattern in the upstream sources.
        match field {
            "time" => return Ok(Value::Int((value & 0xffff_ffff) as u32 as i64)),
            "date" => {
                let date = ((value >> 32) as u32) as i32;
                return Ok(Value::Int(date as i64));
            }
            _ => {}
        }

        let (elem_bytes, signed) = match field {
            "u8" => (1u8, false),
            "i8" => (1u8, true),
            "u16" => (2u8, false),
            "i16" => (2u8, true),
            "u32" => (4u8, false),
            "i32" => (4u8, true),
            "u64" => (8u8, false),
            "i64" => (8u8, true),
            _ => return Err(format!("unknown field: {field}")),
        };
        Ok(Value::IntView {
            value,
            elem_bytes,
            signed,
        })
    }

    pub(super) fn set_field(&self, base: Value, field: &str, value: Value) -> Result<(), String> {
        match base {
            Value::Obj(obj) => {
                obj.borrow_mut().fields.insert(field.to_string(), value);
                Ok(())
            }
            Value::VarRef(name) => {
                let v = self
                    .env
                    .get(&name)
                    .ok_or_else(|| format!("unknown variable: {name}"))?;
                let Value::Obj(obj) = v else {
                    return Err(format!("cannot set field {field} on non-object"));
                };
                obj.borrow_mut().fields.insert(field.to_string(), value);
                Ok(())
            }
            Value::ArrayPtr { arr, index } => {
                let idx: usize = index
                    .try_into()
                    .map_err(|_| "array pointer: negative index".to_string())?;
                let elem = {
                    let arr = arr.borrow();
                    arr.elems
                        .get(idx)
                        .cloned()
                        .ok_or_else(|| "array pointer out of range".to_string())?
                };
                self.set_field(elem, field, value)
            }
            _ => Err(format!("cannot set field {field} on non-object")),
        }
    }

    pub(super) fn assign_lhs(&mut self, lhs: &Expr, value: Value) -> Result<(), String> {
        match lhs {
            Expr::Var(name) => self.env.assign(name, value),
            Expr::Deref(inner) => {
                let ptr = self.eval_expr(inner)?;
                match ptr {
                    Value::VarRef(name) => self.env.assign(&name, value),
                    Value::Ptr { addr, elem_bytes } => {
                        if elem_bytes == 1 {
                            self.heap_write_u8(addr, value.as_i64()? as u8)
                        } else {
                            self.heap_write_i64_le(addr, elem_bytes, value.as_i64()?)
                        }
                    }
                    Value::ArrayPtr { arr, index } => {
                        let idx: usize = index
                            .try_into()
                            .map_err(|_| "deref assignment: negative array pointer".to_string())?;
                        let mut arr = arr.borrow_mut();
                        if idx >= arr.elems.len() {
                            return Err("deref assignment: array pointer out of range".to_string());
                        }
                        arr.elems[idx] = value;
                        Ok(())
                    }
                    Value::ObjFieldRef { obj, field } => {
                        obj.borrow_mut().fields.insert(field, value);
                        Ok(())
                    }
                    Value::Int(addr) => self.heap_write_u8(addr, value.as_i64()? as u8),
                    _ => Err("deref assignment expects a pointer".to_string()),
                }
            }
            Expr::Member { base, field } | Expr::PtrMember { base, field } => {
                let base = self.eval_expr(base)?;
                self.set_field(base, field, value)
            }
            Expr::Index { base, index } => {
                let base_expr = base.as_ref();
                let base = self.eval_expr(base_expr)?;
                let idx = self.eval_expr(index)?.as_i64()?;
                let idx: usize = idx
                    .try_into()
                    .map_err(|_| "index must be non-negative".to_string())?;
                match base {
                    Value::Array(arr) => {
                        let set_font_glyph = idx < 256 && Self::is_text_font_expr(base_expr);
                        let value_bits = if set_font_glyph {
                            Some(match &value {
                                Value::Char(v) => *v,
                                _ => value.as_i64()? as u64,
                            })
                        } else {
                            None
                        };

                        let mut arr = arr.borrow_mut();
                        if idx >= arr.elems.len() {
                            return Err("index out of range".to_string());
                        }
                        arr.elems[idx] = value;

                        if let Some(bits) = value_bits {
                            self.rt.set_font_glyph_u64(idx as u8, bits);
                        }
                        Ok(())
                    }
                    Value::ArrayPtr {
                        arr,
                        index: base_idx,
                    } => {
                        let abs_idx = base_idx
                            .checked_add(idx as i64)
                            .ok_or_else(|| "index overflow".to_string())?;
                        let abs_idx: usize = abs_idx
                            .try_into()
                            .map_err(|_| "index out of range".to_string())?;
                        let mut arr = arr.borrow_mut();
                        if abs_idx >= arr.elems.len() {
                            return Err("index out of range".to_string());
                        }
                        arr.elems[abs_idx] = value;
                        Ok(())
                    }
                    Value::Ptr { addr, elem_bytes } => {
                        let scaled = (idx as i128)
                            .saturating_mul(elem_bytes as i128)
                            .clamp(i64::MIN as i128, i64::MAX as i128)
                            as i64;
                        let target = addr.saturating_add(scaled);
                        if elem_bytes == 1 {
                            self.heap_write_u8(target, value.as_i64()? as u8)
                        } else {
                            self.heap_write_i64_le(target, elem_bytes, value.as_i64()?)
                        }
                    }
                    Value::Int(addr) => {
                        let target = addr.saturating_add(idx as i64);
                        self.heap_write_u8(target, value.as_i64()? as u8)
                    }
                    _ => Err("index assignment is only supported on arrays".to_string()),
                }
            }
            _ => Err("invalid assignment target".to_string()),
        }
    }

    fn eval_bin(&self, op: BinOp, l: Value, r: Value) -> Result<Value, String> {
        match op {
            BinOp::Add => {
                if matches!((&l, &r), (Value::Float(_), _) | (_, Value::Float(_))) {
                    Ok(Value::Float(l.as_f64()? + r.as_f64()?))
                } else {
                    match (l, r) {
                        (Value::Ptr { addr, elem_bytes }, Value::Int(delta))
                        | (Value::Int(delta), Value::Ptr { addr, elem_bytes }) => {
                            let scaled = (delta as i128)
                                .saturating_mul(elem_bytes as i128)
                                .clamp(i64::MIN as i128, i64::MAX as i128)
                                as i64;
                            Ok(Value::Ptr {
                                addr: addr.saturating_add(scaled),
                                elem_bytes,
                            })
                        }
                        (Value::ArrayPtr { arr, index }, Value::Int(delta))
                        | (Value::Int(delta), Value::ArrayPtr { arr, index }) => {
                            Ok(Value::ArrayPtr {
                                arr,
                                index: index.saturating_add(delta),
                            })
                        }
                        (l, r) => Ok(Value::Int(l.as_i64()? + r.as_i64()?)),
                    }
                }
            }
            BinOp::Sub => {
                if matches!((&l, &r), (Value::Float(_), _) | (_, Value::Float(_))) {
                    Ok(Value::Float(l.as_f64()? - r.as_f64()?))
                } else {
                    match (l, r) {
                        (Value::Ptr { addr, elem_bytes }, Value::Int(delta)) => {
                            let scaled = (delta as i128)
                                .saturating_mul(elem_bytes as i128)
                                .clamp(i64::MIN as i128, i64::MAX as i128)
                                as i64;
                            Ok(Value::Ptr {
                                addr: addr.saturating_sub(scaled),
                                elem_bytes,
                            })
                        }
                        (Value::ArrayPtr { arr, index }, Value::Int(delta)) => {
                            Ok(Value::ArrayPtr {
                                arr,
                                index: index.saturating_sub(delta),
                            })
                        }
                        (Value::Ptr { addr: a, .. }, Value::Ptr { addr: b, .. }) => {
                            Ok(Value::Int(a.saturating_sub(b)))
                        }
                        (l, r) => Ok(Value::Int(l.as_i64()? - r.as_i64()?)),
                    }
                }
            }
            BinOp::Mul => {
                if matches!((&l, &r), (Value::Float(_), _) | (_, Value::Float(_))) {
                    Ok(Value::Float(l.as_f64()? * r.as_f64()?))
                } else {
                    Ok(Value::Int(l.as_i64()? * r.as_i64()?))
                }
            }
            BinOp::Div => {
                if matches!((&l, &r), (Value::Float(_), _) | (_, Value::Float(_))) {
                    let d = r.as_f64()?;
                    if d == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    return Ok(Value::Float(l.as_f64()? / d));
                }
                let d = r.as_i64()?;
                if d == 0 {
                    return Err("division by zero".to_string());
                }
                Ok(Value::Int(l.as_i64()? / d))
            }
            BinOp::Rem => {
                if matches!((&l, &r), (Value::Float(_), _) | (_, Value::Float(_))) {
                    let d = r.as_f64()?;
                    if d == 0.0 {
                        return Err("modulo by zero".to_string());
                    }
                    return Ok(Value::Float(l.as_f64()? % d));
                }
                let d = r.as_i64()?;
                if d == 0 {
                    return Err("modulo by zero".to_string());
                }
                Ok(Value::Int(l.as_i64()? % d))
            }
            BinOp::Comma => Ok(r),
            BinOp::Shl => {
                let lhs = l.as_i64()?;
                let rhs = r.as_i64()?;
                if rhs < 0 {
                    return Err("shift count must be non-negative".to_string());
                }
                let sh = (rhs as u64 & 63) as u32;
                Ok(Value::Int((lhs << sh) as i64))
            }
            BinOp::Shr => {
                let lhs = l.as_i64()?;
                let rhs = r.as_i64()?;
                if rhs < 0 {
                    return Err("shift count must be non-negative".to_string());
                }
                let sh = (rhs as u64 & 63) as u32;
                Ok(Value::Int((lhs >> sh) as i64))
            }
            BinOp::Eq => Ok(Value::Int(match (&l, &r) {
                (Value::Obj(a), Value::Obj(b)) => Rc::ptr_eq(a, b) as i64,
                (Value::Obj(_), Value::Int(0)) | (Value::Int(0), Value::Obj(_)) => 0,
                (Value::ArrayPtr { arr: a, index: ia }, Value::ArrayPtr { arr: b, index: ib }) => {
                    (Rc::ptr_eq(a, b) && ia == ib) as i64
                }
                (Value::ArrayPtr { .. }, Value::Int(0))
                | (Value::Int(0), Value::ArrayPtr { .. }) => 0,
                (Value::Str(a), Value::Str(b)) => (a == b) as i64,
                (Value::Float(_), _) | (_, Value::Float(_)) => (l.as_f64()? == r.as_f64()?) as i64,
                _ => (l.as_i64()? == r.as_i64()?) as i64,
            })),
            BinOp::Ne => Ok(Value::Int(match (&l, &r) {
                (Value::Obj(a), Value::Obj(b)) => (!Rc::ptr_eq(a, b)) as i64,
                (Value::Obj(_), Value::Int(0)) | (Value::Int(0), Value::Obj(_)) => 1,
                (Value::ArrayPtr { arr: a, index: ia }, Value::ArrayPtr { arr: b, index: ib }) => {
                    (!Rc::ptr_eq(a, b) || ia != ib) as i64
                }
                (Value::ArrayPtr { .. }, Value::Int(0))
                | (Value::Int(0), Value::ArrayPtr { .. }) => 1,
                (Value::Str(a), Value::Str(b)) => (a != b) as i64,
                (Value::Float(_), _) | (_, Value::Float(_)) => (l.as_f64()? != r.as_f64()?) as i64,
                _ => (l.as_i64()? != r.as_i64()?) as i64,
            })),
            BinOp::Lt => Ok(Value::Int(match (&l, &r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => (l.as_f64()? < r.as_f64()?) as i64,
                _ => (l.as_i64()? < r.as_i64()?) as i64,
            })),
            BinOp::Le => Ok(Value::Int(match (&l, &r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => (l.as_f64()? <= r.as_f64()?) as i64,
                _ => (l.as_i64()? <= r.as_i64()?) as i64,
            })),
            BinOp::Gt => Ok(Value::Int(match (&l, &r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => (l.as_f64()? > r.as_f64()?) as i64,
                _ => (l.as_i64()? > r.as_i64()?) as i64,
            })),
            BinOp::Ge => Ok(Value::Int(match (&l, &r) {
                (Value::Float(_), _) | (_, Value::Float(_)) => (l.as_f64()? >= r.as_f64()?) as i64,
                _ => (l.as_i64()? >= r.as_i64()?) as i64,
            })),
            BinOp::BitAnd => Ok(Value::Int(l.as_i64()? & r.as_i64()?)),
            BinOp::BitXor => Ok(Value::Int(l.as_i64()? ^ r.as_i64()?)),
            BinOp::BitOr => Ok(Value::Int(l.as_i64()? | r.as_i64()?)),
            BinOp::And => Ok(Value::Int((l.truthy() && r.truthy()) as i64)),
            BinOp::Or => Ok(Value::Int((l.truthy() || r.truthy()) as i64)),
        }
    }

    pub(super) fn eval_arg_i64(&mut self, args: &[Expr], idx: usize) -> Result<i64, String> {
        let Some(expr) = args.get(idx) else {
            return Err(format!("missing argument {idx}"));
        };
        self.eval_expr(expr)?.as_i64()
    }

    pub(super) fn eval_arg_f64(&mut self, args: &[Expr], idx: usize) -> Result<f64, String> {
        let Some(expr) = args.get(idx) else {
            return Err(format!("missing argument {idx}"));
        };
        self.eval_expr(expr)?.as_f64()
    }

    pub(super) fn type_size_bytes(ty: &str, pointer: bool) -> usize {
        if pointer {
            return 8;
        }
        match ty {
            "U0" => 0,
            "I8" | "U8" => 1,
            "I16" | "U16" => 2,
            "I32" | "U32" | "F32" => 4,
            "I64" | "U64" | "F64" | "Bool" => 8,
            _ => 8,
        }
    }

    fn cast_int_bits(raw: u64, elem_bits: u32, signed: bool) -> i64 {
        if elem_bits == 0 {
            return 0;
        }

        if elem_bits >= 64 {
            return raw as i64;
        }

        let mask = (1u64 << elem_bits) - 1;
        let raw = raw & mask;
        if signed {
            let sh = 64u32 - elem_bits;
            ((raw << sh) as i64) >> sh
        } else {
            raw as i64
        }
    }

    fn cast_raw_bits_to_value(raw: u64, ty: &str, size_bytes: usize) -> Value {
        match ty {
            "F64" => Value::Float(f64::from_bits(raw)),
            "F32" => Value::Float(f32::from_bits(raw as u32) as f64),
            "Bool" => Value::Int((raw != 0) as i64),
            _ => {
                let signed = ty.starts_with('I');
                let elem_bits = (size_bytes as u32).saturating_mul(8);
                Value::Int(Self::cast_int_bits(raw, elem_bits, signed))
            }
        }
    }

    fn is_text_font_expr(expr: &Expr) -> bool {
        let Expr::Member { base, field } = expr else {
            return false;
        };
        if field != "font" {
            return false;
        }
        matches!(base.as_ref(), Expr::Var(name) if name == "text")
    }

    fn try_read_u64_from_lvalue_expr(
        &mut self,
        expr: &Expr,
        size: usize,
    ) -> Result<Option<u64>, String> {
        if size > 8 {
            return Ok(None);
        }

        match expr {
            Expr::Var(name) => {
                let Some(Value::Array(arr)) = self.env.get(name) else {
                    return Ok(None);
                };
                let arr = arr.borrow();
                Self::read_u64_from_array(&arr, 0, size).map(Some)
            }
            Expr::Index { base, index } => {
                let base = self.eval_expr(base)?;
                let idx = self.eval_expr(index)?.as_i64()?;
                let idx: usize = idx
                    .try_into()
                    .map_err(|_| "index must be non-negative".to_string())?;

                let Value::Array(arr) = base else {
                    return Ok(None);
                };
                let arr = arr.borrow();

                let elem_bytes = arr.elem_bytes.max(1);
                let start = idx
                    .checked_mul(elem_bytes)
                    .ok_or_else(|| "index overflow".to_string())?;
                Self::read_u64_from_array(&arr, start, size).map(Some)
            }
            _ => Ok(None),
        }
    }

    fn read_u64_from_array(
        arr: &ArrayValue,
        start_byte: usize,
        size: usize,
    ) -> Result<u64, String> {
        let total_bytes = arr
            .elems
            .len()
            .checked_mul(arr.elem_bytes.max(1))
            .ok_or_else(|| "array too large".to_string())?;

        let end = start_byte
            .checked_add(size)
            .ok_or_else(|| "cast read overflow".to_string())?;
        if end > total_bytes {
            return Err("cast read out of range".to_string());
        }

        let elem_bytes = arr.elem_bytes.max(1);
        let mut out = 0u64;
        for i in 0..size {
            let abs = start_byte + i;
            let elem_index = abs / elem_bytes;
            let within = abs % elem_bytes;
            let elem = arr
                .elems
                .get(elem_index)
                .ok_or_else(|| "cast read out of range".to_string())?;
            let elem_bits: u64 = match elem {
                Value::Int(v) => *v as u64,
                Value::Char(v) => *v,
                _ => return Err("cast read expects an integer array".to_string()),
            };
            let b = (elem_bits >> ((within as u32) * 8)) & 0xFF;
            out |= b << ((i as u32) * 8);
        }
        Ok(out)
    }

    fn sizeof_expr(&mut self, expr: &Expr) -> Result<i64, String> {
        if let Expr::Var(name) = expr {
            if is_type_name(name) || is_user_type_name(name) {
                return Ok(Self::type_size_bytes(name, false) as i64);
            }
        }

        let v = self.eval_expr(expr)?;
        match v {
            Value::Array(arr) => {
                let arr = arr.borrow();
                Ok((arr.elems.len() * arr.elem_bytes) as i64)
            }
            Value::Str(s) => Ok((s.len() + 1) as i64),
            _ => Ok(8),
        }
    }

    fn is_class_value_type(&self, ty: &str, pointer: bool) -> bool {
        !pointer && self.program.classes.contains_key(ty)
    }

    pub(super) fn default_value_for_type(
        &mut self,
        ty: &str,
        pointer: bool,
    ) -> Result<Value, String> {
        if pointer {
            return Ok(Value::Int(0));
        }

        if self.is_class_value_type(ty, pointer) {
            return self.alloc_class_value(ty);
        }

        if is_user_type_name(ty) {
            return Ok(Value::Obj(Rc::new(RefCell::new(Obj {
                fields: HashMap::new(),
            }))));
        }

        Ok(Value::Int(0))
    }

    fn eval_init_list_for_type(
        &mut self,
        ty: &str,
        pointer: bool,
        items: &[Expr],
    ) -> Result<Value, String> {
        if self.is_class_value_type(ty, pointer) {
            return self.eval_class_init_list(ty, items);
        }

        if items.len() == 1 {
            return self.eval_init_expr_for_type(ty, pointer, &items[0]);
        }

        Err(format!("initializer list is not supported for type {ty}"))
    }

    fn eval_init_expr_for_type(
        &mut self,
        ty: &str,
        pointer: bool,
        expr: &Expr,
    ) -> Result<Value, String> {
        match expr {
            Expr::InitList(items) => self.eval_init_list_for_type(ty, pointer, items),
            other => Ok(self.eval_expr(other)?),
        }
    }

    fn eval_class_init_list(&mut self, ty: &str, items: &[Expr]) -> Result<Value, String> {
        let Some(fields_def) = self.program.classes.get(ty).map(|def| def.fields.clone()) else {
            return Err(format!("unknown class: {ty}"));
        };

        let Value::Obj(obj) = self.alloc_class_value(ty)? else {
            return Err("internal error: alloc_class_value did not return an object".to_string());
        };

        for (idx, field) in fields_def.into_iter().enumerate() {
            let Some(init_expr) = items.get(idx) else {
                break;
            };

            let v = if !field.array_lens.is_empty() {
                self.eval_array_value(
                    &field.ty,
                    field.pointer,
                    &field.array_lens,
                    Some(init_expr),
                    &format!("{ty}.{}", field.name),
                )?
            } else {
                self.eval_init_expr_for_type(&field.ty, field.pointer, init_expr)?
            };

            obj.borrow_mut().fields.insert(field.name.clone(), v);
        }

        Ok(Value::Obj(obj))
    }

    pub(super) fn eval_array_value(
        &mut self,
        ty: &str,
        pointer: bool,
        array_lens: &[Expr],
        init: Option<&Expr>,
        ctx: &str,
    ) -> Result<Value, String> {
        let mut dims: Vec<usize> = Vec::with_capacity(array_lens.len());
        for len_expr in array_lens.iter() {
            let len_i64 = self.eval_expr(len_expr)?.as_i64()?;
            let len: usize = len_i64
                .try_into()
                .map_err(|_| "array size must be non-negative".to_string())?;
            dims.push(len);
        }

        let base_elem_bytes = Self::type_size_bytes(ty, pointer);
        let mut elem_bytes_at_level: Vec<usize> = Vec::with_capacity(dims.len());
        for i in 0..dims.len() {
            let inner = dims
                .get(i + 1..)
                .unwrap_or(&[])
                .iter()
                .fold(1usize, |acc, &v| acc.saturating_mul(v));
            elem_bytes_at_level.push(inner.saturating_mul(base_elem_bytes));
        }

        fn build_array_level(
            vm: &mut Vm,
            ty: &str,
            pointer: bool,
            dims: &[usize],
            elem_bytes_at_level: &[usize],
            level: usize,
            init: Option<&Expr>,
            ctx: &str,
        ) -> Result<Value, String> {
            let len = dims
                .get(level)
                .copied()
                .ok_or_else(|| "invalid array dimension".to_string())?;
            let elem_bytes = *elem_bytes_at_level
                .get(level)
                .ok_or_else(|| "invalid array element size".to_string())?;

            let init_items: Option<&[Expr]> = match init {
                None => None,
                Some(Expr::InitList(items)) => Some(items.as_slice()),
                Some(other) => {
                    return Err(format!(
                        "array initializer must be {{...}}, got {other:?} for {ctx}"
                    ));
                }
            };

            let mut elems: Vec<Value> = Vec::with_capacity(len);
            let is_innermost = level + 1 >= dims.len();
            if is_innermost {
                for i in 0..len {
                    let v = if let Some(items) = init_items {
                        if let Some(item_expr) = items.get(i) {
                            vm.eval_init_expr_for_type(ty, pointer, item_expr)?
                        } else {
                            vm.default_value_for_type(ty, pointer)?
                        }
                    } else {
                        vm.default_value_for_type(ty, pointer)?
                    };
                    elems.push(v);
                }
            } else {
                for i in 0..len {
                    let inner_init = init_items.and_then(|items| items.get(i));
                    elems.push(build_array_level(
                        vm,
                        ty,
                        pointer,
                        dims,
                        elem_bytes_at_level,
                        level + 1,
                        inner_init,
                        ctx,
                    )?);
                }
            }

            Ok(Value::Array(Rc::new(RefCell::new(ArrayValue {
                elems,
                elem_bytes,
            }))))
        }

        build_array_level(self, ty, pointer, &dims, &elem_bytes_at_level, 0, init, ctx)
    }

    fn default_value_for_decl(&mut self, decl: &Decl) -> Result<Value, String> {
        self.default_value_for_type(&decl.ty, decl.pointer)
    }

    pub(super) fn eval_decl_value(&mut self, decl: &Decl) -> Result<Value, String> {
        if decl.array_lens.is_empty() {
            if let Some(expr) = decl.init.as_ref() {
                return self.eval_init_expr_for_type(&decl.ty, decl.pointer, expr);
            }
            return self.default_value_for_type(&decl.ty, decl.pointer);
        }

        self.eval_array_value(
            &decl.ty,
            decl.pointer,
            &decl.array_lens,
            decl.init.as_ref(),
            &decl.name,
        )
    }
}
