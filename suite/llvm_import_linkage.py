"""Opt-in local copies, never an implementation of weak symbol coalescing.

The caller asserts that no outside TU requires the candidate symbols. LLVM ODR
bodies may then be copied for direct calls, retaining every definition/name. No
candidate address may be observed, and the transitive helper closure must have
no named storage dependencies or calls outside that closure. COMDAT is rejected
(including singleton groups): this pass does not implement group selection.
"""
import ctypes as C


def local_copies(importer, module, functions, unsupported):
    l = importer.l
    p = C.c_void_p
    specs = {
        'GetFirstUse': (p, [p]), 'GetNextUse': (p, [p]), 'GetUser': (p, [p]),
        'IsACallInst': (p, [p]), 'IsAConstant': (p, [p]),
        'IsAGlobalVariable': (p, [p]),
    }
    for name, (result, args) in specs.items():
        f = getattr(l.lib, 'LLVM' + name)
        f.restype, f.argtypes = result, args
        setattr(l, name, f)
    candidates = {fn for fn in functions if l.GetLinkage(fn) in (1, 3)}

    def reject(fn, reason):
        raise unsupported(f'{importer.symbol(fn)}: local ODR copy: {reason}')

    for fn in candidates:
        if not l.GetFirstBasicBlock(fn):
            reject(fn, 'requires a definition')
        if l.GetComdat(fn):
            reject(fn, 'COMDAT selection/group contract unsupported')
        use = l.GetFirstUse(fn)
        while use:
            user = l.GetUser(use)
            # Checking all argument operands matters: f(f) is also a direct call.
            if (not l.IsACallInst(user) or l.GetCalledValue(user) != fn
                    or any(l.GetOperand(user, n) == fn
                           for n in range(l.GetNumArgOperands(user)))
                    or l.GetNumOperandBundles(user)):
                reject(fn, 'address escape or non-direct-call use')
            use = l.GetNextUse(use)

    # Inspect transitive local callees as well: hiding a storage dependency one
    # call away must not turn TU-specific state into an apparent ODR-safe body.
    pending, visited = list(candidates), set()
    while pending:
        fn = pending.pop()
        if fn in visited:
            continue
        visited.add(fn)

        def operand(value):
            if l.IsAFunction(value):
                reject(fn, 'function address dependency')
            if l.IsAGlobalVariable(value):
                reject(fn, 'named storage dependency')
            if l.IsAConstant(value):
                for n in range(l.GetNumOperands(value)):
                    operand(l.GetOperand(value, n))

        block = l.GetFirstBasicBlock(fn)
        while block:
            inst = l.GetFirstInstruction(block)
            while inst:
                if l.IsACallInst(inst):
                    callee = l.GetCalledValue(inst)
                    if (not l.IsAFunction(callee) or not l.GetFirstBasicBlock(callee)
                            or (callee not in candidates and l.GetLinkage(callee) not in (8, 9))):
                        reject(fn, 'call outside defined local/ODR helper closure')
                    pending.append(callee)
                    for n in range(l.GetNumArgOperands(inst)):
                        operand(l.GetOperand(inst, n))
                    if l.GetNumOperandBundles(inst):
                        reject(fn, 'operand bundle')
                else:
                    for n in range(l.GetNumOperands(inst)):
                        operand(l.GetOperand(inst, n))
                inst = l.GetNextInstruction(inst)
            block = l.GetNextBasicBlock(block)
    return candidates
