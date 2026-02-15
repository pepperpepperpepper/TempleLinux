use super::super::prelude::*;
use super::super::{ArrayRef, EnvScopeGuard, Obj, Value, Vm};

impl Vm {
    pub(super) fn call_builtin_core(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        match name {
            "Clear" => {
                let c = self.eval_arg_i64(args, 0)? as u8;
                self.rt.clear(c);
                Ok(Value::Void)
            }
            "Now" => {
                if !args.is_empty() {
                    return Err("Now expects 0 args".to_string());
                }
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| "Now: system time is before UNIX_EPOCH".to_string())?;
                let secs = now.as_secs() as i64;
                let nanos = now.subsec_nanos() as i64;

                let days = (secs / 86_400) as i64;
                let secs_in_day = secs.rem_euclid(86_400);
                let mut ticks = secs_in_day
                    .saturating_mul(CDATE_FREQ_HZ)
                    .saturating_add(nanos.saturating_mul(CDATE_FREQ_HZ) / 1_000_000_000);
                if ticks < 0 {
                    ticks = 0;
                }
                if ticks > u32::MAX as i64 {
                    ticks = u32::MAX as i64;
                }
                let packed = (days << 32) | (ticks as u32 as i64);
                Ok(Value::Int(packed))
            }
            "tS" => {
                if !args.is_empty() {
                    return Err("tS expects 0 args".to_string());
                }
                if let Some(v) = self.fixed_ts {
                    return Ok(Value::Float(v));
                }
                Ok(Value::Float(self.start_instant.elapsed().as_secs_f64()))
            }
            "Tri" => {
                if args.len() != 2 {
                    return Err("Tri(t, period) expects 2 args".to_string());
                }
                let t = self.eval_expr(&args[0])?.as_f64()?;
                let mut period = self.eval_expr(&args[1])?.as_f64()?;
                if period == 0.0 {
                    return Ok(Value::Float(0.0));
                }
                if period < 0.0 {
                    period = -period;
                }
                let mut tt = (t.abs() % period) / period;
                tt *= 2.0;
                let out = if tt <= 1.0 { tt } else { 2.0 - tt };
                Ok(Value::Float(out))
            }
            "Abs" => {
                if args.len() != 1 {
                    return Err("Abs(x) expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                match v {
                    Value::Float(f) => Ok(Value::Float(f.abs())),
                    _ => {
                        let i = v.as_i64()?;
                        let out = if i == i64::MIN { i64::MAX } else { i.abs() };
                        Ok(Value::Int(out))
                    }
                }
            }
            "Max" => {
                if args.len() != 2 {
                    return Err("Max(a, b) expects 2 args".to_string());
                }
                let a = self.eval_expr(&args[0])?;
                let b = self.eval_expr(&args[1])?;
                if matches!(a, Value::Float(_)) || matches!(b, Value::Float(_)) {
                    let af = a.as_f64()?;
                    let bf = b.as_f64()?;
                    Ok(Value::Float(af.max(bf)))
                } else {
                    let ai = a.as_i64()?;
                    let bi = b.as_i64()?;
                    Ok(Value::Int(ai.max(bi)))
                }
            }
            "Sqr" => {
                if args.len() != 1 {
                    return Err("Sqr(x) expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                match v {
                    Value::Float(f) => Ok(Value::Float(f * f)),
                    _ => {
                        let i = v.as_i64()?;
                        let prod = (i as i128).saturating_mul(i as i128);
                        let clamped = prod.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
                        Ok(Value::Int(clamped))
                    }
                }
            }
            "Cos" => {
                if args.len() != 1 {
                    return Err("Cos(x) expects 1 arg".to_string());
                }
                let x = self.eval_expr(&args[0])?.as_f64()?;
                Ok(Value::Float(x.cos()))
            }
            "Sin" => {
                if args.len() != 1 {
                    return Err("Sin(x) expects 1 arg".to_string());
                }
                let x = self.eval_expr(&args[0])?.as_f64()?;
                Ok(Value::Float(x.sin()))
            }
            "Sqrt" => {
                if args.len() != 1 {
                    return Err("Sqrt(x) expects 1 arg".to_string());
                }
                let x = self.eval_expr(&args[0])?.as_f64()?;
                Ok(Value::Float(x.sqrt()))
            }
            "Exp" => {
                if args.len() != 1 {
                    return Err("Exp(x) expects 1 arg".to_string());
                }
                let x = self.eval_expr(&args[0])?.as_f64()?;
                Ok(Value::Float(x.exp()))
            }
            "Arg" => {
                if args.len() != 2 {
                    return Err("Arg(x, y) expects 2 args".to_string());
                }
                let x = self.eval_expr(&args[0])?.as_f64()?;
                let y = self.eval_expr(&args[1])?.as_f64()?;
                Ok(Value::Float(y.atan2(x)))
            }
            "ToI64" => {
                if args.len() != 1 {
                    return Err("ToI64(x) expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                match v {
                    Value::Float(f) => Ok(Value::Int(f as i64)),
                    _ => Ok(Value::Int(v.as_i64()?)),
                }
            }
            "Noise" => {
                if args.len() != 3 {
                    return Err("Noise(ms, ona0, ona1) expects 3 args".to_string());
                }
                let _ms = self.eval_expr(&args[0])?;
                let _ona0 = self.eval_expr(&args[1])?;
                let _ona1 = self.eval_expr(&args[2])?;
                Ok(Value::Void)
            }
            "MAlloc" | "CAlloc" | "ACAlloc" => {
                if args.len() != 1 {
                    return Err(format!("{name}(size) expects 1 arg"));
                }
                let zeroed = name != "MAlloc";

                if let Expr::SizeOf(inner) = &args[0] {
                    if let Expr::Var(type_name) = inner.as_ref() {
                        if let Some(def) = self.program.classes.get(type_name) {
                            if def.base_ty.is_none() {
                                return self.alloc_class_value(type_name);
                            }
                        }
                        if is_user_type_name(type_name) {
                            return Ok(Value::Obj(Rc::new(RefCell::new(Obj {
                                fields: HashMap::new(),
                            }))));
                        }
                    }
                }

                let size_i64 = self.eval_expr(&args[0])?.as_i64()?;
                if size_i64 < 0 {
                    return Err(format!("{name}: size must be non-negative"));
                }
                let size: usize = size_i64
                    .try_into()
                    .map_err(|_| format!("{name}: size out of range"))?;
                let addr = self.heap_alloc(size, zeroed);
                Ok(Value::Int(addr))
            }
            "Free" => {
                if args.len() != 1 {
                    return Err("Free(ptr) expects 1 arg".to_string());
                }
                // No-op for now (we don't reclaim heap storage; objects are GC'd by Rust).
                let _ = self.eval_expr(&args[0])?;
                Ok(Value::Void)
            }
            "FileRead" => {
                if args.len() != 1 {
                    return Err("FileRead(path) expects 1 arg".to_string());
                }
                let path = match self.eval_expr(&args[0])? {
                    Value::Str(s) => s,
                    _ => return Err("FileRead: path must be a string".to_string()),
                };
                let host_path = self.resolve_temple_fs_target_read(&path)?;
                let bytes = match std::fs::read(&host_path) {
                    Ok(b) => b,
                    Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Value::Int(0)),
                    Err(err) => {
                        return Err(format!("FileRead: {}: {err}", host_path.display()));
                    }
                };
                let addr = self.heap_alloc(bytes.len() + 1, true);
                self.heap_write_bytes(addr, &bytes)?;
                // Ensure a trailing 0 so HolyC code that expects a terminator won't run off.
                let _ = self.heap_write_u8(addr + bytes.len() as i64, 0);
                Ok(Value::Int(addr))
            }
            "FileWrite" => {
                if args.len() != 3 {
                    return Err("FileWrite(path, buf, size) expects 3 args".to_string());
                }
                let path = match self.eval_expr(&args[0])? {
                    Value::Str(s) => s,
                    _ => return Err("FileWrite: path must be a string".to_string()),
                };
                let buf = self.eval_expr(&args[1])?.as_i64()?;
                let size_i64 = self.eval_expr(&args[2])?.as_i64()?;
                if size_i64 < 0 {
                    return Err("FileWrite: size must be non-negative".to_string());
                }
                let size: usize = size_i64
                    .try_into()
                    .map_err(|_| "FileWrite: size out of range".to_string())?;

                let host_path = self.resolve_temple_fs_target_write(&path)?;
                if let Some(parent) = host_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|err| format!("FileWrite: {}: {err}", parent.display()))?;
                }
                let bytes = if size == 0 {
                    Vec::new()
                } else {
                    self.heap_slice(buf, size)?.to_vec()
                };
                std::fs::write(&host_path, bytes)
                    .map_err(|err| format!("FileWrite: {}: {err}", host_path.display()))?;
                Ok(Value::Int(1))
            }
            "StrLen" => {
                if args.len() != 1 {
                    return Err("StrLen(st) expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                match v {
                    Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                    other => {
                        let addr = other.as_i64()?;
                        if addr == 0 {
                            return Ok(Value::Int(0));
                        }
                        let mut len: i64 = 0;
                        for _ in 0..(1 << 20) {
                            let b = self.heap_read_u8(addr + len)?;
                            if b == 0 {
                                break;
                            }
                            len = len.saturating_add(1);
                        }
                        Ok(Value::Int(len))
                    }
                }
            }
            "StrNew" => {
                if args.len() != 1 {
                    return Err("StrNew(st) expects 1 arg".to_string());
                }
                let v = self.eval_expr(&args[0])?;
                match v {
                    Value::Int(0) => Ok(Value::Int(0)),
                    Value::Str(s) => {
                        let bytes = s.as_bytes();
                        let dst = self.heap_alloc(bytes.len() + 1, true);
                        self.heap_write_bytes(dst, bytes)?;
                        let _ = self.heap_write_u8(dst + bytes.len() as i64, 0);
                        Ok(Value::Int(dst))
                    }
                    other => {
                        let src = other.as_i64()?;
                        if src == 0 {
                            return Ok(Value::Int(0));
                        }
                        let len = match self.call("StrLen", args)? {
                            Value::Int(v) => v as usize,
                            _ => return Err("StrNew: StrLen returned non-int".to_string()),
                        };
                        let dst = self.heap_alloc(len + 1, true);
                        let bytes = self.heap_slice(src, len + 1)?.to_vec();
                        self.heap_write_bytes(dst, &bytes)?;
                        Ok(Value::Int(dst))
                    }
                }
            }
            "StrCpy" => {
                if args.len() != 2 {
                    return Err("StrCpy(dst, src) expects 2 args".to_string());
                }
                let dst = self.eval_expr(&args[0])?.as_i64()?;
                let src_v = self.eval_expr(&args[1])?;
                if dst == 0 {
                    return Err("StrCpy: dst must be non-NULL".to_string());
                }

                let bytes: Vec<u8> = match src_v {
                    Value::Int(0) => vec![0u8],
                    Value::Str(s) => {
                        let mut b = s.into_bytes();
                        b.push(0);
                        b
                    }
                    other => {
                        let src = other.as_i64()?;
                        if src == 0 {
                            vec![0u8]
                        } else {
                            let len = self
                                .call("StrLen", std::slice::from_ref(&args[1]))?
                                .as_i64()? as usize;
                            self.heap_slice(src, len + 1)?.to_vec()
                        }
                    }
                };

                self.heap_write_bytes(dst, &bytes)?;
                Ok(Value::Int(dst))
            }
            "QueInit" => {
                if args.len() != 1 {
                    return Err("QueInit(head) expects 1 arg".to_string());
                }
                let head = self.eval_expr(&args[0])?;
                let Value::Obj(head) = head else {
                    return Err("QueInit: head must be a class/struct pointer".to_string());
                };
                head.borrow_mut()
                    .fields
                    .insert("next".to_string(), Value::Obj(head.clone()));
                head.borrow_mut()
                    .fields
                    .insert("last".to_string(), Value::Obj(head.clone()));
                Ok(Value::Void)
            }
            "QueIns" => {
                if args.len() != 2 {
                    return Err("QueIns(entry, pred) expects 2 args".to_string());
                }
                let entry = self.eval_expr(&args[0])?;
                let pred = self.eval_expr(&args[1])?;
                let (Value::Obj(entry), Value::Obj(pred)) = (entry, pred) else {
                    return Err("QueIns: entry/pred must be class/struct pointers".to_string());
                };

                let succ = pred
                    .borrow()
                    .fields
                    .get("next")
                    .cloned()
                    .ok_or_else(|| "QueIns: pred.next is missing".to_string())?;
                let Value::Obj(succ) = succ else {
                    return Err("QueIns: pred.next must be a class/struct pointer".to_string());
                };

                entry
                    .borrow_mut()
                    .fields
                    .insert("next".to_string(), Value::Obj(succ.clone()));
                entry
                    .borrow_mut()
                    .fields
                    .insert("last".to_string(), Value::Obj(pred.clone()));

                pred.borrow_mut()
                    .fields
                    .insert("next".to_string(), Value::Obj(entry.clone()));
                succ.borrow_mut()
                    .fields
                    .insert("last".to_string(), Value::Obj(entry.clone()));

                Ok(Value::Void)
            }
            "QueRem" => {
                if args.len() != 1 {
                    return Err("QueRem(entry) expects 1 arg".to_string());
                }
                let entry = self.eval_expr(&args[0])?;
                let Value::Obj(entry) = entry else {
                    return Err("QueRem: entry must be a class/struct pointer".to_string());
                };
                let pred = entry
                    .borrow()
                    .fields
                    .get("last")
                    .cloned()
                    .ok_or_else(|| "QueRem: entry.last is missing".to_string())?;
                let succ = entry
                    .borrow()
                    .fields
                    .get("next")
                    .cloned()
                    .ok_or_else(|| "QueRem: entry.next is missing".to_string())?;
                let (Value::Obj(pred), Value::Obj(succ)) = (pred, succ) else {
                    return Err("QueRem: entry.next/last must be class/struct pointers".to_string());
                };
                pred.borrow_mut()
                    .fields
                    .insert("next".to_string(), Value::Obj(succ.clone()));
                succ.borrow_mut()
                    .fields
                    .insert("last".to_string(), Value::Obj(pred.clone()));
                Ok(Value::Void)
            }
            "MemSet" => {
                if args.len() != 3 {
                    return Err("MemSet(dst, val, count) expects 3 args".to_string());
                }
                let dst = self.eval_expr(&args[0])?;
                let val = self.eval_expr(&args[1])?.as_i64()?;
                let count_i64 = self.eval_expr(&args[2])?.as_i64()?;
                let count: usize = count_i64
                    .try_into()
                    .map_err(|_| "MemSet: count must be non-negative".to_string())?;

                fn memset_value(v: &mut Value, val: i64) {
                    match v {
                        Value::Obj(obj) => {
                            let mut o = obj.borrow_mut();
                            for field_v in o.fields.values_mut() {
                                memset_value(field_v, val);
                            }
                        }
                        Value::Array(arr) => {
                            let mut a = arr.borrow_mut();
                            for elem in a.elems.iter_mut() {
                                memset_value(elem, val);
                            }
                        }
                        _ => {
                            *v = Value::Int(val);
                        }
                    }
                }

                match dst {
                    Value::Array(arr) => {
                        let (elem_bytes, len) = {
                            let a = arr.borrow();
                            (a.elem_bytes.max(1), a.elems.len())
                        };
                        let elems_to_set = (count / elem_bytes).min(len);
                        let mut a = arr.borrow_mut();
                        for i in 0..elems_to_set {
                            memset_value(&mut a.elems[i], val);
                        }
                        Ok(Value::Void)
                    }
                    Value::Obj(obj) => {
                        let _ = count;
                        let mut tmp = Value::Obj(obj);
                        memset_value(&mut tmp, val);
                        Ok(Value::Void)
                    }
                    Value::VarRef(name) => match self.env.get(&name) {
                        Some(Value::Obj(obj)) => {
                            let mut tmp = Value::Obj(obj);
                            memset_value(&mut tmp, val);
                            Ok(Value::Void)
                        }
                        Some(Value::Array(arr)) => {
                            let (elem_bytes, len) = {
                                let a = arr.borrow();
                                (a.elem_bytes.max(1), a.elems.len())
                            };
                            let elems_to_set = (count / elem_bytes).min(len);
                            let mut a = arr.borrow_mut();
                            for i in 0..elems_to_set {
                                memset_value(&mut a.elems[i], val);
                            }
                            Ok(Value::Void)
                        }
                        Some(_) => {
                            let _ = count;
                            self.env.assign(&name, Value::Int(val))?;
                            Ok(Value::Void)
                        }
                        None => Err("MemSet: dst must be an array, object, or pointer".to_string()),
                    },
                    Value::Ptr { addr, .. } | Value::Int(addr) => {
                        let b = val as u8;
                        for i in 0..count {
                            self.heap_write_u8(addr.saturating_add(i as i64), b)?;
                        }
                        Ok(Value::Void)
                    }
                    _ => Err("MemSet: dst must be an array, object, or pointer".to_string()),
                }
            }
            "MemSetU16" => {
                if args.len() != 3 {
                    return Err("MemSetU16(dst, val, count) expects 3 args".to_string());
                }
                let dst = self.eval_expr(&args[0])?;
                let val = self.eval_expr(&args[1])?.as_i64()?;
                let count_i64 = self.eval_expr(&args[2])?.as_i64()?;
                let mut remaining: usize = count_i64
                    .try_into()
                    .map_err(|_| "MemSetU16: count must be non-negative".to_string())?;

                fn fill_array(
                    arr: &ArrayRef,
                    val: i64,
                    remaining: &mut usize,
                ) -> Result<(), String> {
                    let mut a = arr.borrow_mut();
                    for elem in a.elems.iter_mut() {
                        if *remaining == 0 {
                            break;
                        }
                        match elem {
                            Value::Array(inner) => fill_array(inner, val, remaining)?,
                            _ => {
                                *elem = Value::Int(val);
                                *remaining -= 1;
                            }
                        }
                    }
                    Ok(())
                }

                match dst {
                    Value::Array(arr) => fill_array(&arr, val, &mut remaining)?,
                    Value::Ptr { addr, .. } => {
                        for i in 0..remaining {
                            let off = (i as i64).saturating_mul(2);
                            self.heap_write_i64_le(addr.saturating_add(off), 2, val)?;
                        }
                        remaining = 0;
                    }
                    Value::Int(addr) => {
                        for i in 0..remaining {
                            let off = (i as i64).saturating_mul(2);
                            self.heap_write_i64_le(addr.saturating_add(off), 2, val)?;
                        }
                        remaining = 0;
                    }
                    _ => return Err("MemSetU16: dst must be an array or pointer".to_string()),
                }

                Ok(Value::Void)
            }
            "MemCpy" => {
                if args.len() != 3 {
                    return Err("MemCpy(dst, src, size) expects 3 args".to_string());
                }
                let dst = self.eval_expr(&args[0])?;
                let src = self.eval_expr(&args[1])?;
                let size_i64 = self.eval_expr(&args[2])?.as_i64()?;
                let size: usize = size_i64
                    .try_into()
                    .map_err(|_| "MemCpy: size must be non-negative".to_string())?;

                let dst_obj = match &dst {
                    Value::Obj(obj) => Some(obj.clone()),
                    Value::VarRef(name) => match self.env.get(name) {
                        Some(Value::Obj(obj)) => Some(obj),
                        _ => None,
                    },
                    _ => None,
                };
                let src_obj = match &src {
                    Value::Obj(obj) => Some(obj.clone()),
                    Value::VarRef(name) => match self.env.get(name) {
                        Some(Value::Obj(obj)) => Some(obj),
                        _ => None,
                    },
                    _ => None,
                };
                if let (Some(dst_obj), Some(src_obj)) = (dst_obj, src_obj) {
                    let _ = size;
                    let snapshot = src_obj.borrow().fields.clone();
                    dst_obj.borrow_mut().fields = snapshot;
                    return Ok(Value::Void);
                }

                fn append_bytes_from_value(
                    vm: &Vm,
                    v: &Value,
                    size: usize,
                    out: &mut Vec<u8>,
                ) -> Result<(), String> {
                    match v {
                        Value::Array(arr) => {
                            let arr = arr.borrow();
                            let elem_bytes = arr.elem_bytes.max(1).min(8);
                            for elem in arr.elems.iter() {
                                match elem {
                                    Value::Array(_) => {
                                        append_bytes_from_value(vm, elem, size, out)?
                                    }
                                    Value::Int(bits) => out.extend_from_slice(
                                        &(*bits as u64).to_le_bytes()[..elem_bytes],
                                    ),
                                    Value::Char(bits) => {
                                        out.extend_from_slice(&bits.to_le_bytes()[..elem_bytes])
                                    }
                                    other => {
                                        return Err(format!(
                                            "MemCpy: unsupported array element value: {other:?}"
                                        ));
                                    }
                                }
                            }
                            Ok(())
                        }
                        Value::Ptr { addr, .. } => {
                            out.extend_from_slice(vm.heap_slice(*addr, size)?);
                            Ok(())
                        }
                        Value::Int(addr) => {
                            out.extend_from_slice(vm.heap_slice(*addr, size)?);
                            Ok(())
                        }
                        other => Err(format!("MemCpy: unsupported src value: {other:?}")),
                    }
                }

                let mut buf = Vec::with_capacity(size);
                append_bytes_from_value(self, &src, size, &mut buf)?;
                if buf.len() < size {
                    return Err("MemCpy: src does not contain enough bytes".to_string());
                }

                match dst {
                    Value::Ptr { addr, .. } => self.heap_write_bytes(addr, &buf[..size])?,
                    Value::Int(addr) => self.heap_write_bytes(addr, &buf[..size])?,
                    _ => return Err("MemCpy: dst must be a pointer".to_string()),
                }

                Ok(Value::Void)
            }
            "QSortI64" => {
                if args.len() != 3 {
                    return Err("QSortI64(base, cnt, cmp_fp) expects 3 args".to_string());
                }
                let base = self.eval_expr(&args[0])?;
                let cnt = self.eval_expr(&args[1])?.as_i64()?;
                let cmp = self.eval_expr(&args[2])?;

                let Value::Array(arr) = base else {
                    return Err("QSortI64: base must be an array".to_string());
                };
                let cnt: usize = cnt
                    .try_into()
                    .map_err(|_| "QSortI64: cnt must be non-negative".to_string())?;

                let cmp_name = match cmp {
                    Value::FuncRef(name) => name,
                    _ => return Err("QSortI64: cmp_fp must be a function pointer".to_string()),
                };

                let n = {
                    let a = arr.borrow();
                    cnt.min(a.elems.len())
                };
                let mut elems = {
                    let a = arr.borrow();
                    a.elems[..n].to_vec()
                };

                elems.sort_by(|a, b| {
                    let res = (|| {
                        let _scope = EnvScopeGuard::new(&mut self.env);
                        self.env.define("__tl_qs_a".to_string(), a.clone());
                        self.env.define("__tl_qs_b".to_string(), b.clone());
                        let args = [
                            Expr::Var("__tl_qs_a".to_string()),
                            Expr::Var("__tl_qs_b".to_string()),
                        ];
                        self.call(&cmp_name, &args).and_then(|v| v.as_i64())
                    })()
                    .unwrap_or(0);

                    if res < 0 {
                        std::cmp::Ordering::Less
                    } else if res > 0 {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });

                let mut a = arr.borrow_mut();
                for i in 0..n {
                    a.elems[i] = elems[i].clone();
                }
                Ok(Value::Void)
            }
            "TaskDerivedValsUpdate" => {
                if args.len() > 1 {
                    return Err("TaskDerivedValsUpdate(task=Fs) expects 0-1 args".to_string());
                }

                let task = match args.get(0) {
                    None | Some(Expr::DefaultArg) => self.env.get("Fs"),
                    Some(expr) => Some(self.eval_expr(expr)?),
                };

                // Best-effort: update the `task` size fields based on the current framebuffer.
                if let Some(Value::Obj(task)) = task.clone() {
                    let (w, h) = self.rt.size();
                    let (horz_scroll, vert_scroll) = {
                        let mut t = task.borrow_mut();
                        t.fields
                            .insert("pix_width".to_string(), Value::Int(w as i64));
                        t.fields
                            .insert("pix_height".to_string(), Value::Int(h as i64));
                        t.fields
                            .insert("win_width".to_string(), Value::Int((w / 8) as i64));
                        t.fields
                            .insert("win_height".to_string(), Value::Int((h / 8) as i64));

                        (
                            t.fields.get("horz_scroll").cloned(),
                            t.fields.get("vert_scroll").cloned(),
                        )
                    };

                    for scroll in [horz_scroll, vert_scroll] {
                        let Some(Value::Obj(scroll)) = scroll else {
                            continue;
                        };
                        let (min_v, pos_v, max_v) = {
                            let s = scroll.borrow();
                            let min_v = s
                                .fields
                                .get("min")
                                .and_then(|v| v.as_i64().ok())
                                .unwrap_or(0);
                            let pos_v = s
                                .fields
                                .get("pos")
                                .and_then(|v| v.as_i64().ok())
                                .unwrap_or(0);
                            let max_v = s
                                .fields
                                .get("max")
                                .and_then(|v| v.as_i64().ok())
                                .unwrap_or(0);
                            (min_v, pos_v, max_v)
                        };

                        let clamped = pos_v.clamp(min_v.min(max_v), min_v.max(max_v));
                        scroll
                            .borrow_mut()
                            .fields
                            .insert("pos".to_string(), Value::Int(clamped));
                    }
                }

                // Update derived values for any controls registered on `Fs->last_ctrl`.
                let Some(Value::Obj(fs)) = self.env.get("Fs") else {
                    return Ok(Value::Void);
                };
                let head = match fs.borrow().fields.get("last_ctrl").cloned() {
                    Some(Value::Obj(head)) => head,
                    _ => return Ok(Value::Void),
                };
                let mut cur = match head.borrow().fields.get("next").cloned() {
                    Some(Value::Obj(cur)) => cur,
                    _ => return Ok(Value::Void),
                };

                let mut steps = 0usize;
                while !Rc::ptr_eq(&cur, &head) {
                    steps += 1;
                    if steps > 4096 {
                        break;
                    }

                    let (next, update) = {
                        let cur = cur.borrow();
                        let next = cur.fields.get("next").cloned();
                        let update = cur.fields.get("update_derived_vals").cloned();
                        (next, update)
                    };

                    if let Some(Value::FuncRef(name)) = update {
                        self.in_draw_it = true;
                        let res = (|| {
                            let _scope = EnvScopeGuard::new(&mut self.env);
                            self.env
                                .define("__tl_ctrl".to_string(), Value::Obj(cur.clone()));
                            let args = [Expr::Var("__tl_ctrl".to_string())];
                            let _ = self.call(&name, &args)?;
                            Ok::<(), String>(())
                        })();
                        self.in_draw_it = false;
                        res?;
                    }

                    cur = match next {
                        Some(Value::Obj(next)) => next,
                        _ => break,
                    };
                }

                Ok(Value::Void)
            }
            _ => Err(format!("internal: call_builtin_core cannot handle {name}")),
        }
    }
}
