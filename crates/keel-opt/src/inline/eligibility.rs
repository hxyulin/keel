//! Whether a callee body can be copied into another function at all.
use super::cost::HARD_STACK;
use keel_ir::{Function, InstData, Intrinsic, Purpose};

pub(super) fn eligibility_reason(
    f: &Function,
    admit_noninterposable: bool,
) -> Option<&'static str> {
    eligibility_reason_with_frames(f, admit_noninterposable, FrameSemantics::Preserve)
}

#[derive(Clone, Copy)]
pub(super) enum FrameSemantics {
    Preserve,
    CRequiredInline,
}

pub(super) fn eligibility_reason_with_frames(
    f: &Function,
    admit_noninterposable: bool,
    frames: FrameSemantics,
) -> Option<&'static str> {
    // Every copy of a weak-ODR definition has an equivalent body, so with a
    // binding promise it is admitted like a strong one. Only the caller
    // changes; the definition keeps its linkage, signature and body.
    if f.attrs.weak
        || (!f.local
            && !(admit_noninterposable
                && (f.attrs.noninterposable || f.attrs.no_semantic_interposition)))
    {
        return Some("public_definition");
    }
    body_eligibility_reason(f, frames)
}

// The intrinsic registry marks the operations that refer only to SSA operands
// and architectural state. Cloning their arguments preserves their existing
// pinning/effects. Frame observers, nonlocal control flow and owned regions
// keep their separate restrictions.
fn inlineable_target_intrinsic(id: Intrinsic) -> bool {
    id.inlineable()
}

pub(super) fn body_eligibility_reason(
    f: &Function,
    frames: FrameSemantics,
) -> Option<&'static str> {
    if f.attrs.noinline || f.attrs.optnone {
        return Some("noinline");
    }
    if f.attrs.naked || f.attrs.interrupt {
        return Some("special_entry");
    }
    if !f.attrs.alwaysinline
        && keel_ir::FunctionPolicy::resolve(&f.attrs)
            .is_ok_and(|p| p.disabled() || p.inline == Some(false))
    {
        return Some("function_optimization_policy");
    }
    if f.sig.variadic {
        return Some("variadic_callee");
    }
    if f.sig.returns_twice {
        return Some("returns_twice_callee");
    }
    if f.personality.is_some() {
        return Some("callee_personality");
    }
    if f.entry_block().is_none() {
        return Some("callee_without_entry");
    }
    if f.stack_slots().map(|(_, s)| u64::from(s.size)).sum::<u64>() > HARD_STACK {
        return Some("callee_hard_stack_declaration_bytes");
    }
    if f.sig
        .params
        .iter()
        .any(|p| matches!(&p.purpose, Purpose::ByVal(l) if l.size() > 128))
    {
        return Some("byval_snapshot_size");
    }
    for (_, i) in f.layout_insts() {
        let reason = match f.inst_data(i) {
            InstData::Call { func, .. } if f.callee(*func).sig.returns_twice => {
                Some("callee_calls_returns_twice")
            }
            InstData::CallIndirect { sig, .. } if f.signature(*sig).returns_twice => {
                Some("callee_calls_returns_twice")
            }
            InstData::Intrinsic { id, .. }
                if !matches!(
                    id,
                    Intrinsic::Memcpy
                        | Intrinsic::Memmove
                        | Intrinsic::Memset
                        | Intrinsic::Debugtrap
                        | Intrinsic::LifetimeStart
                        | Intrinsic::LifetimeEnd
                ) && !inlineable_target_intrinsic(*id)
                    && !(matches!(frames, FrameSemantics::CRequiredInline)
                        && matches!(id, Intrinsic::FrameAddress | Intrinsic::StackSave)) =>
            {
                Some("unsupported_callee_intrinsic")
            }
            InstData::VaArg { .. } => Some("callee_va_arg"),
            InstData::Invoke { .. } => Some("callee_invoke"),
            InstData::LandingPad { .. } | InstData::EhTypeid { .. } => {
                Some("callee_exception_instruction")
            }
            InstData::ReturnCall { .. } | InstData::ReturnCallIndirect { .. } => {
                Some("callee_tail_call")
            }
            InstData::X86RflagsRegion { .. } => Some("callee_owned_flags_region"),
            InstData::InlineAsm { asm, .. } if !f.attrs.alwaysinline && !asm_is_duplicable(asm) => {
                Some("callee_inline_asm")
            }
            _ => None,
        };
        if reason.is_some() {
            return reason;
        }
    }
    None
}

/// Whether a copy of `asm` in another function assembles and allocates as it
/// does in its own. Every copy repeats the labels its template defines, so
/// only numeric local labels and labels made unique with `%=` are allowed.
/// Asm goto reaches the IR as a selector output whose labels use `%=`.
fn asm_is_duplicable(asm: &keel_ir::InlineAsm) -> bool {
    use keel_ir::{AsmArch, AsmClobber, AsmOperand, PhysReg};
    // A caller's frame can reserve a register the callee's did not: x19 is
    // the AArch64 base pointer of a function that allocates stack, and r15
    // the x86 base of an over-aligned frame, which leaves 13 x86 GPRs.
    let frame_base = |r: &PhysReg| {
        !r.float
            && match r.arch {
                AsmArch::Aarch64 => r.num == 19,
                AsmArch::X86 => r.num == 15,
            }
    };
    let operands = asm.outputs.iter().map(|(op, _)| op).chain(&asm.inputs);
    let registers = asm.outputs.len()
        + asm.inputs.len()
        + asm
            .clobbers
            .iter()
            .filter(|c| matches!(c, AsmClobber::Reg(_)))
            .count();
    registers <= 13
        && !asm
            .clobbers
            .iter()
            .any(|c| matches!(c, AsmClobber::Reg(r) if frame_base(r)))
        && !operands.into_iter().any(|op| match op {
            AsmOperand::Fixed(r) => frame_base(r),
            // A snapshot of the callee's stack pointer.
            AsmOperand::StackPointer => true,
            _ => false,
        })
        && asm_template_is_duplicable(&asm.template)
}

/// Rejects named label definitions and directives that define a symbol or
/// macro. Splitting at `;` as well as newlines can only reject more.
fn asm_template_is_duplicable(template: &str) -> bool {
    // The length of a leading symbol name, including `%` substitutions.
    let name_len = |text: &str| {
        let bytes = text.as_bytes();
        let mut end = 0;
        while end < bytes.len() {
            if bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'.' | b'$') {
                end += 1;
            } else if bytes[end] == b'%' {
                end += 1;
                while end < bytes.len() && matches!(bytes[end], b'=' | b'[' | b']') {
                    end += 1;
                }
            } else {
                break;
            }
        }
        end
    };
    template.split(['\n', ';']).all(|statement| {
        let mut rest = statement.trim_start();
        // Each leading `name:` defines a label.
        loop {
            let (name, after) = rest.split_at(name_len(rest));
            let after = after.trim_start();
            if !name.is_empty() && after.starts_with(':') {
                if !(name.bytes().all(|b| b.is_ascii_digit()) || name.contains("%=")) {
                    return false;
                }
                rest = after[1..].trim_start();
                continue;
            }
            // `name = value` assigns a symbol; `==` is a comparison.
            if !name.is_empty() && after.starts_with('=') && !after.starts_with("==") {
                return false;
            }
            let directive = name.to_ascii_lowercase();
            return !matches!(
                directive.as_str(),
                ".set" | ".equ" | ".equiv" | ".eqv" | ".macro" | ".comm" | ".lcomm" | ".symver"
            );
        }
    })
}

#[cfg(test)]
mod report_tests {
    use super::*;
    use keel_ir::{EntityRef, Type};
    // Isolated guard fixtures may be structurally invalid whole functions: these
    // test exact decisions without implying the verifier accepts such inputs.
    #[test]
    fn eligibility_reports_each_unsupported_construct() {
        let source = "func @h(ptr) local {\nblock0(v0: ptr):\n return\n}";
        for (body, expected) in [
            ("v1 = va_arg.i64 v0\n return", "callee_va_arg"),
            (
                "v1, v2 = landingpad cleanup\n return",
                "callee_exception_instruction",
            ),
            (
                "v1 = eh_typeid @type\n return",
                "callee_exception_instruction",
            ),
            ("return_call @external(v0)", "callee_tail_call"),
            ("return_call_indirect v0(v0) : (ptr)", "callee_tail_call"),
            (
                "invoke @external(v0), block1, block2\nblock1:\n return\nblock2:\n trap",
                "callee_invoke",
            ),
        ] {
            let text = format!(
                "decl @external(ptr)\ndecl data @type\n{}",
                source.replace(" return", body)
            );
            let module = keel_ir::parse(&text).unwrap();
            assert_eq!(
                eligibility_reason(&module.functions[0], false),
                Some(expected),
                "{text}"
            );
        }
        for (attrs, expected) in [
            ("returns_twice", "returns_twice_callee"),
            ("personality @p", "callee_personality"),
        ] {
            let text = if attrs == "returns_twice" {
                source.replace("local {", "returns_twice local {")
            } else {
                source.replace("local {", &format!("local {attrs} {{"))
            };
            let module = keel_ir::parse(&text).unwrap();
            assert_eq!(
                eligibility_reason(&module.functions[0], false),
                Some(expected)
            );
        }
        let mut f = Function::new("empty", keel_ir::Signature::new(&[], &[]));
        f.local = true;
        assert_eq!(eligibility_reason(&f, false), Some("callee_without_entry"));
        let fields = vec!["i64"; 17].join(", ");
        let module = keel_ir::parse(&format!(
            "func @h(ptr byval {{{fields}}}) local {{\nblock0(v0: ptr):\n return\n}}"
        ))
        .unwrap();
        assert_eq!(
            eligibility_reason(&module.functions[0], false),
            Some("byval_snapshot_size")
        );
        for (template, constraints, inlinable) in [
            ("", "", true),
            ("cmp %1, %2\n\tcmova %3, %0", "=r,0,r,r,r", true),
            ("1:\n\tjnz 1b", "~{cc}", true),
            (".Lspin_%=: pause; jmp .Lspin_%=", "", true),
            ("lbl_%=: 1: nop", "~{x0}", true),
            ("a == 1", "", true),
            ("spin: jmp spin", "", false),
            ("nop\n  done :", "", false),
            ("1: x%0: nop", "", false),
            ("sym = 1", "", false),
            (".set sym, 1", "", false),
            ("  .MACRO m\n.endm", "", false),
            ("nop", "~{x19}", false),
            ("nop", "~{r15}", false),
            ("nop", "{r15}", false),
            ("nop", "rsp", false),
            ("nop", &vec!["r"; 14].join(","), false),
        ] {
            let mut module = keel_ir::parse(source).unwrap();
            let f = &mut module.functions[0];
            let inst = f.layout_insts().next().unwrap().1;
            let outputs = constraints.matches('=').count();
            let asm =
                keel_ir::InlineAsm::new(template, constraints, &vec![Type::I64; outputs]).unwrap();
            let args = vec![keel_ir::Value::new(0); asm.inputs.len()];
            *f.inst_data_mut(inst) = InstData::InlineAsm {
                asm: Box::new(asm),
                args,
            };
            let expected = (!inlinable).then_some("callee_inline_asm");
            assert_eq!(
                eligibility_reason(f, false),
                expected,
                "{template:?} {constraints}"
            );
        }
        for indirect in [false, true] {
            let mut module = keel_ir::parse(source).unwrap();
            let f = &mut module.functions[0];
            let mut sig = keel_ir::Signature::new(&[], &[]);
            sig.returns_twice = true;
            let call = if indirect {
                let sig = f.import_signature(sig);
                InstData::CallIndirect {
                    sig,
                    callee: keel_ir::Value::new(0),
                    args: vec![].into(),
                }
            } else {
                let func = f.import_function("external", sig);
                InstData::Call {
                    func,
                    args: vec![].into(),
                }
            };
            let inst = f.layout_insts().next().unwrap().1;
            *f.inst_data_mut(inst) = call;
            assert_eq!(
                eligibility_reason(f, false),
                Some("callee_calls_returns_twice")
            );
        }
    }
}
