//! Driver unwind requests. Synchronous and asynchronous flags are independent.
use keel_codegen::UnwindTables;

pub(crate) struct UnwindOptions {
    synchronous: bool,
    asynchronous: bool,
}

impl Default for UnwindOptions {
    fn default() -> Self {
        Self {
            synchronous: false,
            asynchronous: true,
        }
    }
}

impl UnwindOptions {
    pub(crate) fn set(&mut self, flag: &str) {
        match flag {
            "-funwind-tables" => self.synchronous = true,
            "-fno-unwind-tables" => self.synchronous = false,
            "-fasynchronous-unwind-tables" => self.asynchronous = true,
            "-fno-asynchronous-unwind-tables" => self.asynchronous = false,
            _ => unreachable!("validated unwind option"),
        }
    }

    pub(crate) fn policy(&self) -> UnwindTables {
        if self.synchronous || self.asynchronous {
            UnwindTables::All
        } else {
            UnwindTables::RequiredOnly
        }
    }
}
