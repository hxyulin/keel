//! Scalar, memory, call and exception instruction mapping.
use super::*;
impl Translation<'_> {
    fn args(&mut self, inst: Ref) -> Result<Vec<String>> {
        (0..self.l.get_num_operands(inst) as u32)
            .map(|n| self.value(self.l.get_operand(inst, n)))
            .collect()
    }
    pub(super) fn instruction(&mut self, inst: Ref) -> Result<Option<String>> {
        let l = self.l;
        let op = l.get_instruction_opcode(inst);
        if l.can_value_use_fast_math_flags(inst) != 0 && l.get_fast_math_flags(inst) != 0 {
            return Err("floating-point fast-math flags".into());
        }
        let expr = match op {
            9 | 11 | 13 | 16 => {
                let a = self.args(inst)?;
                let name = match op {
                    9 => "fadd",
                    11 => "fsub",
                    13 => "fmul",
                    _ => "fdiv",
                };
                format!("{name} {}, {}", a[0], a[1])
            }
            66 => {
                let a = self.value(l.get_operand(inst, 0))?;
                format!("fneg {a}")
            }
            43 => {
                let a = self.args(inst)?;
                let p = l.get_f_cmp_predicate(inst);
                if matches!(p, 0 | 15) {
                    format!("iconst.i8 {}", u8::from(p == 15))
                } else {
                    let cc = match p {
                        1 | 9 => "eq",
                        2 | 10 => "gt",
                        3 | 11 => "ge",
                        4 | 12 => "lt",
                        5 | 13 => "le",
                        7 => "ord",
                        8 => "uno",
                        6 | 14 => "ne",
                        _ => return Err("invalid FP predicate".into()),
                    };
                    let comparison = format!("fcmp.{cc} {}, {}", a[0], a[1]);
                    if p == 6 {
                        let ne = self.emit(comparison);
                        let ord = self.emit(format!("fcmp.ord {}, {}", a[0], a[1]));
                        format!("and {ne}, {ord}")
                    } else if (9..=13).contains(&p) {
                        let cmp = self.emit(comparison);
                        let uno = self.emit(format!("fcmp.uno {}, {}", a[0], a[1]));
                        format!("or {cmp}, {uno}")
                    } else {
                        comparison
                    }
                }
            }
            33..=38 => {
                let arg = l.get_operand(inst, 0);
                let ty = self.ty(l.type_of(inst))?;
                let printed = l.printed(inst);
                if word(&printed, "nneg")
                    || printed.split_whitespace().any(|w| w.starts_with("round."))
                {
                    return Err("unsupported floating-point conversion flags/rounding".into());
                }
                if matches!(op, 33 | 34) && !matches!(ty.as_str(), "i32" | "i64") {
                    return Err("FP-to-integer destinations must be i32/i64".into());
                }
                let mut value = self.value(arg)?;
                if op == 36 && self.width(arg) == 1 {
                    value = self.emit(format!("ineg {value}"));
                }
                let name = match op {
                    33 => "fptoui",
                    34 => "fptosi",
                    35 => "uitofp",
                    36 => "sitofp",
                    37 => "fptrunc",
                    _ => "fpext",
                };
                format!("{name}.{ty} {value}")
            }
            70 | 71 => {
                let text = if l.get_num_successors(inst) == 1 {
                    format!("jump {}", self.edge(l.get_successor(inst, 0))?)
                } else {
                    let cond = self.value(l.get_condition(inst))?;
                    format!(
                        "brif {cond}, {}, {}",
                        self.edge(l.get_successor(inst, 0))?,
                        self.edge(l.get_successor(inst, 1))?
                    )
                };
                self.lines.push(format!("    {text}"));
                return Ok(None);
            }
            3 => {
                let cond = self.value(l.get_operand(inst, 0))?;
                let default = self.edge(l.get_successor(inst, 0))?;
                let mut cases = Vec::new();
                for n in 1..l.get_num_successors(inst) {
                    let value = l.get_switch_case_value(inst, n);
                    let num = if self.width(value) == 1 {
                        l.const_int_get_z_ext_value(value).to_string()
                    } else {
                        l.const_int_get_s_ext_value(value).to_string()
                    };
                    cases.push(format!("{num}: {}", self.edge(l.get_successor(inst, n))?));
                }
                self.lines.push(format!(
                    "    switch {cond}, {default}, [{}]",
                    cases.join(", ")
                ));
                return Ok(None);
            }
            26 => format!("stack_addr {}", self.slots[&inst]),
            29 => {
                let mut ty = l.get_gep_source_element_type(inst);
                self.layout_type(ty)?;
                let mut addr = self.value(l.get_operand(inst, 0))?;
                for n in 1..l.get_num_operands(inst) as u32 {
                    let index = l.get_operand(inst, n);
                    let offset;
                    if n > 1 && l.get_type_kind(ty) == 10 {
                        if l.is_a_constant_int(index) == 0 {
                            return Err("nonconstant struct GEP index".into());
                        }
                        let field = l.const_int_get_z_ext_value(index) as u32;
                        offset = self.emit(format!(
                            "iconst.i64 {:#x}",
                            l.offset_of_element(self.td, ty, field)
                        ));
                        ty = l.struct_get_type_at_index(ty, field);
                    } else {
                        if n > 1 {
                            if l.get_type_kind(ty) != 11 {
                                return Err("GEP indexes unsupported layout".into());
                            }
                            ty = l.get_element_type(ty);
                        }
                        let width = self.width(index);
                        if !matches!(width, 8 | 16 | 32 | 64) {
                            return Err("GEP index must be i8/i16/i32/i64".into());
                        }
                        let mut off = self.value(index)?;
                        if width < 64 {
                            off = self.emit(format!("sext.i64 {off}"));
                        }
                        let stride =
                            self.emit(format!("iconst.i64 {:#x}", l.abi_size_of_type(self.td, ty)));
                        offset = self.emit(format!("imul {off}, {stride}"));
                    }
                    addr = self.emit(format!("ptradd {addr}, {offset}"));
                }
                return Ok(Some(addr));
            }
            8 | 10 | 12 | 14 | 15 | 17 | 18 | 20..=25 => {
                if self.width(inst) == 1 && !matches!(op, 23..=25) {
                    return Err("i1 arithmetic other than boolean bitwise operations".into());
                }
                let a = self.args(inst)?;
                let mut b = a[1].clone();
                if op == 15 {
                    // Totalize MIN/-1 so refining a poison dividend cannot add a trap on a masked path.
                    let ty = self.ty(l.type_of(inst))?;
                    let minimum = self.emit(format!(
                        "iconst.{ty} {}",
                        -(1i128 << (self.width(inst) - 1))
                    ));
                    let minus = self.emit(format!("iconst.{ty} -1"));
                    let is_min = self.emit(format!("icmp.eq {}, {minimum}", a[0]));
                    let is_minus = self.emit(format!("icmp.eq {b}, {minus}"));
                    let overflow = self.emit(format!("and {is_min}, {is_minus}"));
                    let one = self.emit(format!("iconst.{ty} 1"));
                    b = self.emit(format!("select {overflow}, {one}, {b}"));
                }
                let name = match op {
                    8 => "iadd",
                    10 => "isub",
                    12 => "imul",
                    14 => "udiv",
                    15 => "sdiv",
                    17 => "urem",
                    18 => "srem",
                    20 => "shl",
                    21 => "ushr",
                    22 => "sshr",
                    23 => "and",
                    24 => "or",
                    _ => "xor",
                };
                format!("{name} {}, {b}", a[0])
            }
            30..=32 | 39..=41 => {
                let arg = l.get_operand(inst, 0);
                let mut value = self.value(arg)?;
                let ty = self.ty(l.type_of(inst))?;
                if op == 41 {
                    let source = self.ty(l.type_of(arg))?;
                    if source == ty {
                        return Ok(Some(value));
                    }
                    if !matches!(
                        (source.as_str(), ty.as_str()),
                        ("f32", "i32") | ("i32", "f32") | ("f64", "i64") | ("i64", "f64")
                    ) {
                        return Err("unsupported scalar bitcast".into());
                    }
                    format!("bitcast.{ty} {value}")
                } else if op == 30 && self.width(inst) == 1 {
                    if self.width(arg) > 8 {
                        value = self.emit(format!("trunc.i8 {value}"));
                    }
                    let one = self.emit("iconst.i8 1");
                    format!("and {value}, {one}")
                } else if op == 32 && self.width(arg) == 1 {
                    value = self.emit(format!("ineg {value}"));
                    if ty == "i8" {
                        return Ok(Some(value));
                    }
                    format!("sext.{ty} {value}")
                } else if op == 31 && self.width(arg) == 1 && ty == "i8" {
                    return Ok(Some(value));
                } else if op == 39 && ty != "i64" {
                    let wide = self.emit(format!("ptrtoint.i64 {value}"));
                    let narrow = self.emit(format!("trunc.{ty} {wide}"));
                    if self.width(inst) == 1 {
                        let one = self.emit("iconst.i8 1");
                        format!("and {narrow}, {one}")
                    } else {
                        return Ok(Some(narrow));
                    }
                } else {
                    if op == 40 && self.width(arg) < 64 {
                        value = self.emit(format!("uext.i64 {value}"));
                    }
                    let name = match op {
                        30 => "trunc",
                        31 => "uext",
                        32 => "sext",
                        39 => "ptrtoint",
                        _ => "inttoptr",
                    };
                    format!("{name}.{ty} {value}")
                }
            }
            42 => {
                let x = l.get_operand(inst, 0);
                if self.width(x) == 0 && l.printed(inst).contains("icmp samesign") {
                    return Err("samesign comparison requires scalar integers".into());
                }
                let cc = match l.get_i_cmp_predicate(inst) {
                    32 => "eq",
                    33 => "ne",
                    34 => "ugt",
                    35 => "uge",
                    36 => "ult",
                    37 => "ule",
                    38 => "sgt",
                    39 => "sge",
                    40 => "slt",
                    41 => "sle",
                    _ => return Err("invalid integer predicate".into()),
                };
                if self.width(x) == 1 && !matches!(cc, "eq" | "ne") {
                    return Err("ordered i1 comparison".into());
                }
                let a = self.args(inst)?;
                format!("icmp.{cc} {}, {}", a[0], a[1])
            }
            46 => {
                let a = self.args(inst)?;
                format!("select {}, {}, {}", a[0], a[1], a[2])
            }
            68 => {
                // All supported producers have concrete Keel results. Keeping
                // that result chooses one legal freeze outcome, preserves
                // non-poison inputs, and gives every use the same SSA value.
                return Ok(Some(self.value(l.get_operand(inst, 0))?));
            }
            27 | 28 => {
                if l.get_ordering(inst) != 0 {
                    return Err("atomic memory access".into());
                }
                let flags = keel_ir::MemoryFlags {
                    volatile: l.get_volatile(inst) != 0,
                    align: l.get_alignment(inst).max(1),
                    notrap: false,
                };
                if op == 27 {
                    let ty = self.ty(l.type_of(inst))?;
                    let addr = self.value(l.get_operand(inst, 0))?;
                    let facts = self.load_facts(inst);
                    let v = self.emit(format!("load.{ty} {flags}{addr}{facts}"));
                    if self.width(inst) == 1 {
                        let one = self.emit("iconst.i8 1");
                        format!("and {v}, {one}")
                    } else {
                        return Ok(Some(v));
                    }
                } else {
                    let a = self.args(inst)?;
                    self.lines
                        .push(format!("    store {flags}{}, {}", a[0], a[1]));
                    return Ok(None);
                }
            }
            59 => {
                if !self.split_pads.contains(&self.block) {
                    let p = &self.pairs[&inst];
                    self.lines.push(format!(
                        "    {}, {} = landingpad {}",
                        p[0],
                        p[1],
                        self.clauses(inst)?
                    ));
                }
                return Ok(None);
            }
            54 => {
                if !self.integer_pair(l.type_of(inst)) {
                    return Err("insertvalue outside exact two-i64 contract".into());
                }
                return Ok(None);
            }
            53 => {
                let source = l.get_operand(inst, 0);
                let idx = l.indices(inst);
                if self.integer_pair(l.type_of(source)) {
                    if idx.len() != 1 {
                        return Err("extractvalue outside exact two-i64 contract".into());
                    }
                    return Ok(Some(self.aggregate_component(source, idx[0])?));
                }
                let pair = self
                    .pairs
                    .get(&source)
                    .ok_or("extractvalue requires a landingpad pair or acyclic pair PHI")?;
                if idx.len() != 1 || idx[0] > 1 {
                    return Err("landingpad pair index".into());
                }
                return Ok(Some(pair[idx[0] as usize].clone()));
            }
            58 => {
                let p = self
                    .pairs
                    .get(&l.get_operand(inst, 0))
                    .ok_or("resume requires a landingpad pair or acyclic pair PHI")?;
                self.needs_resume = true;
                self.lines.extend([
                    format!("    call @_Unwind_Resume({})", p[0]),
                    "    trap".into(),
                ]);
                return Ok(None);
            }
            45 | 5 => return self.call(inst, op),
            1 => {
                let text = if l.get_num_operands(inst) == 0 {
                    "    return".into()
                } else {
                    format!("    return {}", self.value(l.get_operand(inst, 0))?)
                };
                self.lines.push(text);
                return Ok(None);
            }
            7 => {
                self.lines.push("    trap".into());
                return Ok(None);
            }
            _ => {
                return Err(format!(
                    "unsupported instruction opcode {op}: {}",
                    l.printed(inst).trim().chars().take(180).collect::<String>()
                ));
            }
        };
        Ok(Some(self.emit(expr)))
    }
    fn call(&mut self, inst: Ref, op: u32) -> Result<Option<String>> {
        let l = self.l;
        self.check_function_attrs(inst, true)?;
        let callee = l.get_called_value(inst);
        let fast = self.fast.contains(&callee);
        if l.get_instruction_call_conv(inst) != if fast { 8 } else { 0 } {
            return Err("non-C call or fastcc contract mismatch".into());
        }
        if l.get_num_operand_bundles(inst) != 0 {
            return Err("call operand bundles".into());
        }
        if op == 45 && l.get_tail_call_kind(inst) == 2 {
            return Err("musttail call".into());
        }
        if self.personalities.contains(&callee) {
            return Err("personality may only be a symbolic EH reference".into());
        }
        let fty = l.get_called_function_type(inst);
        let direct = l.is_a_function(callee) != 0;
        let memory = direct && self.memory_name(callee).is_some();
        let (ret, params) = self.abi_signature(inst, fty, true, false, memory, fast)?;
        if memory {
            if op != 45
                || fty != l.global_get_value_type(callee)
                || (ret.clone(), params.clone()) != self.signatures[&callee]
                || l.get_num_arg_operands(inst) != 4
            {
                return Err("unsupported memory intrinsic call".into());
            }
            let volatile = l.get_operand(inst, 3);
            if l.is_a_constant_int(volatile) == 0 || l.const_int_get_z_ext_value(volatile) != 0 {
                return Err("volatile memory intrinsics are unsupported".into());
            }
            let a = (0..3)
                .map(|n| self.value(l.get_operand(inst, n)))
                .collect::<Result<Vec<_>>>()?;
            let name = if self.memory_name(callee) == Some("memset") {
                "memset"
            } else {
                "memmove"
            };
            self.lines
                .push(format!("    intrinsic {name}({})", a.join(", ")));
            return Ok(None);
        }
        let name = l.name(callee);
        if direct
            && matches!(
                name.as_str(),
                "llvm.lifetime.start.p0" | "llvm.lifetime.end.p0"
            )
        {
            let sig = ("void".into(), vec!["ptr".into()]);
            if op != 45
                || fty != l.global_get_value_type(callee)
                || (ret, params) != sig
                || self.signatures[&callee] != sig
                || l.get_num_arg_operands(inst) != 1
                || !self.slots.contains_key(&l.get_operand(inst, 0))
            {
                return Err(
                    "lifetime marker requires a direct fixed entry-block stack slot".into(),
                );
            }
            let pointer = self.value(l.get_operand(inst, 0))?;
            let operation = if name == "llvm.lifetime.start.p0" {
                "lifetime_start"
            } else {
                "lifetime_end"
            };
            self.lines
                .push(format!("    intrinsic {operation}({pointer})"));
            return Ok(None);
        }
        if direct
            && matches!(
                name.as_str(),
                "llvm.eh.typeid.for" | "llvm.eh.typeid.for.p0"
            )
        {
            let sig = ("i32".into(), vec!["ptr".into()]);
            if op != 45
                || l.get_num_arg_operands(inst) != 1
                || fty != l.global_get_value_type(callee)
                || (ret, params) != sig
                || self.signatures[&callee] != sig
            {
                return Err("unsupported eh.typeid intrinsic signature".into());
            }
            let info = l.get_operand(inst, 0);
            if !self.globals.objects.contains(&info) {
                return Err("eh.typeid requires a named caught typeinfo".into());
            }
            return Ok(Some(self.emit(format!("eh_typeid {}", self.symbol(info)?))));
        }
        if l.get_num_arg_operands(inst) as usize != params.len() {
            return Err("call argument count differs from signature".into());
        }
        if direct {
            if fty != l.global_get_value_type(callee) {
                return Err("call type differs from declaration".into());
            }
            if name.starts_with("llvm.") {
                return Err(format!("LLVM intrinsic {name}"));
            }
            if (ret.clone(), params.clone()) != self.signatures[&callee] {
                return Err("call-site ABI differs from declaration".into());
            }
        }
        if op == 5 && !direct {
            return Err("indirect invoke is unsupported".into());
        }
        let mut args = Vec::new();
        for n in 0..params.len() as u32 {
            let arg = l.get_operand(inst, n);
            if self.integer_pair(l.type_of(arg)) {
                let low = self.aggregate_component(arg, 0)?;
                let high = self.aggregate_component(arg, 1)?;
                let addr = self.emit(format!("stack_addr {}", self.aggregate_slots[&(inst, n)]));
                self.lines.push(format!("    store {low}, {addr}"));
                let offset = self.emit("iconst.i64 8");
                let hi = self.emit(format!("ptradd {addr}, {offset}"));
                self.lines.push(format!("    store {high}, {hi}"));
                args.push(addr);
            } else {
                args.push(self.value(arg)?);
            }
        }
        let args = args.join(", ");
        let expr = if direct {
            if op == 5 {
                let normal = l.get_successor(inst, 0);
                let unwind = l.get_successor(inst, 1);
                let bridge = format!("block{}", self.blocks.len() + self.invoke_edges.len());
                self.invoke_edges
                    .push((bridge.clone(), self.block, normal, None));
                let pad = if self.split_pads.contains(&unwind) {
                    let pad = format!("block{}", self.blocks.len() + self.invoke_edges.len());
                    self.invoke_edges.push((
                        pad.clone(),
                        self.block,
                        unwind,
                        Some(self.pad_insts[&unwind]),
                    ));
                    pad
                } else {
                    self.blocks[&unwind].clone()
                };
                format!("invoke {}({args}), {bridge}, {pad}", self.symbol(callee)?)
            } else {
                format!("call {}({args})", self.symbol(callee)?)
            }
        } else {
            let pointer = self.value(callee)?;
            format!(
                "call_indirect {pointer}({args}) : ({}){} cc(c)",
                params.join(", "),
                if ret == "void" {
                    String::new()
                } else {
                    format!(" -> {ret}")
                }
            )
        };
        let expr = format!("{expr}{}", self.contract(inst, fty, true));
        if ret == "void" {
            self.lines.push(format!("    {expr}"));
            Ok(None)
        } else {
            Ok(Some(self.emit(expr)))
        }
    }
}
