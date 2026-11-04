//! Stack-frame builtins: return and frame addresses.

use super::*;

/// Bounds the number of frame-record loads emitted by one C builtin.
const MAX_FRAME_ADDRESS_DEPTH: i64 = 64;

impl<'a> Parser<'a> {
    /// `__builtin_return_address`.
    pub(super) fn return_address(&mut self, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(1, loc)?;
        if self.target != Target::X86_64LinuxGnu {
            return Err(Error::new(
                loc,
                "`__builtin_return_address` requires x86-64 Linux",
            ));
        }
        let depth = integer_constant_control(&args[0], self.c_layout);
        let Some(depth @ 0..=64) = depth else {
            return Err(Error::new(
                loc,
                "`__builtin_return_address` requires constant integer depth 0..64",
            ));
        };
        Ok(mk(
            ExprKind::Builtin(Builtin::ReturnAddress(depth as u8), &[]),
            self.ptr_type(Ty::Void),
            loc,
        ))
    }

    /// `__builtin_extract_return_addr` and `__builtin_frob_return_addr`.
    pub(super) fn return_address_conversion(&mut self, base: &str, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(1, loc)?;
        if self.target != Target::X86_64LinuxGnu {
            return Err(Error::new(
                loc,
                format!("`__builtin_{base}` requires x86-64 Linux"),
            ));
        }
        // SysV x86-64 stores the actual continuation address. Other
        // targets may encode or sign it and need their own contract.
        if !args[0].ty.is_ptr() && integer_constant_control(&args[0], self.c_layout) != Some(0) {
            return Err(Error::new(
                loc,
                "return-address conversion requires a pointer",
            ));
        }
        let ptr = self.ptr_type(Ty::Void);
        self.convert_for_assign(args[0], &ptr)
    }

    /// `__builtin_frame_address`.
    pub(super) fn frame_address(&mut self, loc: Loc) -> R<Expr<'a>> {
        let args = self.builtin_args(1, loc)?;
        if !Target::NATIVE.contains(&self.target) {
            return Err(Error::new(
                loc,
                "`__builtin_frame_address` requires a native frame-record ABI",
            ));
        }
        // Bound emitted pointer loads without relaxing the shared ICE
        // evaluator's checks for overflow, shifts and side effects.
        let Some(depth) = integer_constant_control(&args[0], self.c_layout)
            .filter(|n| (0..=MAX_FRAME_ADDRESS_DEPTH).contains(n))
        else {
            return Err(Error::new(
                loc,
                "`__builtin_frame_address` requires an integer constant expression in 0..64",
            ));
        };
        Ok(mk(
            ExprKind::Builtin(Builtin::FrameAddress(depth as u8), &[]),
            self.ptr_type(Ty::Void),
            loc,
        ))
    }
}
