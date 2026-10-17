//! Opt-in observations. Counts describe live layout IR and declared stack objects,
//! never backend frame size. Timings are wall-clock nanoseconds, not CPU time.
use keel_ir::{EntityRef, Function, Inst, Module};
use std::fmt::Write;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IrCounts {
    pub functions: usize,
    pub blocks: usize,
    pub instructions: usize,
    pub block_parameters: usize,
    pub stack_declarations: usize,
    pub stack_declaration_bytes: u64,
}
impl IrCounts {
    pub(crate) fn function(f: &Function) -> Self {
        Self {
            functions: 1,
            blocks: f.blocks().count(),
            instructions: f.layout_insts().count(),
            block_parameters: f.blocks().map(|b| f.block_params(b).len()).sum(),
            stack_declarations: f.stack_slots().count(),
            stack_declaration_bytes: f.stack_slots().map(|(_, s)| u64::from(s.size)).sum(),
        }
    }
    pub(crate) fn module(m: &Module) -> Self {
        let mut total = Self::default();
        for f in &m.functions {
            let c = Self::function(f);
            total.functions += c.functions;
            total.blocks += c.blocks;
            total.instructions += c.instructions;
            total.block_parameters += c.block_parameters;
            total.stack_declarations += c.stack_declarations;
            total.stack_declaration_bytes += c.stack_declaration_bytes;
        }
        total
    }
    fn json(&self) -> String {
        format!(
            "{{\"functions\":{},\"blocks\":{},\"instructions\":{},\"block_parameters\":{},\"stack_declarations\":{},\"stack_declaration_bytes\":{}}}",
            self.functions,
            self.blocks,
            self.instructions,
            self.block_parameters,
            self.stack_declarations,
            self.stack_declaration_bytes
        )
    }
}

#[derive(Debug)]
pub struct InlineSite {
    pub caller: String,
    pub callee: String,
    /// Original instruction identity in this pass's frozen input site list.
    pub site: usize,
    pub category: &'static str,
    /// `inlined`, or the first failed check in actual decision order.
    pub outcome: &'static str,
}
impl InlineSite {
    pub(crate) fn new(
        caller: &Function,
        callee: &str,
        site: Inst,
        requested: bool,
        outcome: &'static str,
    ) -> Self {
        Self {
            caller: caller.name.clone(),
            callee: callee.into(),
            site: site.index(),
            category: if requested {
                "alwaysinline"
            } else {
                "ordinary"
            },
            outcome,
        }
    }
}

#[derive(Debug)]
pub struct PassObservation {
    /// Pipeline index; repeated passes retain distinct identities.
    pub pipeline_index: usize,
    pub pass: &'static str,
    pub target: crate::VectorTarget,
    /// None denotes a module invocation.
    pub function: Option<String>,
    pub changed: bool,
    pub pass_ns: u128,
    /// Whether repeated verification was enabled for this invocation.
    pub verification_enabled: bool,
    /// Exact zero when disabled; excludes timer overhead for skipped checks.
    pub verification_ns: u128,
    pub restoration_ns: u128,
    pub restoration_verification_ns: u128,
    pub restoration_changed: Option<bool>,
    /// Before restoration, where required, and after the named pass.
    pub before: IrCounts,
    pub after: IrCounts,
    pub inline_sites: Vec<InlineSite>,
}
#[derive(Debug, Default)]
pub struct OptimizationReport {
    pub invocations: Vec<PassObservation>,
}
/// JSON string escaping, also used for arbitrary function names.
fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                write!(out, "\\u{:04x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
impl OptimizationReport {
    /// Stable field/event order. Only timing values are nondeterministic.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\"schema_version\":2,\"invocations\":[");
        for (n, p) in self.invocations.iter().enumerate() {
            if n != 0 {
                out.push(',');
            }
            let target = match p.target {
                crate::VectorTarget::Portable => "portable",
                crate::VectorTarget::Aarch64Neon => "aarch64-neon",
                crate::VectorTarget::X86Sse2 => "x86-sse2",
            };
            write!(out, "{{\"pipeline_index\":{},\"pass\":{},\"target\":{},\"scope\":{},\"function\":{},\"changed\":{},\"pass_ns\":{},\"verification_enabled\":{},\"verification_ns\":{},\"restoration_ns\":{},\"restoration_verification_ns\":{},\"restoration_changed\":{},\"before\":{},\"after\":{},\"inline_sites\":[", p.pipeline_index, quoted(p.pass), quoted(target), quoted(if p.function.is_some() { "function" } else { "module" }), p.function.as_deref().map(quoted).unwrap_or_else(|| "null".into()), p.changed, p.pass_ns, p.verification_enabled, p.verification_ns, p.restoration_ns, p.restoration_verification_ns, p.restoration_changed.map(|v| v.to_string()).unwrap_or_else(|| "null".into()), p.before.json(), p.after.json()).unwrap();
            for (i, site) in p.inline_sites.iter().enumerate() {
                if i != 0 {
                    out.push(',');
                }
                write!(
                    out,
                    "{{\"caller\":{},\"callee\":{},\"site\":{},\"category\":{},\"outcome\":{}}}",
                    quoted(&site.caller),
                    quoted(&site.callee),
                    site.site,
                    quoted(site.category),
                    quoted(site.outcome)
                )
                .unwrap();
            }
            out.push_str("]}");
        }
        out.push_str("]}\n");
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_names_escape_quotes_controls_and_backslashes() {
        assert_eq!(
            super::quoted("a\"\\\n\r\t\0é"),
            "\"a\\\"\\\\\\n\\r\\t\\u0000é\""
        );
    }
}
