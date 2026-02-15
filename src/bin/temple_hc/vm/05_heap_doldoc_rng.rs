use super::prelude::*;
use super::{Obj, Value, Vm};

impl Vm {
    pub(super) fn heap_alloc(&mut self, size: usize, zeroed: bool) -> i64 {
        if size == 0 {
            return 0;
        }
        let addr = self.heap.len() as i64;
        let fill = if zeroed { 0u8 } else { 0u8 };
        self.heap.extend(std::iter::repeat(fill).take(size));
        addr
    }

    fn heap_check_range(&self, addr: i64, len: usize) -> Result<usize, String> {
        if len == 0 {
            return Ok(addr.max(0) as usize);
        }
        if addr <= 0 {
            return Err("null pointer".to_string());
        }
        let start: usize = addr
            .try_into()
            .map_err(|_| "negative pointer".to_string())?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| "pointer overflow".to_string())?;
        if end > self.heap.len() {
            return Err("pointer out of range".to_string());
        }
        Ok(start)
    }

    pub(super) fn heap_slice(&self, addr: i64, len: usize) -> Result<&[u8], String> {
        let start = self.heap_check_range(addr, len)?;
        Ok(&self.heap[start..start + len])
    }

    pub(super) fn heap_tail(&self, addr: i64) -> Result<&[u8], String> {
        if addr <= 0 {
            return Err("null pointer".to_string());
        }
        let start: usize = addr
            .try_into()
            .map_err(|_| "negative pointer".to_string())?;
        if start >= self.heap.len() {
            return Err("pointer out of range".to_string());
        }
        Ok(&self.heap[start..])
    }

    pub(super) fn heap_write_bytes(&mut self, addr: i64, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() {
            return Ok(());
        }
        let start = self.heap_check_range(addr, bytes.len())?;
        self.heap[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    pub(super) fn heap_read_u8(&self, addr: i64) -> Result<u8, String> {
        Ok(self.heap_slice(addr, 1)?[0])
    }

    pub(super) fn heap_write_u8(&mut self, addr: i64, value: u8) -> Result<(), String> {
        let start = self.heap_check_range(addr, 1)?;
        self.heap[start] = value;
        Ok(())
    }

    pub(super) fn heap_read_i64_le(&self, addr: i64, bytes: usize) -> Result<i64, String> {
        if bytes == 0 || bytes > 8 {
            return Err("heap read: unsupported integer width".to_string());
        }
        let slice = self.heap_slice(addr, bytes)?;
        let mut buf = [0u8; 8];
        buf[..bytes].copy_from_slice(slice);
        let v = u64::from_le_bytes(buf);
        Ok(v as i64)
    }

    pub(super) fn heap_write_i64_le(
        &mut self,
        addr: i64,
        bytes: usize,
        value: i64,
    ) -> Result<(), String> {
        if bytes == 0 || bytes > 8 {
            return Err("heap write: unsupported integer width".to_string());
        }
        let start = self.heap_check_range(addr, bytes)?;
        let v = (value as u64).to_le_bytes();
        self.heap[start..start + bytes].copy_from_slice(&v[..bytes]);
        Ok(())
    }

    pub(super) fn alloc_class_value(&mut self, name: &str) -> Result<Value, String> {
        let Some(fields_def) = self.program.classes.get(name).map(|def| def.fields.clone()) else {
            return Err(format!("unknown class: {name}"));
        };
        let mut fields: HashMap<String, Value> = HashMap::new();
        for field in fields_def {
            let v = if !field.array_lens.is_empty() {
                self.eval_array_value(
                    &field.ty,
                    field.pointer,
                    &field.array_lens,
                    field.init.as_ref(),
                    &format!("{name}.{}", field.name),
                )?
            } else {
                self.default_value_for_type(&field.ty, field.pointer)?
            };
            fields.insert(field.name.clone(), v);
        }
        Ok(Value::Obj(Rc::new(RefCell::new(Obj { fields }))))
    }

    pub(super) fn read_cstr_lossy(&self, addr: i64) -> Result<String, String> {
        if addr == 0 {
            return Ok(String::new());
        }
        let mut bytes = Vec::new();
        for i in 0..(1 << 20) {
            let b = self.heap_read_u8(addr + i as i64)?;
            if b == 0 {
                break;
            }
            bytes.push(b);
        }
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    pub(super) fn load_doldoc_bin(
        &mut self,
        file: &Arc<str>,
        bin_num: u32,
    ) -> Result<(i64, usize), String> {
        let key = (file.clone(), bin_num);
        if let Some(&addr) = self.doldoc_bin_ptr_cache.get(&key) {
            let len = self.doldoc_bin_len_by_ptr.get(&addr).copied().unwrap_or(0);
            return Ok((addr, len));
        }

        let bins = self.program.bins_by_file.get(file).ok_or_else(|| {
            format!(
                "DolDoc bin: file not found in preprocessor output: {}",
                file.as_ref()
            )
        })?;
        let bytes = match bins.get(&bin_num) {
            Some(bytes) => bytes.clone(),
            None => {
                // Some vendored TempleOS sources appear to have incomplete or corrupted DolDoc bin
                // tails (possibly from truncated exports). Prefer "run the app" over failing hard:
                // fall back to the closest available bin payload within the same file, and if none
                // exist, return an empty blob.
                let fallback = bins
                    .range(..=bin_num)
                    .rev()
                    .find(|&(&n, _)| n != 0)
                    .map(|(&n, _)| n)
                    .or_else(|| bins.iter().find(|&(&n, _)| n != 0).map(|(&n, _)| n))
                    .or_else(|| bins.contains_key(&0).then_some(0));

                if let Some(fallback_num) = fallback {
                    let (addr, len) = self.load_doldoc_bin(file, fallback_num)?;
                    self.doldoc_bin_ptr_cache.insert(key, addr);
                    return Ok((addr, len));
                }

                Vec::new()
            }
        };

        if bytes.is_empty() {
            let addr = self.heap_alloc(1, true);
            let _ = self.heap_write_u8(addr, 0);
            self.doldoc_bin_ptr_cache.insert(key, addr);
            self.doldoc_bin_len_by_ptr.insert(addr, 0);
            return Ok((addr, 0));
        }
        let len = bytes.len();
        let addr = self.heap_alloc(len + 1, true);
        self.heap_write_bytes(addr, &bytes)?;
        let _ = self.heap_write_u8(addr + len as i64, 0);
        self.doldoc_bin_ptr_cache.insert(key, addr);
        self.doldoc_bin_len_by_ptr.insert(addr, len);
        Ok((addr, len))
    }

    pub(super) fn set_seed(&mut self, seed: u64) {
        self.rng_seed = seed;
        if seed == 0 {
            self.rng_state = Self::now_nanos() ^ 0x9E37_79B9_7F4A_7C15;
        } else {
            self.rng_state = seed;
        }
    }

    pub(super) fn rand_next_u64(&mut self) -> u64 {
        // SplitMix64-ish. When Seed(0), mix in a timer to make it non-deterministic.
        if self.rng_seed == 0 {
            self.rng_state ^= Self::now_nanos();
        }

        self.rng_state = self.rng_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub(super) fn rand_i16(&mut self) -> i16 {
        (self.rand_next_u64() >> 48) as u16 as i16
    }
}
