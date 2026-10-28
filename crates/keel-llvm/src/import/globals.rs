//! Exact global layout, relocation and RTTI transport.
use super::*;
#[derive(Default)]
pub(super) struct Globals {
    pub objects: Vec<Ref>,
    symbols: HashSet<Ref>,
    sizes: HashMap<Ref, u64>,
    tags: HashMap<Ref, (Ref, Ref)>,
}
impl Translation<'_> {
    pub(super) fn global_layout(&self, t: Ref) -> Result<u64> {
        let l = self.l;
        match l.get_type_kind(t) {
            2 | 3 => {
                self.ty(t)?;
            }
            8 if matches!(l.get_int_type_width(t), 1 | 8 | 16 | 32 | 64) => {}
            12 if l.get_pointer_address_space(t) == 0 => {}
            11 => {
                let size = self.global_layout(l.get_element_type(t))?;
                if u128::from(size) * u128::from(l.get_array_length2(t)) > u128::from(u32::MAX) {
                    return Err("global array layout exceeds u32".into());
                }
            }
            10 if l.is_opaque_struct(t) == 0 => {
                let mut bound = 0u64;
                for n in 0..l.count_struct_element_types(t) {
                    let child = l.struct_get_type_at_index(t, n);
                    bound += self.global_layout(child)?
                        + u64::from(l.abi_alignment_of_type(self.td, child))
                        - 1;
                    if bound > u64::from(u32::MAX) {
                        return Err("global struct layout exceeds u32".into());
                    }
                }
            }
            _ => return Err(
                "unsupported global layout (unsized/vector/address-space/unsupported scalar type)"
                    .into(),
            ),
        }
        let size = l.abi_size_of_type(self.td, t);
        if size > u64::from(u32::MAX) {
            return Err("global layout exceeds u32".into());
        }
        Ok(size)
    }
    pub(super) fn address(&self, v: Ref) -> Result<Option<(Ref, u64)>> {
        let l = self.l;
        if self.globals.symbols.contains(&v) {
            return Ok(Some((v, 0)));
        }
        if l.is_a_constant_expr(v) == 0 {
            return Ok(None);
        }
        if l.get_const_opcode(v) != 29 {
            return Err("only constant GEP symbolic expressions are supported".into());
        }
        let (symbol, base) = self
            .address(l.get_operand(v, 0))?
            .ok_or("constant GEP requires a named symbol base")?;
        let mut offset = i128::from(base);
        let mut t = l.get_gep_source_element_type(v);
        self.global_layout(t)?;
        for n in 1..l.get_num_operands(v) as u32 {
            let index = l.get_operand(v, n);
            if l.is_a_constant_int(index) == 0 || !matches!(self.width(index), 8 | 16 | 32 | 64) {
                return Err("constant GEP requires i8/i16/i32/i64 integer indices".into());
            }
            let number = l.const_int_get_s_ext_value(index);
            if n > 1 && l.get_type_kind(t) == 10 {
                if number < 0 || number >= i64::from(l.count_struct_element_types(t)) {
                    return Err("constant GEP struct index out of bounds".into());
                }
                offset += i128::from(l.offset_of_element(self.td, t, number as u32));
                t = l.struct_get_type_at_index(t, number as u32);
            } else {
                if n > 1 {
                    if l.get_type_kind(t) != 11 {
                        return Err("constant GEP indexes unsupported layout".into());
                    }
                    t = l.get_element_type(t);
                }
                offset += i128::from(number) * i128::from(l.abi_size_of_type(self.td, t));
            }
        }
        if !(0..=i128::from(u32::MAX)).contains(&offset) {
            return Err("symbol addend must fit unsigned u32".into());
        }
        let offset = offset as u64;
        if self.globals.sizes.get(&symbol).is_some_and(|&s| offset > s) {
            return Err("symbol addend past defined object end".into());
        }
        if l.is_a_function(symbol) != 0 && offset != 0 {
            return Err("function symbol addends are unsupported".into());
        }
        Ok(Some((symbol, offset)))
    }
    fn rtti_tag(&self, v: Ref) -> Option<Ref> {
        let l = self.l;
        if l.is_a_constant_expr(v) == 0 || l.get_const_opcode(v) != 40 {
            return None;
        }
        let add = l.get_operand(v, 0);
        if l.is_a_constant_expr(add) == 0
            || l.get_const_opcode(add) != 8
            || self.width(add) != 64
            || word(&l.printed(add), "nuw")
            || word(&l.printed(add), "nsw")
        {
            return None;
        }
        let p = l.get_operand(add, 0);
        let tag = l.get_operand(add, 1);
        if l.is_a_constant_int(tag) == 0
            || self.width(tag) != 64
            || l.const_int_get_z_ext_value(tag) != 1 << 63
            || l.is_a_constant_expr(p) == 0
            || l.get_const_opcode(p) != 39
            || self.width(p) != 64
        {
            return None;
        }
        let symbol = l.get_operand(p, 0);
        self.globals.objects.contains(&symbol).then_some(symbol)
    }
    fn rtti_objects(&mut self) -> Result<HashSet<Ref>> {
        let l = self.l;
        let mut eligible = HashSet::new();
        let objects = self.globals.objects.clone();
        let names: HashMap<_, _> = objects.iter().map(|&g| (l.name(g), g)).collect();
        for g in objects {
            let name = l.name(g);
            let t = l.global_get_value_type(g);
            if !name.starts_with("_ZTI")
                || name.len() <= 4
                || !odr(l.get_linkage(g))
                || l.is_global_constant(g) == 0
                || l.get_initializer(g) == 0
                || l.get_type_kind(t) != 10
                || l.is_packed_struct(t) != 0
            {
                continue;
            }
            let count = l.count_struct_element_types(t);
            if !matches!(count, 2 | 3)
                || self.globals.sizes[&g] != u64::from(count) * 8
                || (0..count).any(|n| {
                    l.get_type_kind(l.struct_get_type_at_index(t, n)) != 12
                        || l.offset_of_element(self.td, t, n) != u64::from(n) * 8
                })
            {
                continue;
            }
            let init = l.get_initializer(g);
            let Some((vtable, offset)) = self.address(l.get_aggregate_element(init, 0))? else {
                continue;
            };
            let runtime = if count == 2 {
                "_ZTVN10__cxxabiv117__class_type_infoE"
            } else {
                "_ZTVN10__cxxabiv120__si_class_type_infoE"
            };
            if l.name(vtable) != runtime
                || offset != 16
                || !self.globals.objects.contains(&vtable)
                || l.get_initializer(vtable) != 0
                || l.get_linkage(vtable) != 0
                || l.get_visibility(vtable) != 0
            {
                continue;
            }
            let Some(&name_object) = names.get(&format!("_ZTS{}", &name[4..])) else {
                continue;
            };
            if !odr(l.get_linkage(name_object))
                || l.is_global_constant(name_object) == 0
                || l.get_initializer(name_object) == 0
            {
                continue;
            }
            let nt = l.global_get_value_type(name_object);
            if l.get_type_kind(nt) != 11
                || l.get_type_kind(l.get_element_type(nt)) != 8
                || l.get_int_type_width(l.get_element_type(nt)) != 8
                || l.get_array_length2(nt) < 2
            {
                continue;
            }
            let last = l.get_aggregate_element(
                l.get_initializer(name_object),
                (l.get_array_length2(nt) - 1) as u32,
            );
            if l.is_a_constant_int(last) == 0 || l.const_int_get_z_ext_value(last) != 0 {
                continue;
            }
            let name_value = l.get_aggregate_element(init, 1);
            let tag = self.rtti_tag(name_value);
            if name_value != name_object && tag != Some(name_object) {
                continue;
            }
            if count == 3 {
                let base = l.get_aggregate_element(init, 2);
                if !self.globals.objects.contains(&base) || !l.name(base).starts_with("_ZTI") {
                    continue;
                }
            }
            eligible.extend([g, name_object]);
            if tag.is_some() {
                self.globals.tags.insert(g, (name_value, name_object));
            }
        }
        Ok(eligible)
    }
    pub(super) fn setup_globals(&mut self, functions: &[Ref]) -> Result<Vec<String>> {
        let l = self.l;
        self.globals.objects =
            sequence(|| l.get_first_global(self.module), |g| l.get_next_global(g));
        self.globals.symbols = functions
            .iter()
            .chain(&self.globals.objects)
            .copied()
            .collect();
        for &g in &self.globals.objects {
            self.ty(l.type_of(g))?;
            if l.name(g).starts_with("llvm.") {
                return Err("reserved LLVM global objects".into());
            }
            let link = l.get_linkage(g);
            let init = l.get_initializer(g);
            if (link != 0 && !local(link) && !(init != 0 && odr(link)))
                || !matches!(l.get_visibility(g), 0 | 1)
                || l.get_dll_storage_class(g) != 0
                || l.is_thread_local(g) != 0
                || l.is_externally_initialized(g) != 0
                || !l.text_property(g, "section").is_empty()
                || l.get_comdat(g) != 0
            {
                return Err(format!(
                    "{}: unsupported global linkage/storage/section/COMDAT",
                    self.symbol(g)?
                ));
            }
            check_suffix(&l.printed(g))?;
            let t = l.global_get_value_type(g);
            let size = self.global_layout(t)?;
            if size == 0 && init != 0 {
                return Err(
                    "global definition size must be nonzero and global size must fit u32".into(),
                );
            }
            if init != 0 {
                self.globals.sizes.insert(g, size);
            }
        }
        let hidden = self.rtti_objects()?;
        let mut output = Vec::new();
        for &g in &self.globals.objects {
            if l.get_visibility(g) != 0 && !hidden.contains(&g) {
                return Err(format!(
                    "{}: unsupported hidden global (requires validated RTTI)",
                    self.symbol(g)?
                ));
            }
            let name = self.symbol(g)?;
            let value = l.get_initializer(g);
            if value == 0 {
                output.push(format!("decl data {name}"));
                continue;
            }
            let t = l.global_get_value_type(g);
            let align = l.get_alignment(g).max(l.abi_alignment_of_type(self.td, t));
            if align == 0 || align > 4096 || !align.is_power_of_two() {
                return Err("global alignment must be a power of two up to 4096".into());
            }
            let items = self.initializer(value, t, 0, align, g)?;
            if items.iter().map(|i| i.0).sum::<u64>() != self.globals.sizes[&g] {
                return Err("initializer size differs from LLVM allocation size".into());
            }
            let mut flags = if local(l.get_linkage(g)) {
                "local "
            } else if odr(l.get_linkage(g)) {
                "weak_odr "
            } else {
                ""
            }
            .to_string();
            if l.get_visibility(g) != 0 {
                flags.push_str("hidden ");
            }
            if l.is_global_constant(g) != 0 {
                flags.push_str("const ");
            }
            output.push(format!(
                "data {name} = {flags}align {align} {{ {} }}",
                items
                    .into_iter()
                    .map(|i| i.1)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Ok(output)
    }
    fn initializer(
        &self,
        v: Ref,
        t: Ref,
        offset: u64,
        align: u32,
        owner: Ref,
    ) -> Result<Vec<(u64, String)>> {
        let l = self.l;
        let size = l.abi_size_of_type(self.td, t);
        if l.is_a_constant_aggregate_zero(v) != 0 || l.is_a_constant_pointer_null(v) != 0 {
            return Ok(if size == 0 {
                vec![]
            } else {
                vec![(size, format!("zero {size}"))]
            });
        }
        let kind = l.get_type_kind(t);
        if matches!(kind, 2 | 3) && l.is_a_constant_fp(v) != 0 {
            let width = if kind == 2 { 32 } else { 64 };
            let bits = self.fp_bits(v, width);
            return Ok(vec![(
                u64::from(width / 8),
                format!("i{width} 0x{bits:0digits$x}", digits = width as usize / 4),
            )]);
        }
        if kind == 8 && l.is_a_constant_int(v) != 0 {
            let width = l.get_int_type_width(t);
            let n = if width == 1 {
                l.const_int_get_z_ext_value(v).to_string()
            } else {
                l.const_int_get_s_ext_value(v).to_string()
            };
            let used = u64::from((width / 8).max(1));
            let mut items = vec![(used, format!("i{} {n}", width.max(8)))];
            if size > used {
                items.push((size - used, format!("zero {}", size - used)));
            }
            return Ok(items);
        }
        if kind == 12 {
            if let Some(&(value, symbol)) = self.globals.tags.get(&owner)
                && value == v
                && offset == 8
            {
                if align < 8 {
                    return Err("RTTI name relocation requires object alignment >= 8".into());
                }
                return Ok(vec![(8, format!("rtti_name {}", self.symbol(symbol)?))]);
            }
            if let Some((symbol, addend)) = self.address(v)? {
                if !offset.is_multiple_of(8) || align < 8 {
                    return Err(
                        "pointer relocation requires aligned offset and object alignment >= 8"
                            .into(),
                    );
                }
                return Ok(vec![(
                    8,
                    format!(
                        "ptr {}{}",
                        self.symbol(symbol)?,
                        if addend == 0 {
                            String::new()
                        } else {
                            format!(" + {addend}")
                        }
                    ),
                )]);
            }
        }
        if matches!(kind, 10 | 11) {
            let count = if kind == 10 {
                u64::from(l.count_struct_element_types(t))
            } else {
                l.get_array_length2(t)
            };
            let mut items = Vec::new();
            let mut cursor = 0;
            for n in 0..count {
                let ct = if kind == 10 {
                    l.struct_get_type_at_index(t, n as u32)
                } else {
                    l.get_element_type(t)
                };
                let position = if kind == 10 {
                    l.offset_of_element(self.td, t, n as u32)
                } else {
                    n * l.abi_size_of_type(self.td, ct)
                };
                if position > cursor {
                    items.push((position - cursor, format!("zero {}", position - cursor)));
                }
                let child = l.get_aggregate_element(v, n as u32);
                if child == 0 {
                    return Err("unsupported aggregate initializer".into());
                }
                let parts = self.initializer(child, ct, offset + position, align, owner)?;
                cursor = position + parts.iter().map(|p| p.0).sum::<u64>();
                items.extend(parts);
            }
            if size > cursor {
                items.push((size - cursor, format!("zero {}", size - cursor)));
            }
            return Ok(items);
        }
        Err("unsupported global initializer (undef/poison/integer expression)".into())
    }
}
fn check_suffix(text: &str) -> Result<()> {
    let (mut depth, mut quoted, mut escaped) = (0i32, false, false);
    for (pos, c) in text.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
        } else if c == '"' {
            quoted = true;
        } else if "([{<".contains(c) {
            depth += 1;
        } else if ")]} >".replace(' ', "").contains(c) {
            depth -= 1;
        } else if c == ',' && depth == 0 {
            let suffix = text[pos + 1..].trim();
            let Some(number) = suffix.strip_prefix("align ") else {
                return Err("unsupported global suffix/metadata".into());
            };
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
                return Err("unsupported global suffix/metadata".into());
            }
            break;
        }
    }
    static GROUP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"#[0-9]+\s*$").expect("constant group regex"));
    if GROUP.is_match(text) {
        return Err("global attribute groups".into());
    }
    Ok(())
}
