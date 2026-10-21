"""Conservative LLVM global objects and symbolic constants for llvm_import.

No LLVM text is rewritten. Layout comes from the verified module's TargetData;
relocations obey Keel's eight-byte alignment and unsigned-u32 addend contract.
Floating-point initializers serialize through integer bits, avoiding host rounding.
"""
import ctypes as C
import re


class Globals:
    def __init__(self, importer, module, functions, unsupported):
        self.Unsupported = unsupported
        self.i, self.l = importer, importer.l
        l = self.l
        p, u = C.c_void_p, C.c_uint
        specs = {
            'GetNextGlobal': (p, [p]), 'GetInitializer': (p, [p]),
            'IsGlobalConstant': (C.c_int, [p]), 'IsThreadLocal': (C.c_int, [p]),
            'IsExternallyInitialized': (C.c_int, [p]),
            'IsAConstantAggregateZero': (p, [p]), 'IsAConstantExpr': (p, [p]),
            'GetConstOpcode': (u, [p]), 'GetAggregateElement': (p, [p, u]),
        }
        for name, (result, args) in specs.items():
            f = getattr(l.lib, 'LLVM' + name)
            f.restype, f.argtypes = result, args
            setattr(l, name, f)
        self.objects = []
        g = l.GetFirstGlobal(module)
        while g:
            self.objects.append(g)
            g = l.GetNextGlobal(g)
        self.symbols = set(functions) | set(self.objects)
        self.sizes = {}
        for g in self.objects:
            importer.type(l.TypeOf(g))  # no nonzero address spaces
            if l.name(g).startswith('llvm.'):
                raise self.Unsupported('reserved LLVM global objects')
            if (l.GetLinkage(g) not in (0, 8, 9) or l.GetVisibility(g) != 0
                    or l.GetDLLStorageClass(g) or l.IsThreadLocal(g)
                    or l.IsExternallyInitialized(g) or l.GetSection(g) or l.GetComdat(g)):
                raise self.Unsupported(f'{importer.symbol(g)}: unsupported global linkage/storage/section/COMDAT')
            self.check_suffix(l.printed(g))
            ty = l.GlobalGetValueType(g)
            self.layout(ty)
            size = l.ABISizeOfType(importer.td, ty)
            if not 0 < size <= 0xffffffff:
                raise self.Unsupported('global size must be nonzero and fit u32')
            if l.GetInitializer(g):
                self.sizes[g] = size

    def check_suffix(self, text):
        # LLVM prints global attributes after the top-level initializer. Only
        # alignment is represented here; reject metadata/sanitizers/partition/
        # code_model and future suffixes, without interpreting initializer text.
        depth, quoted, escaped = 0, False, False
        for position, char in enumerate(text):
            if quoted:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    quoted = False
            elif char == '"':
                quoted = True
            elif char in '([{<':
                depth += 1
            elif char in ')]}>':
                depth -= 1
            elif char == ',' and depth == 0:
                if not re.fullmatch(r'(?:,\s*align [0-9]+)\s*', text[position:]):
                    raise self.Unsupported('unsupported global suffix/metadata')
                break
        if re.search(r'#[0-9]+\s*$', text):
            raise self.Unsupported('global attribute groups')

    def layout(self, ty):
        # Separate from instruction/FP tracks' policy. Bound aggregate sizes
        # before TargetData queries: LLVM's fixed-width size math may overflow.
        l, td = self.l, self.i.td
        kind = l.GetTypeKind(ty)
        if kind in (2, 3):
            self.i.type(ty)
        elif kind == 8 and l.GetIntTypeWidth(ty) in (1, 8, 16, 32, 64):
            pass
        elif kind == 12 and l.GetPointerAddressSpace(ty) == 0:
            pass
        elif kind == 11:
            child_size = self.layout(l.GetElementType(ty))
            if child_size * l.GetArrayLength2(ty) > 0xffffffff:
                raise self.Unsupported('global array layout exceeds u32')
        elif kind == 10 and not l.IsOpaqueStruct(ty):
            # Sum with worst-case padding bounds before querying struct size.
            # Rejecting a near-limit object is preferable to a wrapped size.
            bound = 0
            for n in range(l.CountStructElementTypes(ty)):
                child = l.StructGetTypeAtIndex(ty, n)
                bound += self.layout(child) + l.ABIAlignmentOfType(td, child) - 1
                if bound > 0xffffffff:
                    raise self.Unsupported('global struct layout exceeds u32')
        else:
            raise self.Unsupported('unsupported global layout (unsized/vector/address-space/unsupported scalar type)')
        size = l.ABISizeOfType(td, ty)
        if size > 0xffffffff:
            raise self.Unsupported('global layout exceeds u32')
        return size

    def address(self, value):
        """Return a (symbol value, byte addend), or None for nonsymbol constants."""
        l, i = self.l, self.i
        if value in self.symbols:
            return value, 0
        if not l.IsAConstantExpr(value):
            return None
        if l.GetConstOpcode(value) != 29:
            raise self.Unsupported('only constant GEP symbolic expressions are supported')
        if re.search(r'\binrange\b', l.printed(value)):
            raise self.Unsupported('constant GEP inrange')
        base = self.address(l.GetOperand(value, 0))
        if base is None:
            raise self.Unsupported('constant GEP requires a named symbol base')
        symbol, offset = base
        ty = l.GetGEPSourceElementType(value)
        self.layout(ty)
        for n in range(1, l.GetNumOperands(value)):
            index = l.GetOperand(value, n)
            width = i.width(index)
            if not l.IsAConstantInt(index) or width not in (8, 16, 32, 64):
                raise self.Unsupported('constant GEP requires i8/i16/i32/i64 integer indices')
            number = l.ConstIntGetSExtValue(index)
            if n > 1 and l.GetTypeKind(ty) == 10:
                if not 0 <= number < l.CountStructElementTypes(ty):
                    raise self.Unsupported('constant GEP struct index out of bounds')
                offset += l.OffsetOfElement(i.td, ty, number)
                ty = l.StructGetTypeAtIndex(ty, number)
            else:
                if n > 1:
                    if l.GetTypeKind(ty) != 11:
                        raise self.Unsupported('constant GEP indexes unsupported layout')
                    ty = l.GetElementType(ty)
                offset += number * l.ABISizeOfType(i.td, ty)
        if not 0 <= offset <= 0xffffffff:
            raise self.Unsupported('symbol addend must fit unsigned u32')
        if symbol in self.sizes and offset > self.sizes[symbol]:
            raise self.Unsupported('symbol addend past defined object end')
        if l.IsAFunction(symbol) and offset:
            raise self.Unsupported('function symbol addends are unsupported')
        return symbol, offset

    def operand(self, value):
        address = self.address(value)
        if address is None:
            return None
        symbol, offset = address
        result = self.i.emit('symbol_addr ' + self.i.symbol(symbol))
        if offset:
            addend = self.i.emit(f'iconst.i64 {offset}')
            result = self.i.emit(f'ptradd {result}, {addend}')
        return result

    def initializer(self, value, ty, offset, align):
        l, i = self.l, self.i
        size = l.ABISizeOfType(i.td, ty)
        if l.IsAConstantAggregateZero(value) or l.IsAConstantPointerNull(value):
            return [(size, f'zero {size}')] if size else []
        kind = l.GetTypeKind(ty)
        if kind in (2, 3) and l.IsAConstantFP(value):
            width = 32 if kind == 2 else 64
            integer = l.IntTypeInContext(l.GetTypeContext(ty), width)
            bits = l.ConstIntGetZExtValue(l.ConstBitCast(value, integer))
            return [(width // 8, f'i{width} 0x{bits:0{width // 4}x}')]
        if kind == 8 and l.IsAConstantInt(value):
            width = l.GetIntTypeWidth(ty)
            number = l.ConstIntGetZExtValue(value) if width == 1 else l.ConstIntGetSExtValue(value)
            used = max(1, width // 8)
            items = [(used, f'i{max(8, width)} {number}')]
            if size > used:
                items.append((size - used, f'zero {size - used}'))
            return items
        if kind == 12:
            address = self.address(value)
            if address is not None:
                if offset % 8 or align < 8:
                    raise self.Unsupported('pointer relocation requires aligned offset and object alignment >= 8')
                symbol, addend = address
                return [(8, 'ptr ' + i.symbol(symbol) + (f' + {addend}' if addend else ''))]
        if kind in (10, 11):
            count = l.CountStructElementTypes(ty) if kind == 10 else l.GetArrayLength2(ty)
            items, cursor = [], 0
            for n in range(count):
                child_ty = l.StructGetTypeAtIndex(ty, n) if kind == 10 else l.GetElementType(ty)
                position = l.OffsetOfElement(i.td, ty, n) if kind == 10 else n * l.ABISizeOfType(i.td, child_ty)
                if position > cursor:
                    items.append((position - cursor, f'zero {position - cursor}'))
                child = l.GetAggregateElement(value, n)
                if not child:
                    raise self.Unsupported('unsupported aggregate initializer')
                parts = self.initializer(child, child_ty, offset + position, align)
                items.extend(parts)
                cursor = position + sum(length for length, _ in parts)
            if size > cursor:
                items.append((size - cursor, f'zero {size - cursor}'))
            return items
        raise self.Unsupported('unsupported global initializer (undef/poison/integer expression)')

    def definitions(self):
        l, i = self.l, self.i
        output = []
        for g in self.objects:
            name, value = i.symbol(g), l.GetInitializer(g)
            if not value:
                output.append('decl data ' + name)
                continue
            ty = l.GlobalGetValueType(g)
            align = max(l.GetAlignment(g), l.ABIAlignmentOfType(i.td, ty))
            if not 0 < align <= 4096 or align & (align - 1):
                raise self.Unsupported('global alignment must be a power of two up to 4096')
            items = self.initializer(value, ty, 0, align)
            if sum(size for size, _ in items) != self.sizes[g]:
                raise self.Unsupported('initializer size differs from LLVM allocation size')
            flags = ('local ' if l.GetLinkage(g) in (8, 9) else '') + ('const ' if l.IsGlobalConstant(g) else '')
            output.append(f'data {name} = {flags}align {align} {{ ' + ', '.join(text for _, text in items) + ' }')
        return output
