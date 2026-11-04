//! A flat byte memory made of separate allocations.

use crate::{Stop, Trap};
use std::collections::BTreeMap;

/// Lowest allocation address; lower addresses, null included, never hold data.
const FIRST_ADDRESS: u64 = 0x10_0000;
/// Unmapped bytes after every allocation, so that small overruns fault.
const GUARD: u64 = 64;
/// Largest single allocation, to keep a runaway `stack_alloc` from exhausting the host.
const MAX_ALLOCATION: u64 = 1 << 28;

/// Memory as separate byte allocations in one 64-bit address space: stack
/// slots, data objects, `stack_alloc` blocks and any the caller adds.
/// Addresses are never reused, and an access must lie within one live
/// allocation, so out-of-bounds and use-after-return accesses trap.
/// New memory reads as zero.
#[derive(Debug, Default)]
pub struct Memory {
    allocations: BTreeMap<u64, Allocation>,
    next: u64,
}

#[derive(Debug)]
struct Allocation {
    bytes: Vec<u8>,
    read_only: bool,
    /// False between `lifetime_end` and the next `lifetime_start`.
    live: bool,
}

impl Memory {
    /// Allocates `size` zero bytes aligned to `align` (at least 16) and gives the address.
    ///
    /// # Panics
    ///
    /// If `size` exceeds the interpreter's allocation limit (256 MiB).
    pub fn alloc(&mut self, size: u64, align: u64) -> u64 {
        self.try_alloc(size, align, false)
            .expect("allocation exceeds the interpreter limit")
    }

    /// Allocates a copy of `bytes` aligned to `align` and gives its address.
    pub fn alloc_bytes(&mut self, bytes: &[u8], align: u64) -> u64 {
        let addr = self.alloc(bytes.len() as u64, align);
        self.allocations
            .get_mut(&addr)
            .unwrap()
            .bytes
            .copy_from_slice(bytes);
        addr
    }

    /// Reads `len` bytes at `addr`.
    pub fn read(&self, addr: u64, len: u64) -> Result<&[u8], Trap> {
        let (base, allocation) = self.find(addr, len)?;
        let start = (addr - base) as usize;
        Ok(&allocation.bytes[start..start + len as usize])
    }

    /// Writes `bytes` at `addr`. Read-only data traps.
    pub fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), Trap> {
        let (base, allocation) = self.find(addr, bytes.len() as u64)?;
        if allocation.read_only {
            return Err(Trap::WriteToConstant { addr });
        }
        let start = (addr - base) as usize;
        let allocation = self.allocations.get_mut(&base).unwrap();
        allocation.bytes[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn try_alloc(
        &mut self,
        size: u64,
        align: u64,
        read_only: bool,
    ) -> Result<u64, Stop> {
        if size > MAX_ALLOCATION {
            return Err(Stop::unsupported(format!(
                "allocation of {size} bytes, above the interpreter limit"
            )));
        }
        let align = align.max(16);
        let base = self.next.max(FIRST_ADDRESS).next_multiple_of(align);
        self.next = base + size + GUARD;
        self.allocations.insert(
            base,
            Allocation {
                bytes: vec![0; size as usize],
                read_only,
                live: true,
            },
        );
        Ok(base)
    }

    /// Fills an allocation during setup, ignoring read-only protection.
    pub(crate) fn init(&mut self, base: u64, offset: u64, bytes: &[u8]) {
        let allocation = self.allocations.get_mut(&base).unwrap();
        let start = offset as usize;
        allocation.bytes[start..start + bytes.len()].copy_from_slice(bytes);
    }

    pub(crate) fn free(&mut self, base: u64) {
        self.allocations.remove(&base);
    }

    pub(crate) fn set_live(&mut self, base: u64, live: bool) -> Result<(), Stop> {
        match self.allocations.get_mut(&base) {
            Some(allocation) => {
                allocation.live = live;
                Ok(())
            }
            None => Err(Stop::Invalid(format!(
                "lifetime marker on {base:#x}, which is no stack slot"
            ))),
        }
    }

    fn find(&self, addr: u64, len: u64) -> Result<(u64, &Allocation), Trap> {
        let fault = Trap::OutOfBounds { addr, size: len };
        let (&base, allocation) = self
            .allocations
            .range(..=addr)
            .next_back()
            .ok_or(fault.clone())?;
        let end = addr.checked_add(len).ok_or(fault.clone())?;
        if !allocation.live || end > base + allocation.bytes.len() as u64 {
            return Err(fault);
        }
        Ok((base, allocation))
    }
}
