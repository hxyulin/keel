//! Bounded local ODR copies and contained fastcc edges.
use super::*;
impl Translation<'_> {
    pub(super) fn setup_functions(&mut self, functions: &[Ref], options: Options) -> Result<()> {
        let l = self.l;
        for &f in functions {
            if l.get_function_call_conv(f) == 8 {
                let fty = l.global_get_value_type(f);
                if l.get_first_basic_block(f) == 0
                    || !local(l.get_linkage(f))
                    || l.is_function_var_arg(fty) != 0
                {
                    return Err("fastcc requires a private fixed-arity definition".into());
                }
                for u in sequence(|| l.get_first_use(f), |u| l.get_next_use(u)) {
                    let user = l.get_user(u);
                    if l.is_a_instruction(user) == 0
                        || !matches!(l.get_instruction_opcode(user), 45 | 5)
                        || l.get_called_value(user) != f
                        || l.get_called_function_type(user) != fty
                        || l.get_instruction_call_conv(user) != 8
                    {
                        return Err(
                            "fastcc requires contained direct calls with matching contracts".into(),
                        );
                    }
                    if (0..l.get_num_arg_operands(user)).any(|n| l.get_operand(user, n) == f) {
                        return Err(
                            "fastcc function address escapes through a call argument".into()
                        );
                    }
                }
                self.fast.insert(f);
            }
            if l.has_personality_fn(f) != 0 {
                let p = l.get_personality_fn(f);
                if l.is_a_function(p) == 0 {
                    return Err("personality must be a direct runtime symbol".into());
                }
                self.personalities.insert(p);
            }
        }
        if options.localize_odr_functions {
            self.local_copies(functions)?;
        }
        for &f in functions {
            let sig = self
                .signature(f)
                .map_err(|e| format!("{}: {e}", l.name(f)))?;
            self.signatures.insert(f, sig);
        }
        for &p in &self.personalities {
            for u in sequence(|| l.get_first_use(p), |u| l.get_next_use(u)) {
                let user = l.get_user(u);
                if !(l.is_a_function(user) != 0
                    && l.has_personality_fn(user) != 0
                    && l.get_personality_fn(user) == p)
                {
                    return Err("personality may only be a symbolic EH reference".into());
                }
            }
        }
        Ok(())
    }
    fn local_copies(&mut self, functions: &[Ref]) -> Result<()> {
        let l = self.l;
        let candidates: HashSet<_> = functions
            .iter()
            .copied()
            .filter(|&f| matches!(l.get_linkage(f), 1 | 3))
            .collect();
        for &f in &candidates {
            if l.get_first_basic_block(f) == 0 {
                return Err("local ODR copy: requires a definition".into());
            }
            if l.get_comdat(f) != 0 {
                return Err("local ODR copy: COMDAT selection/group contract unsupported".into());
            }
            for u in sequence(|| l.get_first_use(f), |u| l.get_next_use(u)) {
                let user = l.get_user(u);
                if l.is_a_call_inst(user) == 0
                    || l.get_called_value(user) != f
                    || (0..l.get_num_arg_operands(user)).any(|n| l.get_operand(user, n) == f)
                    || l.get_num_operand_bundles(user) != 0
                {
                    return Err("local ODR copy: address escape or non-direct-call use".into());
                }
            }
        }
        let mut pending: Vec<_> = candidates.iter().copied().collect();
        let mut visited = HashSet::new();
        while let Some(f) = pending.pop() {
            if !visited.insert(f) {
                continue;
            }
            for b in sequence(|| l.get_first_basic_block(f), |b| l.get_next_basic_block(b)) {
                for inst in sequence(|| l.get_first_instruction(b), |i| l.get_next_instruction(i)) {
                    let count = if l.is_a_call_inst(inst) != 0 {
                        let callee = l.get_called_value(inst);
                        if l.is_a_function(callee) == 0
                            || l.get_first_basic_block(callee) == 0
                            || (!candidates.contains(&callee) && !local(l.get_linkage(callee)))
                        {
                            return Err(
                                "local ODR copy: call outside defined local/ODR helper closure"
                                    .into(),
                            );
                        }
                        if l.get_num_operand_bundles(inst) != 0 {
                            return Err("local ODR copy: operand bundle".into());
                        }
                        pending.push(callee);
                        l.get_num_arg_operands(inst)
                    } else {
                        l.get_num_operands(inst) as u32
                    };
                    for n in 0..count {
                        self.local_operand(l.get_operand(inst, n))?;
                    }
                }
            }
        }
        self.copies = candidates;
        Ok(())
    }
    fn local_operand(&self, v: Ref) -> Result<()> {
        let l = self.l;
        if l.is_a_function(v) != 0 {
            return Err("local ODR copy: function address dependency".into());
        }
        if l.is_a_global_variable(v) != 0 {
            return Err("local ODR copy: named storage dependency".into());
        }
        if l.is_a_constant(v) != 0 {
            for n in 0..l.get_num_operands(v) as u32 {
                self.local_operand(l.get_operand(v, n))?;
            }
        }
        Ok(())
    }
}
