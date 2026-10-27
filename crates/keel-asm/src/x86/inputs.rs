//! Explicit binary inputs, resolved before layout and reused across passes.
use keel_asm_ir::x86;
use std::collections::{BTreeMap, btree_map::Entry};

/// Supplies the bytes named by `.incbin`. The caller owns path lookup and I/O.
/// Each distinct name is resolved once per assembly, including across chunks.
pub type BinaryInputResolver<'a> = dyn FnMut(&str) -> Result<Vec<u8>, String> + 'a;

/// Bound emitted data consistently with the assembler's space directives.
const MAX_INCBIN_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Cache(BTreeMap<String, Vec<u8>>);

pub(super) fn unavailable(name: &str) -> Result<Vec<u8>, String> {
    Err(format!(
        ".incbin `{name}` requires an explicit binary-input resolver"
    ))
}

impl Cache {
    pub fn include(
        &mut self,
        text: &str,
        resolver: &mut BinaryInputResolver<'_>,
    ) -> Result<Option<Vec<u8>>, String> {
        let (directive, args) = text
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((text.trim(), ""));
        if directive != ".incbin" {
            return Ok(None);
        }
        let fields = x86::split_inline(args, ',');
        if !(1..=3).contains(&fields.len()) {
            return Err(".incbin expects a quoted filename, optional skip and count".into());
        }
        let bytes = x86::unquote(fields[0].trim())?;
        let name = String::from_utf8(bytes).map_err(|_| ".incbin filenames must be UTF-8")?;
        if name.is_empty() || name.contains('\0') {
            return Err(".incbin filename must be nonempty and contain no NUL".into());
        }
        let size = |field: &str| -> Result<usize, String> {
            if field.trim().is_empty() {
                return Ok(0);
            }
            usize::try_from(x86::expression::constant(field)?).map_err(|_| {
                ".incbin skip/count must be nonnegative and fit the host address space".into()
            })
        };
        let skip = fields.get(1).map_or(Ok(0), |s| size(s))?;
        let count = fields.get(2).map(|s| size(s)).transpose()?;
        let input = match self.0.entry(name) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let name = entry.key();
                let bytes = resolver(name).map_err(|error| format!(".incbin `{name}`: {error}"))?;
                entry.insert(bytes)
            }
        };
        // GNU as treats an explicit zero count as the remaining file, like an
        // omitted count. It still checks the filename and skip in that case.
        let count = match count {
            Some(n) if n != 0 => n,
            _ => input
                .len()
                .checked_sub(skip)
                .ok_or(".incbin skip exceeds the input size")?,
        };
        let end = skip
            .checked_add(count)
            .filter(|&end| end <= input.len())
            .ok_or(".incbin range exceeds the input size")?;
        if count > MAX_INCBIN_BYTES {
            return Err(".incbin output exceeds 64 MiB".into());
        }
        Ok(Some(input[skip..end].to_vec()))
    }
}
