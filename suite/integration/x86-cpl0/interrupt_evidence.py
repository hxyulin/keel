"""Bind IRQ-frame observations to independent GNU-decoded HLT boundaries."""
import re
import subprocess

import run

REPORT = re.compile(re.escape(run.READY.decode())
                    + r'KEEL IRQ HLT ([0-9a-f]{16})\n'
                    + r'KEEL IRQ STIHLT ([0-9a-f]{16})\n'
                    + re.escape(run.PASS.decode()))
INSTRUCTION = re.compile(r'^\s*([0-9a-f]+):\s+((?:[0-9a-f]{2}\s+)+)([a-z][a-z0-9_.]*)\b')


def report(result):
    if result['status'] != 'pass' or result['returncode'] != 33:
        return None
    match = REPORT.fullmatch(result['transcript'])
    return tuple(int(value, 16) for value in match.groups()) if match else None


def halt_resume(listing, adjacent_sti=False):
    instructions = []
    for line in listing.splitlines():
        match = INSTRUCTION.match(line)
        if match:
            instructions.append((int(match[1], 16), bytes.fromhex(match[2]), match[3]))
    halts = [i for i, (_, _, mnemonic) in enumerate(instructions) if mnemonic == 'hlt']
    if len(halts) != 1:
        return None
    index = halts[0]
    address, encoded, _ = instructions[index]
    if encoded != b'\xf4':
        return None
    if adjacent_sti and (index == 0 or instructions[index - 1] != (address - 1, b'\xfb', 'sti')):
        return None
    return address + 1


def observe(result, elf, objdump):
    observed = report(result)
    expected = []
    if observed is not None:
        for function, pair in [('interrupt_halt', False), ('interrupt_enable_halt', True)]:
            command = [objdump, '-d', f'--disassemble={function}', str(elf)]
            decoded = subprocess.run(command, capture_output=True, text=True, timeout=20)
            if decoded.returncode:
                raise run.HarnessError(f'objdump witness failed: {command!r}: {decoded.stderr}')
            expected.append(halt_resume(decoded.stdout, pair))
    return dict(report=observed is not None, observed=observed, expected=expected,
                valid=observed is not None and tuple(expected) == observed)


def expected_verdict(result, evidence, negative):
    normal_fail = (result['status'] == 'guest-error' and result['returncode'] == 35
                   and result['transcript'] == (run.READY + run.FAIL).decode())
    normal_report = evidence['report'] and report(result) is not None
    if negative:
        # A skipped HLT can race the timer. Even if the guest returns normally
        # with a complete report, its recorded RIP must fail the boundary check.
        return normal_fail or (normal_report and not evidence['valid'])
    return normal_report and evidence['valid']
