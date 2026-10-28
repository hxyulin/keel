#!/usr/bin/env python3
"""Diagnostic LLVM 23 scalar IR importer; see llvm_import_README.md.

Uses LLVM's parser/verifier through its C API, then verifies the buffered Keel
translation before publishing it. This deliberately accepts a small subset.
"""
import argparse
import ctypes as C
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

from llvm_import_globals import Globals
from llvm_import_linkage import local_copies


P, U = C.c_void_p, C.c_uint
ABI_UNSUPPORTED = ('sret', 'byval', 'byref', 'inalloca', 'preallocated',
                   'inreg', 'nest', 'swiftself', 'swifterror', 'swiftasync', 'alignstack')


class Unsupported(ValueError):
    pass


class LLVM:
    # LLVMLinkage from llvm/include/llvm-c/Core.h (not C++ GlobalValue::LinkageTypes).
    # https://github.com/llvm/llvm-project/blob/main/llvm/include/llvm-c/Core.h
    ExternalLinkage = 0
    LinkOnceODRLinkage = 3
    WeakODRLinkage = 6
    InternalLinkage = 8
    PrivateLinkage = 9
    ODR_LINKAGES = (LinkOnceODRLinkage, WeakODRLinkage)
    LOCAL_LINKAGES = (InternalLinkage, PrivateLinkage)

    def __init__(self, path):
        self.lib = C.CDLL(path)
        specs = {
            'GetVersion': (None, [C.POINTER(U)] * 3),
            'ContextCreate': (P, []), 'ContextDispose': (None, [P]),
            'CreateMemoryBufferWithMemoryRangeCopy': (P, [C.c_char_p, C.c_size_t, C.c_char_p]),
            'ParseIRInContext': (C.c_int, [P, P, C.POINTER(P), C.POINTER(P)]),
            'VerifyModule': (C.c_int, [P, C.c_int, C.POINTER(P)]),
            'DisposeMessage': (None, [P]), 'DisposeModule': (None, [P]),
            'GetTarget': (C.c_char_p, [P]), 'GetDataLayoutStr': (C.c_char_p, [P]),
            'GetModuleInlineAsm': (P, [P, C.POINTER(C.c_size_t)]),
            'GetFirstFunction': (P, [P]), 'GetNextFunction': (P, [P]),
            'GetFirstGlobal': (P, [P]), 'GetFirstGlobalAlias': (P, [P]),
            'GetFirstGlobalIFunc': (P, [P]), 'GetValueName2': (P, [P, C.POINTER(C.c_size_t)]),
            'GetNamedMetadata': (P, [P, C.c_char_p, C.c_size_t]),
            'PrintValueToString': (P, [P]), 'GlobalGetValueType': (P, [P]),
            'TypeOf': (P, [P]), 'GetTypeKind': (U, [P]), 'GetIntTypeWidth': (U, [P]),
            'GetTypeContext': (P, [P]), 'IntTypeInContext': (P, [P, U]),
            'IsAConstantFP': (P, [P]), 'IsAInsertValueInst': (P, [P]), 'ConstBitCast': (P, [P, P]),
            'GetFCmpPredicate': (U, [P]), 'GetFastMathFlags': (U, [P]),
            'CanValueUseFastMathFlags': (C.c_int, [P]),
            'GetPointerAddressSpace': (U, [P]), 'GetReturnType': (P, [P]),
            'IsFunctionVarArg': (C.c_int, [P]), 'GetFunctionCallConv': (U, [P]),
            'GetInstructionCallConv': (U, [P]), 'GetLinkage': (U, [P]),
            'GetVisibility': (U, [P]), 'HasPersonalityFn': (C.c_int, [P]),
            'GetPersonalityFn': (P, [P]),
            'GetFirstUse': (P, [P]), 'GetNextUse': (P, [P]), 'GetUser': (P, [P]),
            'GetNumClauses': (U, [P]), 'GetClause': (P, [P, U]),
            'IsCleanup': (C.c_int, [P]),
            'GetNumIndices': (U, [P]), 'GetIndices': (C.POINTER(U), [P]),
            'GetGC': (C.c_char_p, [P]), 'GetSection': (C.c_char_p, [P]),
            'GetComdat': (P, [P]),
            'HasPrefixData': (C.c_int, [P]), 'HasPrologueData': (C.c_int, [P]),
            'GetDLLStorageClass': (U, [P]), 'GetNumOperandBundles': (U, [P]),
            'GetAlignment': (U, [P]), 'CountParams': (U, [P]), 'GetParam': (P, [P, U]),
            'GetEnumAttributeKindForName': (U, [C.c_char_p, C.c_size_t]),
            'GetEnumAttributeAtIndex': (P, [P, U, U]),
            'GetCallSiteEnumAttribute': (P, [P, U, U]),
            'GetAttributeCountAtIndex': (U, [P, U]),
            'GetAttributesAtIndex': (None, [P, U, C.POINTER(P)]),
            'GetCallSiteAttributeCount': (U, [P, U]),
            'GetCallSiteAttributes': (None, [P, U, C.POINTER(P)]),
            'IsStringAttribute': (C.c_int, [P]),
            'GetEnumAttributeKind': (U, [P]), 'GetTypeAttributeValue': (P, [P]),
            'CountParamTypes': (U, [P]), 'GetParamTypes': (None, [P, C.POINTER(P)]),
            'IsPackedStruct': (C.c_int, [P]),
            'GetStringAttributeKind': (P, [P, C.POINTER(U)]),
            'GetFirstBasicBlock': (P, [P]), 'GetNextBasicBlock': (P, [P]),
            'GetFirstInstruction': (P, [P]), 'GetNextInstruction': (P, [P]),
            'GetInstructionOpcode': (U, [P]), 'GetNumOperands': (C.c_int, [P]),
            'GetOperand': (P, [P, U]), 'IsAConstantInt': (P, [P]),
            'IsAConstantPointerNull': (P, [P]), 'IsAFunction': (P, [P]), 'IsAInstruction': (P, [P]),
            'ConstIntGetZExtValue': (C.c_ulonglong, [P]),
            'ConstIntGetSExtValue': (C.c_longlong, [P]),
            'GetICmpPredicate': (U, [P]), 'GetCalledValue': (P, [P]),
            'GetCalledFunctionType': (P, [P]), 'GetTailCallKind': (U, [P]),
            'GetNumArgOperands': (U, [P]), 'GetVolatile': (C.c_int, [P]),
            'GetOrdering': (U, [P]), 'GetNUW': (C.c_int, [P]),
            'GetNSW': (C.c_int, [P]), 'GetExact': (C.c_int, [P]),
            'CreateTargetData': (P, [C.c_char_p]), 'DisposeTargetData': (None, [P]),
            'ByteOrder': (U, [P]), 'PointerSize': (U, [P]),
            'CountIncoming': (U, [P]), 'GetIncomingValue': (P, [P, U]),
            'GetIncomingBlock': (P, [P, U]), 'GetNumSuccessors': (U, [P]),
            'GetSuccessor': (P, [P, U]), 'GetCondition': (P, [P]),
            'GetSwitchCaseValue': (P, [P, U]),
            'GetGEPSourceElementType': (P, [P]), 'GetAllocatedType': (P, [P]),
            'GetElementType': (P, [P]), 'GetArrayLength2': (C.c_uint64, [P]),
            'IsOpaqueStruct': (C.c_int, [P]), 'CountStructElementTypes': (U, [P]),
            'StructGetTypeAtIndex': (P, [P, U]),
            'ABISizeOfType': (C.c_ulonglong, [P, P]),
            'ABIAlignmentOfType': (U, [P, P]),
            'OffsetOfElement': (C.c_ulonglong, [P, P, U]),
        }
        for name, (result, args) in specs.items():
            f = getattr(self.lib, 'LLVM' + name)
            f.restype, f.argtypes = result, args
            setattr(self, name, f)
        version = [U() for _ in range(3)]
        self.GetVersion(*(C.byref(v) for v in version))
        self.version = tuple(v.value for v in version)
        if self.version[0] != 23:
            raise Unsupported(f'validated LLVM major is 23; loaded {self.version}')
        self.attribute_kinds = {}
        for name in ABI_UNSUPPORTED + ('signext', 'zeroext', 'naked', 'returns_twice', 'null_pointer_is_valid', 'strictfp', 'nofpclass'):
            b = name.encode()
            kind = self.GetEnumAttributeKindForName(b, len(b))
            if not kind:
                raise Unsupported(f'loaded LLVM does not recognize required attribute {name}')
            self.attribute_kinds[name] = kind

    def name(self, value):
        size = C.c_size_t()
        p = self.GetValueName2(value, C.byref(size))
        return C.string_at(p, size.value).decode('utf-8')

    def printed(self, value):
        p = self.PrintValueToString(value)
        try:
            return C.string_at(p).decode('utf-8', errors='replace')
        finally:
            self.DisposeMessage(p)

    def attribute(self, value, index, name, call=False):
        kind = self.attribute_kinds[name]
        get = self.GetCallSiteEnumAttribute if call else self.GetEnumAttributeAtIndex
        return bool(get(value, index, kind))

    def check_function_attributes(self, value, call=False):
        index = 0xffffffff
        for name in ('naked', 'returns_twice', 'alignstack', 'null_pointer_is_valid', 'strictfp'):
            if self.attribute(value, index, name, call=call):
                raise Unsupported(f'unsupported function/call attribute {name}')
        count = self.GetCallSiteAttributeCount if call else self.GetAttributeCountAtIndex
        get = self.GetCallSiteAttributes if call else self.GetAttributesAtIndex
        attributes = (P * count(value, index))()
        get(value, index, attributes)
        # Calls carry their own contracts, especially when there is no named
        # declaration to consult. Accept known non-ABI function attributes only.
        call_allowed = ('nounwind', 'willreturn', 'nofree', 'nosync', 'nocallback',
                        'noinline', 'alwaysinline', 'inlinehint', 'cold', 'hot',
                        'memory', 'allocsize', 'allockind', 'speculatable',
                        'mustprogress', 'norecurse', 'nobuiltin', 'builtin', 'noreturn')
        call_kinds = {self.GetEnumAttributeKindForName(n.encode(), len(n)) for n in call_allowed}
        for attribute in attributes:
            if call and (self.IsStringAttribute(attribute)
                         or self.GetEnumAttributeKind(attribute) not in call_kinds):
                raise Unsupported('unsupported call function ABI contract')
            if self.IsStringAttribute(attribute):
                length = U()
                key = self.GetStringAttributeKind(attribute, C.byref(length))
                name = C.string_at(key, length.value).decode()
                # These may alter FP results or the execution environment. Even
                # apparently benign settings are outside this default-IEEE bridge.
                if name.startswith(('denormal-fp-math', 'rounding-mode')) or name in (
                    'unsafe-fp-math', 'no-infs-fp-math', 'no-nans-fp-math',
                    'no-signed-zeros-fp-math', 'approx-func-fp-math',
                ):
                    raise Unsupported(f'unsupported floating-point attribute {name}')
                if name.startswith(('aarch64_', 'instrument-function-', 'patchable-')):
                    raise Unsupported(f'unsupported function/call string attribute {name}')


def sequence(first, next_, owner):
    value = first(owner)
    while value:
        yield value
        value = next_(value)


class Importer:
    ABI_UNSUPPORTED = ABI_UNSUPPORTED
    MEMORY_INTRINSICS = {
        'llvm.memcpy.p0.p0.i64': 'memcpy',
        'llvm.memmove.p0.p0.i64': 'memmove',
        'llvm.memset.p0.i64': 'memset',
    }
    BINARY = {8: 'iadd', 10: 'isub', 12: 'imul', 14: 'udiv', 15: 'sdiv',
              17: 'urem', 18: 'srem', 20: 'shl', 21: 'ushr', 22: 'sshr',
              23: 'and', 24: 'or', 25: 'xor'}
    FP_BINARY = {9: 'fadd', 11: 'fsub', 13: 'fmul', 16: 'fdiv'}
    CMP = {32: 'eq', 33: 'ne', 34: 'ugt', 35: 'uge', 36: 'ult',
           37: 'ule', 38: 'sgt', 39: 'sge', 40: 'slt', 41: 'sle'}

    def __init__(self, llvm, localize_odr=False):
        self.l = llvm
        self.localize_odr = localize_odr
        self.local_copies = set()

    def symbol(self, value):
        name = self.l.name(value)
        if not re.fullmatch(r'[A-Za-z0-9_.$]+', name):
            raise Unsupported(f'unsupported symbol spelling {name!r}')
        return '@' + name

    def type(self, ty, abi=False):
        kind = self.l.GetTypeKind(ty)
        if kind == 0:
            return 'void'
        if kind in (2, 3):
            return {2: 'f32', 3: 'f64'}[kind]
        if kind == 8:
            width = self.l.GetIntTypeWidth(ty)
            if width in (1, 8, 16, 32, 64):
                return 'i8' if width == 1 else f'i{width}'
        if kind == 12 and self.l.GetPointerAddressSpace(ty) == 0:
            return 'ptr'
        raise Unsupported('supported types are scalar i1/i8/i16/i32/i64/f32/f64 and address-space-zero pointers')

    def width(self, value):
        ty = self.l.TypeOf(value)
        return self.l.GetIntTypeWidth(ty) if self.l.GetTypeKind(ty) == 8 else 0

    def signature(self, fn):
        l = self.l
        if l.GetFunctionCallConv(fn) != 0 and fn not in self.fast_functions:
            raise Unsupported(f'{self.symbol(fn)}: non-C calling convention')
        fty = l.GlobalGetValueType(fn)
        if fn in self.personalities:
            # The Itanium personality is a symbolic runtime reference, never a
            # Keel call. Its int (...) declaration describes no ordinary ABI.
            if (l.name(fn) != '__gxx_personality_v0' or l.GetFirstBasicBlock(fn)
                    or not l.IsFunctionVarArg(fty) or l.CountParamTypes(fty)
                    or self.type(l.GetReturnType(fty)) != 'i32'
                    or l.GetLinkage(fn) != 0 or l.GetVisibility(fn) != 0):
                raise Unsupported('unsupported personality declaration')
        elif l.IsFunctionVarArg(fty):
            raise Unsupported(f'{self.symbol(fn)}: variadic signature')
        linkage = l.GetLinkage(fn)
        defined_odr = bool(l.GetFirstBasicBlock(fn)) and linkage in l.ODR_LINKAGES
        if ((linkage not in (l.ExternalLinkage, *l.LOCAL_LINKAGES)
                and not defined_odr and fn not in self.local_copies)
                or l.GetVisibility(fn) != 0):
            raise Unsupported(f'{self.symbol(fn)}: unsupported linkage/visibility')
        if l.GetAlignment(fn) > 4:
            raise Unsupported(f'{self.symbol(fn)}: explicit function alignment')
        if l.GetComdat(fn):
            raise Unsupported(f'{self.symbol(fn)}: COMDAT membership')
        if l.GetGC(fn) or l.GetSection(fn) or l.HasPrefixData(fn) or l.HasPrologueData(fn) or l.GetDLLStorageClass(fn):
            raise Unsupported(f'{self.symbol(fn)}: GC, section, prefix/prologue data or DLL storage')
        l.check_function_attributes(fn)
        if fn in self.personalities:
            return 'void', []
        return self.abi_signature(fn, fty, definition=bool(l.GetFirstBasicBlock(fn)),
                                  internal_fast=fn in self.fast_functions,
                                  memory_intrinsic=l.name(fn) in self.MEMORY_INTRINSICS)

    def integer_pair_type(self, ty):
        """Only the LLVM-lowered Apple arm64 two-word integer aggregate."""
        l = self.l
        return (l.GetTypeKind(ty) == 11 and l.GetArrayLength2(ty) == 2
                and l.GetTypeKind(l.GetElementType(ty)) == 8
                and l.GetIntTypeWidth(l.GetElementType(ty)) == 64
                and l.ABISizeOfType(self.td, ty) == 16
                and l.ABIAlignmentOfType(self.td, ty) == 8)

    def aggregate_component(self, value, index):
        # Resolve insert chains structurally, even when LLVM block order is not
        # dominance order. An overwritten poison/undef lane is never evaluated;
        # every observed lane must ultimately be a supported, defined scalar.
        l = self.l
        if not self.integer_pair_type(l.TypeOf(value)) or index not in (0, 1):
            raise Unsupported('aggregate component outside exact two-i64 contract')
        while l.IsAInsertValueInst(value):
            if l.GetNumIndices(value) != 1 or l.GetIndices(value)[0] > 1:
                raise Unsupported('insertvalue outside exact two-i64 contract')
            if l.GetIndices(value)[0] == index:
                return self.value(l.GetOperand(value, 1))
            value = l.GetOperand(value, 0)
        if value in self.aggregate_params:
            return self.aggregate_params[value][index]
        raise Unsupported('unsupported aggregate SSA: requires parameter or insertvalue with defined observed lanes')

    def abi_signature(self, value, fty, call=False, definition=False, memory_intrinsic=False, internal_fast=False):
        """Use LLVM's function type and explicit ABI contract, never infer C types."""
        l = self.l
        if l.IsFunctionVarArg(fty):
            raise Unsupported('variadic signature')
        params = (P * l.CountParamTypes(fty))()
        l.GetParamTypes(fty, params)
        types = [l.GetReturnType(fty), *params]
        if memory_intrinsic:
            name = l.name(l.GetCalledValue(value) if call else value)
            expected = ['void', 'ptr', 'i8' if self.MEMORY_INTRINSICS[name] == 'memset' else 'ptr', 'i64', 'i8']
            if (definition or len(types) != 5
                    or [self.type(t) for t in types] != expected
                    or l.GetTypeKind(types[4]) != 8 or l.GetIntTypeWidth(types[4]) != 1):
                raise Unsupported('unsupported memory intrinsic signature')
        # These attributes constrain valid values/memory but do not change the
        # machine calling convention. All other parameter/return contracts fail
        # closed, including new LLVM ABI attributes such as noext.
        allowed = ('signext', 'zeroext', 'sret', 'noundef', 'nonnull', 'noalias',
                   'nocapture', 'captures', 'nofree', 'readonly', 'readnone', 'writeonly',
                   'dereferenceable', 'dereferenceable_or_null', 'align', 'range',
                   'dead_on_unwind', 'writable', 'initializes', 'returned')
        kinds = {l.GetEnumAttributeKindForName(n.encode(), len(n)) for n in allowed}
        returned_kind = l.GetEnumAttributeKindForName(b'returned', len(b'returned'))
        count = l.GetCallSiteAttributeCount if call else l.GetAttributeCountAtIndex
        get = l.GetCallSiteAttributes if call else l.GetAttributesAtIndex
        result = []
        for index, ty in enumerate(types):
            if l.attribute(value, index, 'nofpclass', call=call):
                raise Unsupported('unsupported nofpclass promise')
            attrs = (P * count(value, index))()
            get(value, index, attrs)
            for attr in attrs:
                if (memory_intrinsic and index == 4 and not l.IsStringAttribute(attr)
                        and l.GetEnumAttributeKind(attr) == l.GetEnumAttributeKindForName(b'immarg', 6)):
                    continue
                if l.IsStringAttribute(attr) or l.GetEnumAttributeKind(attr) not in kinds:
                    raise Unsupported('unsupported parameter/return ABI attribute')
                # Omit LLVM's returned promise without creating Keel return or
                # alias facts. Accept only parameters with exactly the result
                # type; LLVM's verifier checks the rest of the contract.
                if l.GetEnumAttributeKind(attr) == returned_kind and (index == 0 or ty != types[0]):
                    raise Unsupported('returned requires a parameter with the exact result type')
            if l.GetTypeKind(ty) in (10, 11):
                if index == 0 or not self.integer_pair_type(ty):
                    raise Unsupported('aggregate ABI supports only exact [2 x i64] parameters; aggregate returns unsupported')
                text = 'ptr byval {i64, i64}'
            else:
                text = self.type(ty, abi=True)
            sret = l.attribute(value, index, 'sret', call=call)
            if sret:
                # LLVM already lowered this result to x8. Keel reclassifies sret
                # layouts, so accept only the proven 24-byte integer triple.
                # In particular, small structs and HFAs must NOT be copied here.
                if index != 1 or text != 'ptr' or l.GetTypeKind(types[0]) != 0:
                    raise Unsupported('sret requires first pointer parameter and void result')
                getter = l.GetCallSiteEnumAttribute if call else l.GetEnumAttributeAtIndex
                layout = l.GetTypeAttributeValue(getter(value, index, l.attribute_kinds['sret']))
                if (l.GetTypeKind(layout) != 10 or l.IsOpaqueStruct(layout)
                        or l.IsPackedStruct(layout) or l.CountStructElementTypes(layout) != 3
                        or any(l.GetTypeKind(l.StructGetTypeAtIndex(layout, i)) != 8
                               or l.GetIntTypeWidth(l.StructGetTypeAtIndex(layout, i)) != 64
                               for i in range(3))
                        or l.ABISizeOfType(self.td, layout) != 24
                        or l.ABIAlignmentOfType(self.td, layout) != 8
                        or any(l.OffsetOfElement(self.td, layout, i) != i * 8 for i in range(3))):
                    raise Unsupported('sret layout outside proven unpacked three-i64 contract')
                text += ' sret {i64, i64, i64}'
            extensions = [a for a in ('signext', 'zeroext') if l.attribute(value, index, a, call=call)]
            if internal_fast and l.GetTypeKind(ty) == 8 and l.GetIntTypeWidth(ty) == 1 and not extensions:
                # Keel represents i1 by a canonical i8; no outside ABI observes
                # these private edges. Choose our own zero extension contract.
                extensions = ['zeroext']
            if extensions and (sret or l.GetTypeKind(ty) != 8):
                raise Unsupported('extension requires integer ABI type')
            if len(extensions) > 1:
                raise Unsupported('conflicting ABI extensions')
            # A zero-extended i1 is physically the same canonical 0/1 value
            # as Keel i8 uext on the supported Apple AArch64 target. Sign
            # extension is NOT equivalent: true would be -1 rather than 1.
            # Unspecified extension remains outside the proven contract.
            if (l.GetTypeKind(ty) == 8 and l.GetIntTypeWidth(ty) == 1
                    and extensions != ['zeroext'] and not (memory_intrinsic and index == 4)):
                raise Unsupported('i1 ABI parameters/returns require explicit zeroext')
            if index == 0 and text in ('i8', 'i16') and (definition or call) and not extensions and not internal_fast:
                raise Unsupported('i8/i16 return definitions/calls require signext or zeroext')
            for attr in extensions:
                text += ' ' + {'signext': 'sext', 'zeroext': 'uext'}[attr]
            result.append(text)
        return result[0], result[1:]

    def emit(self, expr):
        value = f'v{self.next_value}'
        self.next_value += 1
        self.lines.append(f'    {value} = {expr}')
        return value

    def value(self, value):
        if value in self.values:
            return self.values[value]
        l = self.l
        if l.IsAConstantFP(value):
            ty = l.TypeOf(value)
            text = self.type(ty)
            width = {'f32': 32, 'f64': 64}[text]
            # Bitcast inside LLVM, rather than round-tripping through a host
            # double (which can quiet sNaNs and lose f32 NaN payload bits).
            integer = l.IntTypeInContext(l.GetTypeContext(ty), width)
            bits = l.ConstIntGetZExtValue(l.ConstBitCast(value, integer))
            return self.emit(f'fconst.{text} 0x{bits:0{width // 4}x}')
        if l.IsAConstantInt(value):
            ty = self.type(l.TypeOf(value))
            n = l.ConstIntGetZExtValue(value) if self.width(value) == 1 else l.ConstIntGetSExtValue(value)
            return self.emit(f'iconst.{ty} {n}')
        if value in self.personalities:
            raise Unsupported('personality may only be a symbolic EH reference')
        if l.IsAFunction(value):
            return self.emit(f'symbol_addr {self.symbol(value)}')
        if l.IsAConstantPointerNull(value):
            zero = self.emit('iconst.i64 0')
            return self.emit(f'inttoptr.ptr {zero}')
        symbolic = self.globals.operand(value)
        if symbolic is not None:
            return symbolic
        raise Unsupported('unsupported operand (including undef, poison or constant expressions): ' + l.printed(value)[:160])

    def layout_type(self, ty):
        """Validate before asking LLVM for a size (unsized types can abort LLVM)."""
        l = self.l
        kind = l.GetTypeKind(ty)
        if kind in (2, 3, 8, 12):
            self.type(ty)
        elif kind == 11:
            self.layout_type(l.GetElementType(ty))
        elif kind == 10 and not l.IsOpaqueStruct(ty):
            for i in range(l.CountStructElementTypes(ty)):
                self.layout_type(l.StructGetTypeAtIndex(ty, i))
        else:
            raise Unsupported('unsupported/unsized memory layout type')

    def edge(self, target):
        args = []
        for phi in self.phis[target]:
            incoming = [self.l.GetIncomingValue(phi, i)
                        for i in range(self.l.CountIncoming(phi))
                        if self.l.GetIncomingBlock(phi, i) == self.block]
            # LLVM verifies one equal incoming value per duplicate predecessor edge.
            if not incoming or any(v != incoming[0] for v in incoming):
                raise Unsupported('inconsistent PHI incoming values for predecessor')
            if phi in self.pairs:
                if incoming[0] not in self.pairs:
                    raise Unsupported('landingpad PHI requires only landingpad pairs')
                args.extend(self.pairs[incoming[0]])
            else:
                args.append(self.value(incoming[0]))
        return self.blocks[target] + ('(' + ', '.join(args) + ')' if args else '')

    def landing_pair_type(self, ty):
        l = self.l
        return (l.GetTypeKind(ty) == 10 and not l.IsOpaqueStruct(ty)
                and not l.IsPackedStruct(ty) and l.CountStructElementTypes(ty) == 2
                and self.type(l.StructGetTypeAtIndex(ty, 0)) == 'ptr'
                and self.type(l.StructGetTypeAtIndex(ty, 1)) == 'i32')

    def fresh_pair(self):
        pair = (f'v{self.next_value}', f'v{self.next_value + 1}')
        self.next_value += 2
        return pair

    def clauses(self, inst):
        l = self.l
        clauses = []
        for n in range(l.GetNumClauses(inst)):
            clause = l.GetClause(inst, n)
            if l.GetTypeKind(l.TypeOf(clause)) != 12:
                raise Unsupported('landingpad filters are unsupported')
            if l.IsAConstantPointerNull(clause):
                clauses.append('catch null')
            elif clause in self.globals.objects:
                # All global definitions have already passed exact layout,
                # initializer, relocation and linkage transport. Typeinfo is
                # runtime data; do not synthesize or reinterpret its structure.
                clauses.append('catch ' + self.symbol(clause))
            else:
                raise Unsupported('catch requires a named global typeinfo or null')
        if l.IsCleanup(inst):
            clauses.append('cleanup')
        if not clauses:
            raise Unsupported('empty landingpad')
        return ', '.join(clauses)

    def instruction(self, inst):
        l = self.l
        op = l.GetInstructionOpcode(inst)
        operands = lambda: [l.GetOperand(inst, i) for i in range(l.GetNumOperands(inst))]
        if l.CanValueUseFastMathFlags(inst) and l.GetFastMathFlags(inst):
            raise Unsupported('floating-point fast-math flags')
        if op in self.FP_BINARY:
            a, b = map(self.value, operands())
            return self.emit(f'{self.FP_BINARY[op]} {a}, {b}')
        if op == 66:  # fneg is an exact sign-bit operation, including NaNs.
            return self.emit('fneg ' + self.value(l.GetOperand(inst, 0)))
        if op == 43:
            a, b = map(self.value, operands())
            predicate = l.GetFCmpPredicate(inst)
            if predicate in (0, 15):
                return self.emit(f'iconst.i8 {int(predicate == 15)}')
            direct = {1: 'eq', 2: 'gt', 3: 'ge', 4: 'lt', 5: 'le',
                      7: 'ord', 8: 'uno', 14: 'ne'}
            if predicate in direct:
                return self.emit(f'fcmp.{direct[predicate]} {a}, {b}')
            if predicate == 6:  # one = ordered AND unequal
                ne = self.emit(f'fcmp.ne {a}, {b}')
                ordered = self.emit(f'fcmp.ord {a}, {b}')
                return self.emit(f'and {ne}, {ordered}')
            cc = {9: 'eq', 10: 'gt', 11: 'ge', 12: 'lt', 13: 'le'}[predicate]
            comparison = self.emit(f'fcmp.{cc} {a}, {b}')
            unordered = self.emit(f'fcmp.uno {a}, {b}')
            return self.emit(f'or {comparison}, {unordered}')
        if op in (33, 34, 35, 36, 37, 38):
            arg = l.GetOperand(inst, 0)
            ty = self.type(l.TypeOf(inst))
            if re.search(r'\b(?:nneg|round\.[a-z]+)\b', l.printed(inst)):
                raise Unsupported('unsupported floating-point conversion flags/rounding')
            if op in (33, 34) and ty not in ('i32', 'i64'):
                raise Unsupported('FP-to-integer destinations must be i32/i64')
            # LLVM defines FP->int only when truncation toward zero fits the
            # destination. Keel saturation agrees on that domain; its results
            # for LLVM poison inputs are a refinement, not a saturation promise
            # to LLVM callers. Do not differential-test those inputs.
            value = self.value(arg)
            if op in (35, 36) and self.width(arg) == 1:
                if op == 36:
                    value = self.emit(f'ineg {value}')
            name = {33: 'fptoui', 34: 'fptosi', 35: 'uitofp',
                    36: 'sitofp', 37: 'fptrunc', 38: 'fpext'}[op]
            return self.emit(f'{name}.{ty} {value}')
        if op in (70, 71):  # LLVM 23 split unconditional/conditional branches.
            if l.GetNumSuccessors(inst) == 1:
                text = 'jump ' + self.edge(l.GetSuccessor(inst, 0))
            else:
                cond = self.value(l.GetCondition(inst))
                yes, no = (self.edge(l.GetSuccessor(inst, i)) for i in range(2))
                text = f'brif {cond}, {yes}, {no}'
            self.lines.append('    ' + text)
            return None
        if op == 3:
            cond = self.value(l.GetOperand(inst, 0))
            default = self.edge(l.GetSuccessor(inst, 0))
            cases = []
            for i in range(1, l.GetNumSuccessors(inst)):
                value = l.GetSwitchCaseValue(inst, i)
                n = l.ConstIntGetZExtValue(value) if self.width(value) == 1 else l.ConstIntGetSExtValue(value)
                cases.append(f'{n}: {self.edge(l.GetSuccessor(inst, i))}')
            self.lines.append(f'    switch {cond}, {default}, [' + ', '.join(cases) + ']')
            return None
        if op == 26:
            return self.emit('stack_addr ' + self.slots[inst])
        if op == 29:
            # inbounds/no-wrap/inrange constrain LLVM-defined executions; they
            # do not change the computed address on that domain. Keel keeps the
            # exact GEP arithmetic, without adding poison/escape/alias facts.
            ty = l.GetGEPSourceElementType(inst)
            self.layout_type(ty)
            addr = self.value(l.GetOperand(inst, 0))
            for i in range(1, l.GetNumOperands(inst)):
                index = l.GetOperand(inst, i)
                if i > 1 and l.GetTypeKind(ty) == 10:
                    if not l.IsAConstantInt(index):
                        raise Unsupported('nonconstant struct GEP index')
                    field = l.ConstIntGetZExtValue(index)
                    offset = self.emit(f'iconst.i64 {l.OffsetOfElement(self.td, ty, field):#x}')
                    ty = l.StructGetTypeAtIndex(ty, field)
                else:
                    if i > 1:
                        if l.GetTypeKind(ty) != 11:
                            raise Unsupported('GEP indexes unsupported layout')
                        ty = l.GetElementType(ty)
                    width = self.width(index)
                    if width not in (8, 16, 32, 64):
                        raise Unsupported('GEP index must be i8/i16/i32/i64')
                    offset = self.value(index)
                    if width < 64:
                        offset = self.emit(f'sext.i64 {offset}')
                    stride = self.emit(f'iconst.i64 {l.ABISizeOfType(self.td, ty):#x}')
                    offset = self.emit(f'imul {offset}, {stride}')
                addr = self.emit(f'ptradd {addr}, {offset}')
            return addr
        if op in self.BINARY:
            # nsw/nuw, disjoint and exact only add poison outcomes. Ordinary
            # wrapping/bitwise/shift/division results refine those outcomes;
            # on non-poison inputs the operations agree. Do not infer these
            # flags as Keel optimizer facts. Explicit poison/undef/freeze still
            # fail closed. See llvm_import_flags_validate.py for the domain.
            if self.width(inst) == 1 and op not in (23, 24, 25):
                raise Unsupported('i1 arithmetic other than boolean bitwise operations')
            a, b = map(self.value, operands())
            if op == 15:
                # LLVM sdiv MIN/-1 is immediate UB for concrete operands, but
                # a poison dividend can flow through sdiv to an unselected
                # select arm on a defined execution. Refining that dividend
                # to MIN must not introduce Keel's overflow trap. Totalize
                # just the overflow pair; a zero (or poison) divisor remains
                # immediate LLVM UB, so retain Keel's zero-divisor trap.
                ty = self.type(l.TypeOf(inst))
                minimum = self.emit(f'iconst.{ty} {-(1 << (self.width(inst) - 1))}')
                minus_one = self.emit(f'iconst.{ty} -1')
                is_min = self.emit(f'icmp.eq {a}, {minimum}')
                is_minus_one = self.emit(f'icmp.eq {b}, {minus_one}')
                overflow = self.emit(f'and {is_min}, {is_minus_one}')
                one = self.emit(f'iconst.{ty} 1')
                b = self.emit(f'select {overflow}, {one}, {b}')
            # Keel srem MIN/-1 is already nontrapping (zero), which also
            # safely refines a poison dividend; no extra lowering is needed.
            return self.emit(f'{self.BINARY[op]} {a}, {b}')
        if op in (30, 31, 32, 39, 40, 41):
            arg = l.GetOperand(inst, 0)
            value = self.value(arg)
            ty = self.type(l.TypeOf(inst))
            # trunc nuw/nsw and zext nneg likewise refine to ordinary casts.
            if op == 41:
                source_ty = self.type(l.TypeOf(arg))
                if source_ty == ty:
                    return value
                if (source_ty, ty) in (('f32', 'i32'), ('i32', 'f32'),
                                       ('f64', 'i64'), ('i64', 'f64')):
                    return self.emit(f'bitcast.{ty} {value}')
                raise Unsupported('unsupported scalar bitcast')
            if op == 30 and self.width(inst) == 1:
                if self.width(arg) > 8:
                    value = self.emit(f'trunc.i8 {value}')
                one = self.emit('iconst.i8 1')
                return self.emit(f'and {value}, {one}')
            if op == 32 and self.width(arg) == 1:
                value = self.emit(f'ineg {value}')
                return value if ty == 'i8' else self.emit(f'sext.{ty} {value}')
            if op == 31 and self.width(arg) == 1 and ty == 'i8':
                return value
            name = {30: 'trunc', 31: 'uext', 32: 'sext', 39: 'ptrtoint', 40: 'inttoptr'}[op]
            if op == 39 and ty != 'i64':
                wide = self.emit(f'ptrtoint.i64 {value}')
                narrow = self.emit(f'trunc.{ty} {wide}')
                if self.width(inst) == 1:
                    one = self.emit('iconst.i8 1')
                    narrow = self.emit(f'and {narrow}, {one}')
                return narrow
            if op == 40 and self.width(arg) < 64:
                value = self.emit(f'uext.i64 {value}')
            return self.emit(f'{name}.{ty} {value}')
        if op == 42:
            # samesign adds poison for opposite signs; ordinary icmp refines it.
            args = operands()
            if not self.width(args[0]) and re.search(r'\bicmp samesign\b', l.printed(inst)):
                raise Unsupported('samesign comparison requires scalar integers')
            cc = self.CMP[l.GetICmpPredicate(inst)]
            if self.width(args[0]) == 1 and cc not in ('eq', 'ne'):
                raise Unsupported('ordered i1 comparison')
            a, b = map(self.value, args)
            return self.emit(f'icmp.{cc} {a}, {b}')
        if op == 46:
            cond, a, b = map(self.value, operands())
            return self.emit(f'select {cond}, {a}, {b}')
        if op in (27, 28):
            if l.GetOrdering(inst) != 0:
                raise Unsupported('atomic memory access')
            flags = ' volatile' if l.GetVolatile(inst) else ''
            if op == 27:
                ty = self.type(l.TypeOf(inst))
                addr = self.value(l.GetOperand(inst, 0))
                v = self.emit(f'load.{ty}{flags} {addr}')
                if self.width(inst) == 1:
                    one = self.emit('iconst.i8 1')
                    v = self.emit(f'and {v}, {one}')
                return v
            a, addr = map(self.value, operands())
            self.lines.append(f'    store{flags} {a}, {addr}')
            return None
        if op == 59:  # landingpad: a dedicated pair, never general aggregate SSA.
            if self.block not in self.split_pads:
                self.lines.append('    ' + ', '.join(self.pairs[inst]) + ' = landingpad ' + self.clauses(inst))
            return None
        if op == 54:  # insertvalue, resolved per observed lane (no memory alias).
            if not self.integer_pair_type(l.TypeOf(inst)):
                raise Unsupported('insertvalue outside exact two-i64 contract')
            return None
        if op == 53:  # extractvalue
            source = l.GetOperand(inst, 0)
            if self.integer_pair_type(l.TypeOf(source)):
                if l.GetNumIndices(inst) != 1:
                    raise Unsupported('extractvalue outside exact two-i64 contract')
                return self.aggregate_component(source, l.GetIndices(inst)[0])
            if source not in self.pairs or l.GetNumIndices(inst) != 1:
                raise Unsupported('extractvalue requires a landingpad pair or acyclic pair PHI')
            index = l.GetIndices(inst)[0]
            if index > 1:
                raise Unsupported('landingpad pair index')
            return self.pairs[source][index]
        if op == 58:  # resume
            pair = self.pairs.get(l.GetOperand(inst, 0))
            if pair is None:
                raise Unsupported('resume requires a landingpad pair or acyclic pair PHI')
            self.needs_resume = True
            self.lines += [f'    call @_Unwind_Resume({pair[0]})', '    trap']
            return None
        if op in (45, 5):  # call / direct invoke
            l.check_function_attributes(inst, call=True)
            callee = l.GetCalledValue(inst)
            fast = callee in self.fast_functions
            if l.GetInstructionCallConv(inst) != (8 if fast else 0):
                raise Unsupported('non-C call or fastcc contract mismatch')
            if l.GetNumOperandBundles(inst):
                raise Unsupported('call operand bundles')
            if op == 45 and l.GetTailCallKind(inst) == 2:  # LLVMTailCallKindMustTail
                raise Unsupported('musttail call')
            if callee in self.personalities:
                raise Unsupported('personality may only be a symbolic EH reference')
            fty = l.GetCalledFunctionType(inst)
            memory = l.IsAFunction(callee) and l.name(callee) in self.MEMORY_INTRINSICS
            ret, params = self.abi_signature(inst, fty, call=True, memory_intrinsic=memory, internal_fast=fast)
            if memory:
                if (op != 45 or fty != l.GlobalGetValueType(callee)
                        or (ret, params) != self.signatures[callee]
                        or l.GetNumArgOperands(inst) != 4):
                    raise Unsupported('unsupported memory intrinsic call')
                volatile = l.GetOperand(inst, 3)
                if not l.IsAConstantInt(volatile) or l.ConstIntGetZExtValue(volatile):
                    raise Unsupported('volatile memory intrinsics are unsupported')
                args = ', '.join(self.value(l.GetOperand(inst, i)) for i in range(3))
                operation = self.MEMORY_INTRINSICS[l.name(callee)]
                # LLVM memcpy permits identical pointers; Keel memcpy requires
                # nonoverlapping regions. Memmove preserves that extra defined
                # case without inventing disjointness facts.
                if operation == 'memcpy':
                    operation = 'memmove'
                self.lines.append(f'    intrinsic {operation}({args})')
                return None
            if l.IsAFunction(callee) and l.name(callee) in ('llvm.lifetime.start.p0', 'llvm.lifetime.end.p0'):
                # LLVM 23 canonicalizes the older size+pointer form to a single
                # pointer. Keep fixed slots alive for the whole function: this
                # agrees on initialized, live memory and refines poison loads
                # outside a lifetime. No lifetime or alias facts reach Keel.
                if (op != 45 or fty != l.GlobalGetValueType(callee)
                        or (ret, params) != ('void', ['ptr'])
                        or self.signatures[callee] != ('void', ['ptr'])
                        or l.GetNumArgOperands(inst) != 1
                        or l.GetOperand(inst, 0) not in self.slots):
                    raise Unsupported('lifetime marker requires a direct fixed entry-block stack slot')
                return None
            if l.IsAFunction(callee) and l.name(callee) in ('llvm.eh.typeid.for', 'llvm.eh.typeid.for.p0'):
                if (op != 45 or l.GetNumArgOperands(inst) != 1
                        or fty != l.GlobalGetValueType(callee)
                        or (ret, params) != ('i32', ['ptr'])
                        or self.signatures[callee] != ('i32', ['ptr'])):
                    raise Unsupported('unsupported eh.typeid intrinsic signature')
                typeinfo = l.GetOperand(inst, 0)
                if typeinfo not in self.globals.objects:
                    raise Unsupported('eh.typeid requires a named caught typeinfo')
                return self.emit('eh_typeid ' + self.symbol(typeinfo))
            if l.GetNumArgOperands(inst) != len(params):
                raise Unsupported('call argument count differs from signature')
            direct = bool(l.IsAFunction(callee))
            if direct:
                if fty != l.GlobalGetValueType(callee):
                    raise Unsupported('call type differs from declaration')
                if l.name(callee).startswith('llvm.'):
                    raise Unsupported('LLVM intrinsic ' + l.name(callee))
                if (ret, params) != self.signatures[callee]:
                    raise Unsupported('call-site ABI differs from declaration')
            if op == 5 and not direct:
                raise Unsupported('indirect invoke is unsupported')
            arguments = []
            for i in range(len(params)):
                arg = l.GetOperand(inst, i)
                if self.integer_pair_type(l.TypeOf(arg)):
                    # One explicit byval argument retains register exhaustion,
                    # stack alignment and copy semantics in Keel's ABI planner.
                    lanes = [self.aggregate_component(arg, n) for n in range(2)]
                    address = self.emit('stack_addr ' + self.aggregate_slots[inst, i])
                    self.lines.append(f'    store {lanes[0]}, {address}')
                    offset = self.emit('iconst.i64 8')
                    high = self.emit(f'ptradd {address}, {offset}')
                    self.lines.append(f'    store {lanes[1]}, {high}')
                    arguments.append(address)
                else:
                    arguments.append(self.value(arg))
            args = ', '.join(arguments)
            if direct:
                expr = f'call {self.symbol(callee)}({args})'
                if op == 5:
                    normal, unwind = (l.GetSuccessor(inst, i) for i in range(2))
                    bridge = f'block{len(self.blocks) + len(self.invoke_edges)}'
                    self.invoke_edges.append((bridge, self.block, normal, None))
                    if unwind in self.split_pads:
                        pad_bridge = f'block{len(self.blocks) + len(self.invoke_edges)}'
                        self.invoke_edges.append((pad_bridge, self.block, unwind, self.pad_insts[unwind]))
                    else:
                        pad_bridge = self.blocks[unwind]
                    expr = f'invoke {self.symbol(callee)}({args}), {bridge}, {pad_bridge}'
            else:
                pointer = self.value(callee)
                signature = '(' + ', '.join(params) + ')'
                if ret != 'void':
                    signature += ' -> ' + ret
                expr = f'call_indirect {pointer}({args}) : {signature} cc(c)'
            if ret == 'void':
                self.lines.append('    ' + expr)
                return None
            return self.emit(expr)
        if op == 1:
            args = operands()
            self.lines.append('    return' + (' ' + self.value(args[0]) if args else ''))
            return None
        if op == 7:
            self.lines.append('    trap')
            return None
        raise Unsupported(f'unsupported instruction opcode {op}: ' + l.printed(inst).strip()[:180])

    def translate(self, module):
        # The target data lifetime covers all GEP/alloca layout queries.
        self.td = self.l.CreateTargetData(self.l.GetDataLayoutStr(module))
        try:
            return self.translate_module(module)
        finally:
            self.l.DisposeTargetData(self.td)

    def translate_module(self, module):
        l = self.l
        target = l.GetTarget(module).decode()
        if not re.fullmatch(r'(?:arm64|aarch64)-apple-(?:darwin|macosx)[0-9.]*', target):
            raise Unsupported(f'only Apple AArch64 modules supported; input target {target!r}')
        layout = l.GetDataLayoutStr(module)
        td = l.CreateTargetData(layout)
        try:
            if not layout or l.ByteOrder(td) != 1 or l.PointerSize(td) != 8:
                raise Unsupported('requires explicit little-endian 64-bit pointer data layout')
        finally:
            l.DisposeTargetData(td)
        # Reject alternate address-space-zero pointer/index representations.
        for component in layout.decode().split('-'):
            if re.match(r'p(?:0)?:', component) and not re.fullmatch(r'p(?:0)?:64:64(?::64(?::64)?)?', component):
                raise Unsupported('unsupported address-space-zero pointer/index layout')
        size = C.c_size_t()
        l.GetModuleInlineAsm(module, C.byref(size))
        if size.value:
            raise Unsupported('module inline assembly')
        if l.GetFirstGlobalAlias(module) or l.GetFirstGlobalIFunc(module):
            raise Unsupported('aliases and ifuncs are unsupported')
        if l.GetNamedMetadata(module, b'llvm.linker.options', len(b'llvm.linker.options')):
            raise Unsupported('module autolink requests (llvm.linker.options)')
        functions = list(sequence(l.GetFirstFunction, l.GetNextFunction, module))
        self.fast_functions = set()
        for fn in functions:
            if l.GetFunctionCallConv(fn) != 8:  # LLVMFastCallConv, not a machine ABI mapping.
                continue
            if (not l.GetFirstBasicBlock(fn) or l.GetLinkage(fn) not in l.LOCAL_LINKAGES
                    or l.IsFunctionVarArg(l.GlobalGetValueType(fn))):
                raise Unsupported('fastcc requires a private fixed-arity definition')
            for use in sequence(l.GetFirstUse, l.GetNextUse, fn):
                user = l.GetUser(use)
                if (not l.IsAInstruction(user) or l.GetInstructionOpcode(user) not in (45, 5)
                        or l.GetCalledValue(user) != fn
                        or l.GetCalledFunctionType(user) != l.GlobalGetValueType(fn)
                        or l.GetInstructionCallConv(user) != 8):
                    raise Unsupported('fastcc requires contained direct calls with matching contracts')
                # The function may be both the called operand and an escaping
                # argument of the same call. Checking only GetCalledValue is insufficient.
                if any(l.GetOperand(user, i) == fn for i in range(l.GetNumArgOperands(user))):
                    raise Unsupported('fastcc function address escapes through a call argument')
            self.fast_functions.add(fn)
        self.personalities = set()
        for fn in functions:
            if l.HasPersonalityFn(fn):
                personality = l.GetPersonalityFn(fn)
                if not l.IsAFunction(personality):
                    raise Unsupported('personality must be a direct runtime symbol')
                self.personalities.add(personality)
        if self.localize_odr:
            self.local_copies = local_copies(self, module, functions, Unsupported)
        self.signatures = {}
        for fn in functions:
            try:
                self.signatures[fn] = self.signature(fn)
            except Unsupported as e:
                raise Unsupported(f'{self.symbol(fn)}: {e}') from e
        self.needs_resume = False
        self.globals = Globals(self, module, functions, Unsupported)
        for personality in self.personalities:
            for use in sequence(l.GetFirstUse, l.GetNextUse, personality):
                user = l.GetUser(use)
                if not (l.IsAFunction(user) and l.HasPersonalityFn(user)
                        and l.GetPersonalityFn(user) == personality):
                    raise Unsupported('personality may only be a symbolic EH reference')
        output = self.globals.definitions()
        for fn in functions:
            if l.name(fn) in self.MEMORY_INTRINSICS:
                # Intrinsics have no ordinary machine ABI or addressable symbol.
                for use in sequence(l.GetFirstUse, l.GetNextUse, fn):
                    user = l.GetUser(use)
                    if (not l.IsAInstruction(user) or l.GetInstructionOpcode(user) != 45
                            or l.GetCalledValue(user) != fn):
                        raise Unsupported('memory intrinsic requires a direct call')
                continue
            ret, params = self.signatures[fn]
            name = self.symbol(fn)
            signature = name + '(' + ', '.join(params) + ')'
            if ret != 'void':
                signature += ' -> ' + ret
            blocks = list(sequence(l.GetFirstBasicBlock, l.GetNextBasicBlock, fn))
            if not blocks:
                output.append('decl ' + signature + ' cc(c)')
                continue
            local = ' local' if l.GetLinkage(fn) in l.LOCAL_LINKAGES or fn in self.local_copies else ''
            if not local and l.GetLinkage(fn) in l.ODR_LINKAGES:
                local = ' weak_odr'
            personality = (' personality ' + self.symbol(l.GetPersonalityFn(fn))
                           if l.HasPersonalityFn(fn) else '')
            self.lines = ['func ' + signature + (' cc(fast)' if fn in self.fast_functions else ' cc(c)') + local + personality + ' {']
            self.values = {l.GetParam(fn, i): f'v{i}' for i in range(l.CountParams(fn))}
            self.next_value = len(params)
            self.aggregate_params = {}
            self.aggregate_slots = {}
            self.blocks = {block: f'block{i}' for i, block in enumerate(blocks)}
            instructions = {block: list(sequence(l.GetFirstInstruction, l.GetNextInstruction, block))
                            for block in blocks}
            self.phis = {block: [v for v in instructions[block] if l.GetInstructionOpcode(v) == 44]
                         for block in blocks}
            self.pairs = {}
            self.invoke_edges = []
            self.pad_insts = {block: inst for block in blocks for inst in instructions[block]
                              if l.GetInstructionOpcode(inst) == 59}
            caught = {l.GetClause(pad, n) for pad in self.pad_insts.values()
                      for n in range(l.GetNumClauses(pad))}
            if len(caught) > 63:
                raise Unsupported('more than 63 caught types in one function')
            self.split_pads = {block for block in self.pad_insts if self.phis[block]}
            # Reserve names for all results before translating any edge or instruction.
            for block in blocks:
                for inst in instructions[block]:
                    if (l.GetInstructionOpcode(inst) == 59
                            or (l.GetInstructionOpcode(inst) == 44 and l.GetTypeKind(l.TypeOf(inst)) == 10)):
                        if not self.landing_pair_type(l.TypeOf(inst)):
                            raise Unsupported('landingpad result must be unpacked {ptr, i32}')
                        self.pairs[inst] = self.fresh_pair()
                        continue
                    if (l.GetInstructionOpcode(inst) == 54
                            and self.integer_pair_type(l.TypeOf(inst))):
                        continue
                    if l.GetTypeKind(l.TypeOf(inst)) != 0:
                        self.type(l.TypeOf(inst))
                        self.values[inst] = f'v{self.next_value}'
                        self.next_value += 1
            grounded = set(self.pad_insts.values())
            pending = {v for v in self.pairs if v not in grounded}
            while pending:
                ready = {v for v in pending if all(l.GetIncomingValue(v, n) in grounded
                         for n in range(l.CountIncoming(v)))}
                if not ready:
                    raise Unsupported('landingpad PHIs must be acyclic merges of landingpad pairs')
                grounded.update(ready)
                pending -= ready
            self.slots = {}
            for block in blocks:
                for inst in instructions[block]:
                    if l.GetInstructionOpcode(inst) != 26:
                        continue
                    count = l.GetOperand(inst, 0)
                    if block != blocks[0] or not l.IsAConstantInt(count) or self.width(count) > 64:
                        raise Unsupported(f'{name}: only constant entry-block alloca supported')
                    if re.search(r'\b(?:inalloca|swifterror)\b', l.printed(inst)):
                        raise Unsupported(f'{name}: special alloca ABI')
                    self.type(l.TypeOf(inst))
                    ty = l.GetAllocatedType(inst)
                    self.layout_type(ty)
                    size = l.ABISizeOfType(self.td, ty) * l.ConstIntGetZExtValue(count)
                    align = max(l.GetAlignment(inst), l.ABIAlignmentOfType(self.td, ty))
                    if not 0 < size <= 0xffffffff or align > 16:
                        raise Unsupported(f'{name}: alloca size/alignment exceeds fixed stack slot contract')
                    slot = f'ss{len(self.slots)}'
                    self.slots[inst] = slot
                    self.lines.append(f'    {slot} = stack_slot {size}, align {align}')
            for block in blocks:
                for inst in instructions[block]:
                    if l.GetInstructionOpcode(inst) not in (45, 5):
                        continue
                    for i in range(l.GetNumArgOperands(inst)):
                        if self.integer_pair_type(l.TypeOf(l.GetOperand(inst, i))):
                            slot = f'ss{len(self.slots) + len(self.aggregate_slots)}'
                            self.aggregate_slots[inst, i] = slot
                            self.lines.append(f'    {slot} = stack_slot 16, align 8')
            aliases = {}
            for block in blocks:
                self.block = block
                parameters = []
                if block == blocks[0]:
                    parameters += [f'v{i}: {"ptr" if self.integer_pair_type(l.TypeOf(l.GetParam(fn, i))) else self.type(l.TypeOf(l.GetParam(fn, i)))}' for i in range(len(params))]
                if block in self.split_pads:
                    a, b = self.pairs[self.pad_insts[block]]
                    parameters += [f'{a}: ptr', f'{b}: i32']
                for phi in self.phis[block]:
                    if phi in self.pairs:
                        a, b = self.pairs[phi]
                        parameters += [f'{a}: ptr', f'{b}: i32']
                    else:
                        parameters += [f'{self.values[phi]}: {self.type(l.TypeOf(phi))}']
                self.lines.append(self.blocks[block] + ('(' + ', '.join(parameters) + ')' if parameters else '') + ':')
                if block == blocks[0]:
                    for i in range(len(params)):
                        arg = l.GetParam(fn, i)
                        if self.integer_pair_type(l.TypeOf(arg)):
                            low = self.emit(f'load.i64 v{i}')
                            offset = self.emit('iconst.i64 8')
                            address = self.emit(f'ptradd v{i}, {offset}')
                            high = self.emit(f'load.i64 {address}')
                            self.aggregate_params[arg] = (low, high)
                for inst in instructions[block]:
                    if l.GetInstructionOpcode(inst) == 44:
                        continue
                    try:
                        value = self.instruction(inst)
                        if value is not None:
                            aliases[self.values[inst]] = value
                    except Unsupported as e:
                        raise Unsupported(f'{name}: {e}') from e
            # Invoke edge copies execute only after the call (and after a
            # cloned landingpad on unwind). Preserve each LLVM PHI incoming.
            for bridge, predecessor, target, pad in self.invoke_edges:
                self.block = predecessor
                self.lines.append(bridge + ':')
                if pad is not None:
                    pair = self.fresh_pair()
                    self.lines.append('    ' + ', '.join(pair) + ' = landingpad ' + self.clauses(pad))
                    edge = self.edge(target)
                    # split pads necessarily have PHIs, so edge has parentheses.
                    edge = edge.replace('(', '(' + ', '.join(pair) + ', ', 1)
                else:
                    edge = self.edge(target)
                self.lines.append('    jump ' + edge)
            # Resolve aliases after every block: LLVM block order need not be dominance order.
            def resolve(match):
                value, path = match.group(), []
                while value in aliases:
                    path.append(value)
                    value = aliases[value]
                for alias in path:
                    aliases[alias] = value
                return value
            self.lines = [re.sub(r'(?<![A-Za-z0-9_.$@])v[0-9]+\b', resolve, line) for line in self.lines]
            self.lines.append('}')
            output.append('\n'.join(self.lines))
        if self.needs_resume:
            existing = [fn for fn in functions if l.name(fn) == '_Unwind_Resume']
            if existing:
                if self.signatures[existing[0]] != ('void', ['ptr']) or l.GetFirstBasicBlock(existing[0]):
                    raise Unsupported('incompatible _Unwind_Resume runtime declaration')
            else:
                output.insert(0, 'decl @_Unwind_Resume(ptr) cc(c)')
        return '\n\n'.join(output) + '\n'


def import_module(l, source, localize_odr=False):
    context, module = l.ContextCreate(), P()
    error = P()
    try:
        # LLVMParseIRInContext takes ownership of this copied buffer.
        buffer = l.CreateMemoryBufferWithMemoryRangeCopy(source, len(source), b'input.ll')
        if l.ParseIRInContext(context, buffer, C.byref(module), C.byref(error)):
            raise Unsupported(C.string_at(error).decode(errors='replace'))
        if l.VerifyModule(module, 2, C.byref(error)):
            raise Unsupported(C.string_at(error).decode(errors='replace'))
        if error.value:
            l.DisposeMessage(error)
            error = P()
        return Importer(l, localize_odr=localize_odr).translate(module)
    finally:
        if error.value:
            l.DisposeMessage(error)
        if module.value:
            l.DisposeModule(module)
        l.ContextDispose(context)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--libllvm', required=True, help='path to LLVM 23 shared library')
    p.add_argument('--keel', required=True, help='Keel executable used to verify output')
    p.add_argument('--localize-odr-functions', action='store_true',
                   help='assert no outside-TU references to linkonce_odr/available_externally '
                        'definitions; retain direct-only bodies without named storage as local copies '
                        '(no exported/coalesced symbol contract)')
    p.add_argument('input', help='LLVM .ll file, or - for stdin')
    p.add_argument('-o', '--output', default='-', help='Keel IR output, default stdout')
    args = p.parse_args()
    source = sys.stdin.buffer.read() if args.input == '-' else Path(args.input).read_bytes()
    result = import_module(LLVM(args.libllvm), source, localize_odr=args.localize_odr_functions)
    with tempfile.TemporaryDirectory(prefix='keel-llvm-import-') as tmp:
        path = Path(tmp) / 'verify.keel'
        path.write_text(result)
        v = subprocess.run([args.keel, 'opt', '--opt-tier', 'none', str(path), '-o', os.devnull], capture_output=True)
        if v.returncode:
            raise Unsupported('Keel verification failed: ' + v.stderr.decode(errors='replace'))
    if args.output == '-':
        sys.stdout.write(result)
    else:
        dest = Path(args.output)
        name = None
        try:
            with tempfile.NamedTemporaryFile(mode='w', dir=dest.parent, prefix='.llvm-import-', delete=False) as tmp:
                name = tmp.name
                tmp.write(result)
            os.replace(name, dest)
        finally:
            if name and os.path.exists(name):
                os.unlink(name)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, AttributeError, C.ArgumentError) as e:
        print(f'llvm-import: {e}', file=sys.stderr)
        sys.exit(1)
