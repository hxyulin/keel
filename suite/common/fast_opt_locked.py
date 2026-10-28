#!/usr/bin/env python3
"""Serialize fast-opt builds/measurements with the shared investigation lock.

Usage: python3 suite/common/fast_opt_locked.py LOG.jsonl COMMAND [ARG ...]
The command's stdout/stderr are inherited. Append-only metadata records load,
wait time, exact argv, and status; measurements should use a frozen checkout.
"""
import fcntl
import json
import os
import pathlib
import subprocess
import sys
import time


def main():
    log, *command = sys.argv[1:]
    if not command:
        raise SystemExit(__doc__)
    requested = time.time()
    with open('/tmp/keel-perf-20261019.lock', 'a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        start = time.time()
        before = os.getloadavg()
        result = subprocess.run(command)
        record = dict(command=command, started_unix=start,
                      wait_s=start-requested, wall_s=time.time()-start,
                      load_before=before, load_after=os.getloadavg(),
                      returncode=result.returncode)
        with pathlib.Path(log).open('a') as out:
            out.write(json.dumps(record)+'\n')
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
