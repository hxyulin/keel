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


P, U = C.c_void_p, C.c_uint
ABI_UNSUPPORTED = ('sret', 'byval', 'byref', 'inalloca', 'preallocated',
                   'inreg', 'nest', 'swiftself', 'swifterror', 'swiftasync', 'alignstack')


class Unsupported(ValueError):
    pass


class LLVM:
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
            'GetPointerAddressSpace': (U, [P]), 'GetReturnType': (P, [P]),
            'IsFunctionVarArg': (C.c_int, [P]), 'GetFunctionCallConv': (U, [P]),
            'GetInstructionCallConv': (U, [P]), 'GetLinkage': (U, [P]),
            'GetVisibility': (U, [P]), 'HasPersonalityFn': (C.c_int, [P]),
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
            'GetStringAttributeKind': (P, [P, C.POINTER(U)]),
            'GetFirstBasicBlock': (P, [P]), 'GetNextBasicBlock': (P, [P]),
            'GetFirstInstruction': (P, [P]), 'GetNextInstruction': (P, [P]),
            'GetInstructionOpcode': (U, [P]), 'GetNumOperands': (C.c_int, [P]),
            'GetOperand': (P, [P, U]), 'IsAConstantInt': (P, [P]),
            'IsAConstantPointerNull': (P, [P]), 'IsAFunction': (P, [P]),
            'ConstIntGetZExtValue': (C.c_ulonglong, [P]),
            'ConstIntGetSExtValue': (C.c_longlong, [P]),
            'GetICmpPredicate': (U, [P]), 'GetCalledValue': (P, [P]),
            'GetCalledFunctionType': (P, [P]),
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
        for name in ABI_UNSUPPORTED + ('signext', 'zeroext', 'naked', 'returns_twice', 'null_pointer_is_valid'):
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
        for name in ('naked', 'returns_twice', 'alignstack', 'null_pointer_is_valid'):
            if self.attribute(value, index, name, call=call):
                raise Unsupported(f'unsupported function/call attribute {name}')
        count = self.GetCallSiteAttributeCount if call else self.GetAttributeCountAtIndex
        get = self.GetCallSiteAttributes if call else self.GetAttributesAtIndex
        attributes = (P * count(value, index))()
        get(value, index, attributes)
        for attribute in attributes:
            if self.IsStringAttribute(attribute):
                length = U()
                key = self.GetStringAttributeKind(attribute, C.byref(length))
                name = C.string_at(key, length.value).decode()
                if name.startswith(('aarch64_', 'instrument-function-', 'patchable-')):
                    raise Unsupported(f'unsupported function/call string attribute {name}')


def sequence(first, next_, owner):
    value = first(owner)
    while value:
        yield value
        value = next_(value)


class Importer:
    ABI_UNSUPPORTED = ABI_UNSUPPORTED
    BINARY = {8: 'iadd', 10: 'isub', 12: 'imul', 14: 'udiv', 15: 'sdiv',
              17: 'urem', 18: 'srem', 20: 'shl', 21: 'ushr', 22: 'sshr',
              23: 'and', 24: 'or', 25: 'xor'}
    CMP = {32: 'eq', 33: 'ne', 34: 'ugt', 35: 'uge', 36: 'ult',
           37: 'ule', 38: 'sgt', 39: 'sge', 40: 'slt', 41: 'sle'}

    def __init__(self, llvm):
        self.l = llvm

    def symbol(self, value):
        name = self.l.name(value)
        if not re.fullmatch(r'[A-Za-z0-9_.$]+', name):
            raise Unsupported(f'unsupported symbol spelling {name!r}')
        return '@' + name

    def type(self, ty, abi=False):
        kind = self.l.GetTypeKind(ty)
        if kind == 0:
            return 'void'
        if kind == 8:
            width = self.l.GetIntTypeWidth(ty)
            if width in (8, 16, 32, 64) or (width == 1 and not abi):
                return 'i8' if width == 1 else f'i{width}'
        if kind == 12 and self.l.GetPointerAddressSpace(ty) == 0:
            return 'ptr'
        raise Unsupported('supported types are scalar i8/i16/i32/i64 and address-space-zero pointers; i1 is internal only')

    def width(self, value):
        ty = self.l.TypeOf(value)
        return self.l.GetIntTypeWidth(ty) if self.l.GetTypeKind(ty) == 8 else 0

    def signature(self, fn):
        l = self.l
        if l.GetFunctionCallConv(fn) != 0:
            raise Unsupported(f'{self.symbol(fn)}: non-C calling convention')
        fty = l.GlobalGetValueType(fn)
        if l.IsFunctionVarArg(fty):
            raise Unsupported(f'{self.symbol(fn)}: variadic signature')
        if l.GetLinkage(fn) not in (0, 8, 9) or l.GetVisibility(fn) != 0:
            raise Unsupported(f'{self.symbol(fn)}: unsupported linkage/visibility')
        if l.HasPersonalityFn(fn) or l.GetAlignment(fn) > 4:
            raise Unsupported(f'{self.symbol(fn)}: personality or explicit function alignment')
        if l.GetComdat(fn):
            raise Unsupported(f'{self.symbol(fn)}: COMDAT membership')
        if l.GetGC(fn) or l.GetSection(fn) or l.HasPrefixData(fn) or l.HasPrologueData(fn) or l.GetDLLStorageClass(fn):
            raise Unsupported(f'{self.symbol(fn)}: GC, section, prefix/prologue data or DLL storage')
        l.check_function_attributes(fn)
        types = [l.GetReturnType(fty)] + [l.TypeOf(l.GetParam(fn, i)) for i in range(l.CountParams(fn))]
        result = []
        for index, ty in enumerate(types):
            for attr in self.ABI_UNSUPPORTED:
                if l.attribute(fn, index, attr):
                    raise Unsupported(f'{self.symbol(fn)}: unsupported ABI attribute {attr}')
            text = self.type(ty, abi=True)
            if index == 0 and text in ('i8', 'i16') and l.GetFirstBasicBlock(fn) and not any(
                l.attribute(fn, 0, attr) for attr in ('signext', 'zeroext')
            ):
                raise Unsupported(f'{self.symbol(fn)}: i8/i16 return definitions require signext or zeroext')
            for attr, keel in [('signext', 'sext'), ('zeroext', 'uext')]:
                if l.attribute(fn, index, attr):
                    text += ' ' + keel
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
        if l.IsAConstantInt(value):
            ty = self.type(l.TypeOf(value))
            n = l.ConstIntGetZExtValue(value) if self.width(value) == 1 else l.ConstIntGetSExtValue(value)
            return self.emit(f'iconst.{ty} {n}')
        if l.IsAConstantPointerNull(value):
            zero = self.emit('iconst.i64 0')
            return self.emit(f'inttoptr.ptr {zero}')
        raise Unsupported('unsupported operand (including undef, poison, constant expressions or globals): ' + l.printed(value)[:160])

    def layout_type(self, ty):
        """Validate before asking LLVM for a size (unsized types can abort LLVM)."""
        l = self.l
        kind = l.GetTypeKind(ty)
        if kind in (8, 12):
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
            args.append(self.value(incoming[0]))
        return self.blocks[target] + ('(' + ', '.join(args) + ')' if args else '')

    def instruction(self, inst):
        l = self.l
        op = l.GetInstructionOpcode(inst)
        operands = lambda: [l.GetOperand(inst, i) for i in range(l.GetNumOperands(inst))]
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
            if re.search(r'\binrange\b', l.printed(inst)):
                raise Unsupported('unsupported GEP flags')
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
            if op in (8, 10, 12, 20) and (l.GetNUW(inst) or l.GetNSW(inst)):
                raise Unsupported('nuw/nsw arithmetic is not in the initial subset')
            if op in (14, 15, 21, 22) and l.GetExact(inst):
                raise Unsupported('exact arithmetic is not in the initial subset')
            if op == 24 and re.search(r'\bor disjoint\b', l.printed(inst)):
                raise Unsupported('disjoint OR is not in the initial subset')
            if self.width(inst) == 1 and op not in (23, 24, 25):
                raise Unsupported('i1 arithmetic other than boolean bitwise operations')
            a, b = map(self.value, operands())
            return self.emit(f'{self.BINARY[op]} {a}, {b}')
        if op in (30, 31, 32, 39, 40, 41):
            arg = l.GetOperand(inst, 0)
            value = self.value(arg)
            ty = self.type(l.TypeOf(inst))
            if re.search(r'\b(?:zext nneg|trunc (?:nuw|nsw))\b', l.printed(inst)):
                raise Unsupported('poison-sensitive conversion flags')
            if op == 41:
                if self.type(l.TypeOf(arg)) != ty:
                    raise Unsupported('non-identity bitcast')
                return value
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
            if re.search(r'\bicmp samesign\b', l.printed(inst)):
                raise Unsupported('samesign comparison')
            args = operands()
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
        if op == 45:
            l.check_function_attributes(inst, call=True)
            callee = l.GetCalledValue(inst)
            if not l.IsAFunction(callee) or l.GetInstructionCallConv(inst) != 0:
                raise Unsupported('indirect or non-C call')
            if l.GetNumOperandBundles(inst):
                raise Unsupported('call operand bundles')
            if l.GetCalledFunctionType(inst) != l.GlobalGetValueType(callee):
                raise Unsupported('call type differs from declaration')
            if l.name(callee).startswith('llvm.'):
                raise Unsupported('LLVM intrinsic ' + l.name(callee))
            if 'musttail' in l.printed(inst):
                raise Unsupported('musttail call')
            ret, params = self.signatures[callee]
            for index in range(l.GetNumArgOperands(inst) + 1):
                for attr in self.ABI_UNSUPPORTED:
                    if l.attribute(inst, index, attr, call=True):
                        raise Unsupported('call-site ABI attribute ' + attr)
                for attr in ('signext', 'zeroext'):
                    if l.attribute(inst, index, attr, call=True) != l.attribute(callee, index, attr):
                        raise Unsupported('call-site extension differs from declaration')
            args = ', '.join(self.value(l.GetOperand(inst, i)) for i in range(l.GetNumArgOperands(inst)))
            expr = f'call {self.symbol(callee)}({args})'
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
        if l.GetFirstGlobal(module) or l.GetFirstGlobalAlias(module) or l.GetFirstGlobalIFunc(module):
            raise Unsupported('global data, aliases and ifuncs are not in the initial subset')
        if l.GetNamedMetadata(module, b'llvm.linker.options', len(b'llvm.linker.options')):
            raise Unsupported('module autolink requests (llvm.linker.options)')
        functions = list(sequence(l.GetFirstFunction, l.GetNextFunction, module))
        self.signatures = {fn: self.signature(fn) for fn in functions}
        output = []
        for fn in functions:
            ret, params = self.signatures[fn]
            name = self.symbol(fn)
            signature = name + '(' + ', '.join(params) + ')'
            if ret != 'void':
                signature += ' -> ' + ret
            blocks = list(sequence(l.GetFirstBasicBlock, l.GetNextBasicBlock, fn))
            if not blocks:
                output.append('decl ' + signature + ' cc(c)')
                continue
            local = ' local' if l.GetLinkage(fn) in (8, 9) else ''
            self.lines = ['func ' + signature + ' cc(c)' + local + ' {']
            self.values = {l.GetParam(fn, i): f'v{i}' for i in range(l.CountParams(fn))}
            self.next_value = len(params)
            self.blocks = {block: f'block{i}' for i, block in enumerate(blocks)}
            instructions = {block: list(sequence(l.GetFirstInstruction, l.GetNextInstruction, block))
                            for block in blocks}
            self.phis = {block: [v for v in instructions[block] if l.GetInstructionOpcode(v) == 44]
                         for block in blocks}
            # Reserve names for all results before translating any edge or instruction.
            for block in blocks:
                for inst in instructions[block]:
                    if l.GetTypeKind(l.TypeOf(inst)) != 0:
                        self.type(l.TypeOf(inst))
                        self.values[inst] = f'v{self.next_value}'
                        self.next_value += 1
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
            aliases = {}
            for block in blocks:
                self.block = block
                parameters = []
                if block == blocks[0]:
                    parameters += [f'v{i}: {self.type(l.TypeOf(l.GetParam(fn, i)))}' for i in range(len(params))]
                parameters += [f'{self.values[v]}: {self.type(l.TypeOf(v))}' for v in self.phis[block]]
                self.lines.append(self.blocks[block] + ('(' + ', '.join(parameters) + ')' if parameters else '') + ':')
                for inst in instructions[block]:
                    if l.GetInstructionOpcode(inst) == 44:
                        continue
                    try:
                        value = self.instruction(inst)
                        if value is not None:
                            aliases[self.values[inst]] = value
                    except Unsupported as e:
                        raise Unsupported(f'{name}: {e}') from e
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
        return '\n\n'.join(output) + '\n'


def import_module(l, source):
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
        return Importer(l).translate(module)
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
    p.add_argument('input', help='LLVM .ll file, or - for stdin')
    p.add_argument('-o', '--output', default='-', help='Keel IR output, default stdout')
    args = p.parse_args()
    source = sys.stdin.buffer.read() if args.input == '-' else Path(args.input).read_bytes()
    result = import_module(LLVM(args.libllvm), source)
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
