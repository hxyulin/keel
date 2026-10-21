//! Bounded, module-local preservation proofs for finalized integer call chains.
//!
//! x9-x15 have no implicit uses in integer lowering: every use belongs to the
//! allocation (including sparse transfers). Local ABI shuffles and lowering
//! scratch use reserved x0-x8/x16/x17. Nested calls additionally clobber the
//! bank and require finalized proofs; helpers are excluded. FP is unchanged.
use super::*;
use std::collections::{HashMap, HashSet};

pub(super) type Summaries = HashMap<String, (Signature, u64)>;
pub(super) type Prepared = HashMap<usize, (Output, Vec<(String, Option<DebugScope>)>)>;

pub(super) fn eligible(func: &Function, module: &Module) -> bool {
    func.local
        && !func.weak_odr
        && func.sig.cc == CallConv::Fast
        && !func.sig.variadic
        && !func.sig.returns_twice
        && func.sig.params.len() <= 8
        && func.sig.returns.len() <= 1
        && module.files.is_empty()
        && func.personality.is_none()
        && func.num_stack_slots() == 0
        && func.sig.params.iter().chain(&func.sig.returns).all(|p| p.purpose == Purpose::Normal)
        && (0..func.num_values()).all(|i| matches!(func.value_type(Value::new(i)), Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr))
        && !func.layout_insts().any(|(_, i)| {
            let data = func.inst_data(i);
            (is_call(func, data) && !matches!(data, InstData::Call { .. }))
                || matches!(data, InstData::Intrinsic { .. } | InstData::InlineAsm { .. } | InstData::ReturnCall { .. } | InstData::ReturnCallIndirect { .. } | InstData::LandingPad { .. })
                || matches!(data, InstData::SymbolAddr { symbol } if module.data_object(func.symbol_name(*symbol)).is_some_and(|d| d.thread_local))
        })
}

pub(super) fn prepare(
    module: &Module,
    pools: &Pools,
    pipeline: &crate::CodegenPipeline<'_>,
    structured: bool,
) -> Result<(Summaries, Prepared), CodegenError> {
    let mut summaries = Summaries::new();
    let mut prepared = Prepared::new();
    if !module.files.is_empty()
        || !module
            .functions
            .iter()
            .any(|f| f.local && f.sig.cc == CallConv::Fast)
    {
        return Ok((summaries, prepared));
    }
    let called: HashSet<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.layout_insts().filter_map(|(_, i)| {
                let InstData::Call { func: callee, .. } = f.inst_data(i) else {
                    return None;
                };
                let decl = f.callee(*callee);
                (decl.sig.cc == CallConv::Fast).then_some(decl.name.as_str())
            })
        })
        .collect();
    // Work is bounded independently of call-graph shape. Each round sees only
    // summaries finalized in earlier rounds, so source order cannot affect the
    // depth limit. Cycles (and their callers) never become ready. A chain with
    // an unknown, mismatched, or ineligible callee also retains ABI clobbers.
    const MAX_CHAIN_FUNCTIONS: usize = 512;
    const MAX_DEPTH: usize = 16;
    const MAX_INSTRUCTIONS: usize = 65_536;
    const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
    const MAX_OUTPUT_ITEMS: usize = 65_536;
    let mut budget = MAX_INSTRUCTIONS;
    let mut pending = Vec::new();
    let mut chains = Vec::new();
    for (index, original) in module.functions.iter().enumerate() {
        let count = original.layout_insts().take(129).count();
        if !called.contains(original.name.as_str())
            || count > 128
            || original.blocks().take(17).count() > 16
            || !eligible(original, module)
        {
            continue;
        }
        if original
            .layout_insts()
            .any(|(_, i)| matches!(original.inst_data(i), InstData::Call { .. }))
        {
            if chains.len() < MAX_CHAIN_FUNCTIONS {
                chains.push((index, count));
            }
        } else if count <= budget {
            // Preserve the previous leaf coverage before spending any budget
            // on newly eligible non-leaves. A module-wide candidate cap used
            // to drop late leaves and add saves to their existing callers.
            budget -= count;
            pending.push(index);
        }
    }
    for (index, count) in chains {
        if count <= budget {
            budget -= count;
            pending.push(index);
        }
    }
    let caller_mask = REGISTERS
        .caller_saved
        .iter()
        .fold(0, |mask, &r| mask | (1u64 << r));
    let mut output_bytes = 0;
    let mut output_items = 0;
    for _ in 0..MAX_DEPTH {
        let mut ready = Vec::new();
        pending.retain(|&index| {
            let original = &module.functions[index];
            let mut nested_clobbers = 0;
            for (_, inst) in original.layout_insts() {
                if let InstData::Call { func: callee, .. } = original.inst_data(inst) {
                    let decl = original.callee(*callee);
                    let Some((sig, preserved)) = summaries.get(&decl.name) else {
                        return true;
                    };
                    if &decl.sig != sig {
                        return true;
                    }
                    nested_clobbers |= caller_mask & !preserved;
                }
            }
            ready.push((index, nested_clobbers));
            false
        });
        if ready.is_empty() {
            break;
        }
        for (index, nested_clobbers) in ready {
            let original = &module.functions[index];
            #[cfg(feature = "backend-profile")]
            let _function_profile = crate::backend_profile::function(&original.name);
            let func = vector::lower(original);
            let compiler =
                FunctionCompiler::new(&func, module, pools, pipeline, structured, &summaries)?;
            // finish/validate have accounted for defs AND uses, including
            // allocation transfers. Integer ABI shuffles and lowering scratch
            // use x0-x8/x16/x17, outside this proof's x9-x15 bank. Calls are the
            // only admitted implicit clobbers and are covered transitively.
            let used = compiler.alloc.mentioned_gprs() | nested_clobbers;
            let preserved = caller_mask & !used;
            let mut output = Output::new(structured);
            let marks = compiler.compile(&mut output);
            match &output {
                Output::Text(text) => output_bytes += text.len(),
                Output::Structured(program) => output_items += program.items.len(),
            }
            if output_bytes > MAX_OUTPUT_BYTES || output_items > MAX_OUTPUT_ITEMS {
                // Never publish a summary for a body that will be regenerated.
                return Ok((summaries, prepared));
            }
            summaries.insert(original.name.clone(), (original.sig.clone(), preserved));
            prepared.insert(index, (output, marks));
        }
    }
    Ok((summaries, prepared))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(count: usize) -> String {
        let mut source = String::new();
        for i in 0..count {
            let body = if i + 1 == count {
                "return v0".to_owned()
            } else {
                format!("v1 = call @f{}(v0)\n return v1", i + 1)
            };
            source.push_str(&format!(
                "func @f{i}(i64) -> i64 cc(fast) local {{\nblock0(v0:i64):\n {body}\n}}\n"
            ));
        }
        source
    }

    fn proof(source: &str) -> (Summaries, Prepared) {
        let module = keel_ir::parse(source).unwrap();
        // Also exercise defensive producer guards on verifier-rejected IR.
        prepare(
            &module,
            &REGISTERS,
            &crate::CodegenPipeline {
                selector: &crate::SelectionTier::Full,
                allocator: &crate::regalloc::AllocationTier::Full,
            },
            false,
        )
        .unwrap()
    }

    #[test]
    fn bottom_up_depth_budget_and_body_identity() {
        let (summaries, prepared) = proof(&chain(20));
        assert_eq!(summaries.len(), 16);
        assert_eq!(prepared.len(), summaries.len());
        assert!(summaries.contains_key("f4"));
        assert!(!summaries.contains_key("f3"));
        for (index, (body, _)) in prepared {
            let Output::Text(text) = body else { panic!() };
            assert!(text.contains(&format!("_f{index}:\n")));
            assert!(summaries.contains_key(&format!("f{index}")));
        }
        let source = chain(4);
        let module = keel_ir::parse(&source).unwrap();
        let mut reversed = module.clone();
        reversed.functions.reverse();
        let emit = |m: &Module| {
            prepare(
                m,
                &REGISTERS,
                &crate::CodegenPipeline {
                    selector: &crate::SelectionTier::Full,
                    allocator: &crate::regalloc::AllocationTier::Full,
                },
                false,
            )
            .unwrap()
            .0
        };
        assert_eq!(emit(&module), emit(&reversed));
    }

    #[test]
    fn function_instruction_budgets_and_single_finalization() {
        let mut source = String::new();
        let mut calls = String::new();
        for i in 0..513 {
            source.push_str(&format!(
                "func @leaf{i}(i64) -> i64 cc(fast) local {{\nblock0(v0:i64):\n return v0\n}}\n"
            ));
            calls.push_str(&format!("v{} = call @leaf{i}(v0)\n", i + 1));
        }
        source.push_str(&format!(
            "func @root(i64) -> i64 {{\nblock0(v0:i64):\n {calls} return v0\n}}"
        ));
        let (summaries, prepared) = proof(&source);
        assert_eq!(summaries.len(), 513);
        assert_eq!(prepared.len(), 513);

        // New non-leaves must not crowd out old leaves, including leaves
        // appearing late in the module. Only the extension has a 512 cap.
        let mut source = String::new();
        let mut calls = String::new();
        for i in 0..513 {
            source.push_str(&format!(
                "func @wrapper{i}(i64) -> i64 cc(fast) local {{\nblock0(v0:i64):\n v1 = call @leaf{i}(v0)\n return v1\n}}\n"
            ));
            calls.push_str(&format!("v{} = call @wrapper{i}(v0)\n", i + 1));
        }
        for i in 0..513 {
            source.push_str(&format!(
                "func @leaf{i}(i64) -> i64 cc(fast) local {{\nblock0(v0:i64):\n return v0\n}}\n"
            ));
        }
        source.push_str(&format!(
            "func @root(i64) -> i64 {{\nblock0(v0:i64):\n {calls} return v0\n}}"
        ));
        let (summaries, prepared) = proof(&source);
        assert_eq!(summaries.len(), 513 + 512);
        assert_eq!(prepared.len(), summaries.len());
        assert!(summaries.contains_key("leaf512"));
        assert!(!summaries.contains_key("wrapper512"));

        let source = chain(3).replace(
            "return v0",
            &format!(
                "{}return v0",
                (1..130)
                    .map(|i| format!("v{i} = iconst.i64 {i}\n"))
                    .collect::<String>()
            ),
        );
        assert!(proof(&source).0.is_empty());

        struct Counting(std::cell::RefCell<HashMap<String, usize>>);
        impl crate::regalloc::RegisterAllocator for Counting {
            fn name(&self) -> &'static str {
                "count-finalizations"
            }
            fn allocate(
                &self,
                input: &crate::regalloc::AllocationInput<'_>,
            ) -> crate::regalloc::Allocation {
                *self
                    .0
                    .borrow_mut()
                    .entry(input.func.name.clone())
                    .or_default() += 1;
                crate::regalloc::AllocationTier::Full.allocate(input)
            }
        }
        let allocator = Counting(Default::default());
        let pipeline = crate::CodegenPipeline {
            selector: &crate::SelectionTier::Full,
            allocator: &allocator,
        };
        let module = keel_ir::parse(&chain(4)).unwrap();
        super::super::compile_with_pipeline(&module, &REGISTERS, &pipeline).unwrap();
        assert_eq!(allocator.0.borrow().len(), 4);
        assert!(allocator.0.borrow().values().all(|&count| count == 1));
        allocator.0.borrow_mut().clear();
        super::super::compile_program_with_pipeline(&module, &REGISTERS, &pipeline).unwrap();
        assert_eq!(allocator.0.borrow().len(), 4);
        assert!(allocator.0.borrow().values().all(|&count| count == 1));
    }

    #[test]
    fn unknown_recursive_and_guarded_edges_never_publish_chain_proofs() {
        let source = chain(3);
        for modified in [
            source.replace("return v0", "v1 = call @f0(v0)\n return v1"),
            source.replace("return v0", "v1 = call @f2(v0)\n return v1"),
            format!(
                "decl @unknown(i64) -> i64\n{}",
                source.replace("return v0", "v1 = call @unknown(v0)\n return v1")
            ),
            source.replace("return v0", "asm \"nop\", \"\"()\n return v0"),
            source.replace(
                "@f2(i64) -> i64 cc(fast) local",
                "@f2(i64) -> i64 cc(fast) returns_twice local",
            ),
            source.replace(
                "@f2(i64) -> i64 cc(fast) local",
                "@f2(i64) -> i64 cc(fast) weak_odr",
            ),
            source.replace(
                "return v0",
                "v1 = inttoptr.ptr v0\n v2 = call_indirect v1(v0) : (i64) -> i64\n return v2",
            ),
            source.replace(
                "return v0",
                "v1 = inttoptr.ptr v0\n intrinsic memcpy(v1,v1,v0)\n return v0",
            ),
            source.replace("return v0", "return_call @f2(v0)"),
            format!(
                "data @tls = thread_local align 8 {{ i64 5 }}\n{}",
                source.replace("return v0", "v1 = symbol_addr @tls\n return v0")
            ),
            format!(
                "decl @personality()\n{}",
                source.replace(
                    "@f2(i64) -> i64 cc(fast) local",
                    "@f2(i64) -> i64 cc(fast) local personality @personality"
                )
            ),
            format!("file f0 = \"chain.keel\"\n{source}"),
        ] {
            assert!(proof(&modified).0.is_empty(), "{modified}");
        }
        // A declaration mismatch must not inherit a same-name definition's
        // summary, even before the module verifier rejects the malformed IR.
        let mut module = keel_ir::parse(&source).unwrap();
        let callee = module.functions[1].callees().next().unwrap().0;
        module.functions[1].callee_mut(callee).sig.returns[0].ext = keel_ir::ArgExt::Uext;
        let summaries = prepare(
            &module,
            &REGISTERS,
            &crate::CodegenPipeline {
                selector: &crate::SelectionTier::Full,
                allocator: &crate::regalloc::AllocationTier::Full,
            },
            false,
        )
        .unwrap()
        .0;
        assert!(summaries.contains_key("f2"));
        assert!(!summaries.contains_key("f1"));
    }
}
