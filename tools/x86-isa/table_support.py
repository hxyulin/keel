"""Shared Rust representations for the legacy/VEX and EVEX catalogues."""

OPERAND_SIZE_RULES = {
    'default': 'Default',
    'byteop': 'Byte',
    'default64': 'Default64',
    'force64': 'Force64',
    'force32_or_64': 'Force32Or64',
    'force32_or_rexw': 'Force32OrRexW',
    'ignore66': 'Ignore66',
    'rexw32': 'RexW32',
}


def operand_size_rule(name):
    try:
        return 'OperandSizeRule::' + OPERAND_SIZE_RULES[name]
    except KeyError:
        raise ValueError(f'unsupported operand-size rule: {name}') from None


class OperandLists:
    """Emit each distinct operand slice once, in first-use order."""

    def __init__(self):
        self._names = {}

    def reference(self, operands):
        key = tuple(operands)
        if not key:
            return '&[]'
        if key not in self._names:
            self._names[key] = f'OPERANDS_{len(self._names)}'
        return self._names[key]

    def declarations(self):
        lines = []
        for operands, name in self._names.items():
            lines += ['#[rustfmt::skip]',
                      f'static {name}: &[IsaOperand] = &[',
                      *[operand + ',' for operand in operands], '];']
        return lines
