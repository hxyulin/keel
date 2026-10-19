#!/usr/bin/env python3
"""Run one measurement command while holding the shared perf lock.

Diagnostic helper for the IR-lowering investigation. Holds an exclusive
fcntl lock on /tmp/keel-perf-20261019.lock across the subprocess (so other
benchmark agents serialize with it), records load averages before/after and
the lock wait, and writes stdout to --output.

  python3 suite/ir_lowering_locked_run.py --output out.jsonl -- CMD ARGS...
"""
import argparse
import fcntl
import json
import os
import subprocess
import sys
import time

LOCK = '/tmp/keel-perf-20261019.lock'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True)
    parser.add_argument('--meta', help='JSON metadata file (default OUTPUT.meta.json)')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        parser.error('missing command')
    requested = time.time()
    with open(LOCK, 'a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        acquired = time.time()
        before = os.getloadavg()
        with open(args.output, 'w') as out:
            result = subprocess.run(command, stdout=out)
        after = os.getloadavg()
        finished = time.time()
    meta = {
        'command': command,
        'returncode': result.returncode,
        'lock': LOCK,
        'lock_wait_s': acquired - requested,
        'wall_s': finished - acquired,
        'loadavg_before': before,
        'loadavg_after': after,
        'started_unix': acquired,
    }
    with open(args.meta or args.output + '.meta.json', 'w') as f:
        json.dump(meta, f, indent=2)
    print(json.dumps(meta), file=sys.stderr)
    sys.exit(result.returncode)


if __name__ == '__main__':
    main()
