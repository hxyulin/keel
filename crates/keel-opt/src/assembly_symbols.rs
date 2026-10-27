//! Conservative symbol roots for assembly embedded in native IR.

/// Plain macro substitutions can only reuse names already present in the
/// template or its IR operands. Concatenation, generated counters, alternate
/// macro syntax and external includes need an expanded symbol inventory.
fn constructs_names(text: &str) -> bool {
    if text.contains(".altmacro")
        || text.contains(".include")
        || text
            .lines()
            .any(|line| line.contains('\\') && (line.contains('"') || line.contains('\'')))
    {
        return true;
    }
    let bytes = text.as_bytes();
    let symbol = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'$');
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        if i > 0 && symbol(bytes[i - 1]) {
            return true;
        }
        let start = i + 1;
        if start == bytes.len() || !(bytes[start].is_ascii_alphabetic() || bytes[start] == b'_') {
            return true;
        }
        i = start + 1;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        if i < bytes.len() && (symbol(bytes[i]) || bytes[i] == b'\\') {
            return true;
        }
    }
    false
}

fn interpolated_name(parts: &[&str], name: &str) -> bool {
    // A bare operand may print a register or an existing symbol, which are
    // handled separately. Generated identifier fragments require literals.
    if parts.iter().all(|part| part.is_empty()) {
        return false;
    }
    let Some(mut rest) = name.strip_prefix(parts[0]) else {
        return false;
    };
    for part in &parts[1..parts.len() - 1] {
        let Some(index) = rest.find(part) else {
            return false;
        };
        rest = &rest[index + part.len()..];
    }
    rest.ends_with(parts[parts.len() - 1])
}

fn interpolation(token: &str) -> Option<Vec<&str>> {
    let token = token.trim_start_matches('$');
    let bytes = token.as_bytes();
    let (mut i, mut start) = (0, 0);
    let mut parts = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'=' {
            break;
        }
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'%') {
            i += 2;
            continue;
        }
        let mut end = i + 1;
        if bytes.get(end) == Some(&b'=') {
            end += 1;
        } else if bytes.get(end) == Some(&b'[') {
            end = token[end..].find(']')? + end + 1;
        } else {
            if bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
                end += 1;
            }
            let number = end;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end == number {
                return None;
            }
        }
        parts.push(&token[start..i]);
        i = end;
        start = end;
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(&token[start..i]);
    Some(parts)
}

pub(crate) fn roots(module: &keel_ir::Module) -> keel_ir::fxhash::FxHashSet<String> {
    use keel_ir::{DataItem, InstData};
    let mut roots = keel_ir::fxhash::FxHashSet::default();
    let mut assembly: Vec<&str> = module.assembly.iter().map(String::as_str).collect();
    for f in &module.functions {
        if !f.local || f.attrs.used || f.attrs.retain || f.attrs.constructor || f.attrs.destructor {
            roots.insert(f.name.clone());
        }
        if let Some(personality) = &f.personality {
            roots.insert(personality.clone());
        }
        for (_, i) in f.layout_insts() {
            match f.inst_data(i) {
                InstData::InlineAsm { asm, .. } => assembly.push(&asm.template),
                InstData::SymbolAddr { symbol } => {
                    roots.insert(f.symbol_name(*symbol).to_owned());
                }
                _ => {}
            }
        }
    }
    let opaque = assembly.iter().any(|text| constructs_names(text));
    // Operand modifiers and %= may build identifiers from literal fragments.
    // Match them as wildcards, independent of the physical register chosen.
    let generated: Vec<Vec<_>> = assembly
        .iter()
        .flat_map(|text| {
            text.split(|c: char| {
                !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '%' | '=' | '[' | ']'))
            })
        })
        .filter_map(interpolation)
        .collect();
    for f in &module.functions {
        if roots.contains(&f.name) {
            continue;
        }
        let prefixed = format!("_{}", f.name);
        // Register spellings can arise from bare assembly operands. Non-C
        // identifiers also need target-specific quoting and stay conservative.
        let unusual = !f
            .name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || f.name.as_bytes().first().is_some_and(u8::is_ascii_digit)
            || keel_ir::PhysReg::parse(&f.name).is_ok();
        if opaque
            || unusual
            || assembly.iter().any(|text| text.contains(&f.name))
            || generated.iter().any(|parts| {
                interpolated_name(parts, &f.name) || interpolated_name(parts, &prefixed)
            })
        {
            roots.insert(f.name.clone());
        }
    }
    // Data is not collected by these passes, so every initializer is a root.
    for object in &module.data {
        if let Some(init) = &object.init {
            for item in &init.items {
                if let DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } = item {
                    roots.insert(symbol.clone());
                }
            }
        }
    }
    roots
}
