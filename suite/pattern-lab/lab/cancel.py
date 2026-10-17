"""CLI cancellation and short signal-safe lifecycle sections (main thread only)."""
from contextlib import contextmanager
import signal
import threading

class TerminationRequested(KeyboardInterrupt):
    pass

def raise_cancel(signum, frame=None):
    if signum == signal.SIGTERM:
        raise TerminationRequested('SIGTERM')
    raise KeyboardInterrupt('SIGINT')

@contextmanager
def signal_handlers(handler):
    saved={}
    if threading.current_thread() is threading.main_thread():
        for sig in (signal.SIGINT,signal.SIGTERM):
            saved[sig]=signal.signal(sig,handler)
    try:
        yield
    finally:
        for sig,old in saved.items(): signal.signal(sig,old)

@contextmanager
def defer_spawn_cancellation():
    # A signal between Popen returning and assignment must not lose ownership
    # of a newly detached child. Delivery is deferred until ownership is stored.
    pending=[]
    with signal_handlers(lambda sig,frame: pending.append(sig)):
        yield
    if pending: raise_cancel(pending[0])

@contextmanager
def ignore_cancellation():
    # Kill/reap and evidence finalization are bounded critical sections. A
    # repeated INT/TERM must not interrupt them and strand the original child.
    with signal_handlers(signal.SIG_IGN):
        yield
