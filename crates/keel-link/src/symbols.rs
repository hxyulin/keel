//! Stable global identities with definitions iterated in name order.

use crate::{Result, Target};
use std::{collections::BTreeMap, num::NonZeroU32};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GlobalId(NonZeroU32);
impl GlobalId {
    pub(crate) fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

#[derive(Default)]
pub(crate) struct Globals {
    names: BTreeMap<String, GlobalId>,
    targets: Vec<Option<Target>>,
}
impl Globals {
    pub(crate) fn intern(&mut self, name: &str) -> Result<GlobalId> {
        if let Some(&id) = self.names.get(name) {
            return Ok(id);
        }
        let value = u32::try_from(self.targets.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .and_then(NonZeroU32::new)
            .ok_or("global symbol table exceeds 32-bit IDs")?;
        let id = GlobalId(value);
        self.targets.push(None);
        self.names.insert(name.into(), id);
        Ok(id)
    }
    pub(crate) fn id(&self, name: &str) -> Option<GlobalId> {
        self.names.get(name).copied()
    }
    pub(crate) fn target(&self, id: GlobalId) -> Option<Target> {
        self.targets[id.index()]
    }
    pub(crate) fn set(&mut self, id: GlobalId, target: Target) {
        self.targets[id.index()] = Some(target);
    }
    pub(crate) fn insert(&mut self, name: String, target: Target) -> Result<()> {
        let id = self.intern(&name)?;
        self.set(id, target);
        Ok(())
    }
    pub(crate) fn get(&self, name: &str) -> Option<&Target> {
        let id = self.names.get(name)?;
        self.targets[id.index()].as_ref()
    }
    pub(crate) fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
    pub(crate) fn id_count(&self) -> usize {
        self.targets.len()
    }
    // Clearing a definition keeps its identity valid for existing source IDs.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&str, &mut Target) -> bool) {
        for (name, id) in &self.names {
            let target = &mut self.targets[id.index()];
            if target.as_mut().is_some_and(|t| !keep(name, t)) {
                *target = None;
            }
        }
    }
}
pub(crate) struct Definitions<'a> {
    names: std::collections::btree_map::Iter<'a, String, GlobalId>,
    targets: &'a [Option<Target>],
}
impl<'a> Iterator for Definitions<'a> {
    type Item = (&'a String, &'a Target);
    fn next(&mut self) -> Option<Self::Item> {
        for (name, id) in self.names.by_ref() {
            if let Some(target) = &self.targets[id.index()] {
                return Some((name, target));
            }
        }
        None
    }
}
impl<'a> IntoIterator for &'a Globals {
    type Item = (&'a String, &'a Target);
    type IntoIter = Definitions<'a>;
    fn into_iter(self) -> Self::IntoIter {
        Definitions {
            names: self.names.iter(),
            targets: &self.targets,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_survive_pruning_and_definitions_keep_name_order() {
        let mut globals = Globals::default();
        let z = globals.intern("z").unwrap();
        let a = globals.intern("a").unwrap();
        let unused = globals.intern("unused").unwrap();
        assert_eq!(globals.intern("z").unwrap(), z);
        assert!(globals.get("unused").is_none());
        globals.set(z, Target::Definition(2, 7));
        globals.set(a, Target::Common(8));
        assert_eq!(
            (&globals)
                .into_iter()
                .map(|(s, _)| s.as_str())
                .collect::<Vec<_>>(),
            ["a", "z"]
        );
        globals.retain(|name, _| name != "a");
        assert!(globals.target(a).is_none());
        assert_eq!(globals.intern("a").unwrap(), a);
        globals.set(a, Target::Import(3));
        assert!(matches!(globals.target(a), Some(Target::Import(3))));
        assert!(matches!(globals.target(z), Some(Target::Definition(2, 7))));
        assert!(globals.target(unused).is_none());
        assert_eq!(
            (&globals)
                .into_iter()
                .map(|(s, _)| s.as_str())
                .collect::<Vec<_>>(),
            ["a", "z"]
        );
    }
}
