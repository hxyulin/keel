#!/usr/bin/env python3
"""Real LLVM 23 policy/translation checks; no native interoperability claims.

python3 suite/test_llvm_import_odr.py --libllvm /path/to/libLLVM.dylib
Add --keel /path/to/keel for real CLI preservation/atomicity checks. Once the
foundational syntax is merged, add --verify-weak-odr to require its verifier.
Run compiler builds/native validation separately under fast_opt_locked.py.
"""
import argparse
import ctypes as C
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from llvm_import import LLVM, P, Unsupported, import_module


HEADER = b'''target triple = "arm64-apple-macosx14.0.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"
'''
RTTI = '''@class_vtable = external global [0 x ptr]
@si_vtable = external global [0 x ptr]
@base_name = linkonce_odr constant [2 x i8] c"B\\00"
@derived_name = weak_odr constant [2 x i8] c"D\\00"
@base = linkonce_odr constant {ptr, ptr} {
 ptr getelementptr (ptr, ptr @class_vtable, i64 2), ptr @base_name}, align 8
@derived = weak_odr constant {ptr, ptr, ptr} {
 ptr getelementptr (ptr, ptr @si_vtable, i64 2), ptr @derived_name, ptr @base}, align 8
'''
HIDDEN_RTTI = '''@_ZTVN10__cxxabiv117__class_type_infoE = external global [0 x ptr]
@_ZTVN10__cxxabiv120__si_class_type_infoE = external global [0 x ptr]
@_ZTS1B = linkonce_odr hidden constant [3 x i8] c"1B\\00"
@_ZTS1D = weak_odr hidden constant [3 x i8] c"1D\\00"
@_ZTI1B = linkonce_odr hidden constant {ptr, ptr} {
 ptr getelementptr (ptr, ptr @_ZTVN10__cxxabiv117__class_type_infoE, i64 2),
 ptr inttoptr (i64 add (i64 ptrtoint (ptr @_ZTS1B to i64), i64 -9223372036854775808) to ptr)}, align 8
@_ZTI1D = weak_odr hidden constant {ptr, ptr, ptr} {
 ptr getelementptr (ptr, ptr @_ZTVN10__cxxabiv120__si_class_type_infoE, i64 2),
 ptr inttoptr (i64 add (i64 ptrtoint (ptr @_ZTS1D to i64), i64 -9223372036854775808) to ptr), ptr @_ZTI1B}, align 8
'''
EH = '''declare i32 @__gxx_personality_v0(...)
declare void @thrower()
define void @catcher() personality ptr @__gxx_personality_v0 {
entry:
 invoke void @thrower() to label %normal unwind label %pad
normal:
 ret void
pad:
 %lp = landingpad {ptr, i32} catch ptr @derived
 resume {ptr, i32} %lp
}
'''

LIFETIMES = '''declare void @llvm.lifetime.start.p0(i64 immarg, ptr nocapture)
declare void @llvm.lifetime.end.p0(i64 immarg, ptr nocapture)
define i64 @lifetime_roundtrip(i64 %x) {
 %p = alloca i64, align 8
 call void @llvm.lifetime.start.p0(i64 8, ptr %p)
 store i64 %x, ptr %p
 call void @llvm.lifetime.end.p0(i64 8, ptr %p)
 %dead = load i64, ptr %p
 %quotient = sdiv i64 %dead, -1
 %masked = select i1 false, i64 %quotient, i64 %x
 call void @llvm.lifetime.start.p0(i64 8, ptr %p)
 %next = add i64 %x, 1
 store i64 %next, ptr %p
 %live = load i64, ptr %p
 call void @llvm.lifetime.end.p0(i64 8, ptr %p)
 %result = xor i64 %masked, %live
 ret i64 %result
}
'''


class ODRImport(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.llvm = LLVM(ARGS.libllvm)

    def translate(self, body, **kwargs):
        source = HEADER + body.encode()
        before = bytes(source)
        result = import_module(self.llvm, source, **kwargs)
        self.assertEqual(source, before)
        return result

    def reject(self, body, diagnostic=None, **kwargs):
        with self.assertRaisesRegex(Unsupported, diagnostic or '.'):
            self.translate(body, **kwargs)

    def test_c_api_linkage_values(self):
        # Verify the named constants against this installed C API, too.
        l = self.llvm
        for spelling, expected in [('linkonce_odr', l.LinkOnceODRLinkage),
                                   ('weak_odr', l.WeakODRLinkage)]:
            with self.subTest(spelling=spelling):
                context, module, error = l.ContextCreate(), P(), P()
                try:
                    source = HEADER + f'define {spelling} void @f() {{ ret void }}'.encode()
                    buffer = l.CreateMemoryBufferWithMemoryRangeCopy(source, len(source), b'odr')
                    self.assertEqual(l.ParseIRInContext(context, buffer, C.byref(module), C.byref(error)), 0)
                    self.assertEqual(l.GetLinkage(l.GetFirstFunction(module)), expected)
                finally:
                    if error.value:
                        l.DisposeMessage(error)
                    if module.value:
                        l.DisposeModule(module)
                    l.ContextDispose(context)

    def test_definition_linkages_and_symbol_identities(self):
        for linkage in ('linkonce_odr', 'weak_odr'):
            with self.subTest(linkage=linkage):
                result = self.translate(f'''@address = global ptr @_Z1fv
@raw.$name = {linkage} global i64 7
define {linkage} ptr @_Z1fv() {{ ret ptr @raw.$name }}
define internal void @unused() {{ ret void }}
define private void @private_fn() {{ ret void }}
declare void @external()
''')
                self.assertIn('data @raw.$name = weak_odr align 8 { i64 7 }', result)
                self.assertIn('ptr @_Z1fv', result)
                self.assertIn('func @_Z1fv() -> ptr cc(c) weak_odr {', result)
                self.assertIn('func @unused() cc(c) local {', result)
                self.assertIn('func @private_fn() cc(c) local {', result)
                self.assertIn('decl @external() cc(c)', result)
                self.assertEqual(result.count('func '), 3)

    def test_virtual_dispatch(self):
        result = self.translate('''@vtable = linkonce_odr constant [3 x ptr]
 [ptr null, ptr null, ptr @method], align 8
define linkonce_odr i64 @method(ptr %this, i64 %x) { ret i64 %x }
define i64 @dispatch(ptr %object, i64 %x) {
 %vt = load ptr, ptr %object, align 8
 %slot = getelementptr ptr, ptr %vt, i64 0
 %method = load ptr, ptr %slot, align 8
 %result = call i64 %method(ptr %object, i64 %x)
 ret i64 %result
}
define ptr @address_point() {
 ret ptr getelementptr ([3 x ptr], ptr @vtable, i64 0, i64 2)
}
''')
        self.assertIn('weak_odr const align 8 { zero 8, zero 8, ptr @method }', result)
        self.assertIn('iconst.i64 16', result)
        self.assertIn('call_indirect', result)

    def test_returned_contract_is_omitted_for_parameters_and_calls(self):
        result = self.translate('''declare ptr @constructor(ptr returned)
define linkonce_odr ptr @wrapper(ptr returned %object) {
 %result = call ptr @constructor(ptr returned %object)
 ret ptr %result
}
define i64 @identity(i64 returned %x) { ret i64 %x }
''')
        self.assertIn('decl @constructor(ptr) -> ptr cc(c)', result)
        self.assertIn('func @wrapper(ptr) -> ptr cc(c) weak_odr', result)
        self.assertIn('func @identity(i64) -> i64 cc(c)', result)
        self.assertNotIn('returned', result)
        for body in [
            'define returned ptr @f(ptr %x) { ret ptr %x }',
            'declare i64 @f(ptr returned)',
            'declare void @f(ptr returned)',
            'declare <2 x i64> @f(<2 x i64> returned)',
            'declare ptr @f(ptr byval(i64) returned)',
            'declare ptr @f(ptr)\ndefine ptr @g(ptr %p) { %r = call returned ptr @f(ptr %p)\nret ptr %r }',
        ]:
            with self.subTest(body=body):
                self.reject(body)

    def test_inrange_vtable_address_arithmetic_and_boundaries(self):
        body = '''@vtable = linkonce_odr constant { [3 x ptr] } zeroinitializer, align 8
@slot = global ptr getelementptr inbounds inrange(-16, 8) ({ [3 x ptr] }, ptr @vtable, i32 0, i32 0, i32 2)
define ptr @set(ptr returned %object) {
 store ptr getelementptr inbounds nuw inrange(-16, 8) (i8, ptr @vtable, i64 16), ptr %object, align 8
 ret ptr %object
}
define ptr @instruction(ptr %object) {
 %p = getelementptr inbounds nuw i8, ptr %object, i64 16
 ret ptr %p
}
'''
        result = self.translate(body)
        self.assertIn('data @slot = align 8 { ptr @vtable + 16 }', result)
        self.assertIn('iconst.i64 16', result)
        self.assertNotIn('inrange', result)
        self.assertNotIn('returned', result)
        external = '@external = external global [0 x i8]\n@p = global ptr getelementptr inrange(0, 1) (i8, ptr @external, i64 {})'
        self.assertIn('ptr @external + 4294967295', self.translate(external.format(0xffffffff)))
        self.reject(external.format(1 << 32), 'unsigned u32')
        self.reject(external.format(-1), 'unsigned u32')
        self.reject(body.replace('i64 16)', 'i64 25)'), 'past defined object end')
        self.assertIn('ptr @vtable + 24', self.translate(body.replace('i32 2)', 'i32 3)')))
        self.reject(body.replace('i32 2)', 'i32 4)'), 'past defined object end')
        self.reject('''@external = external global [0 x i8]
@p = global ptr inttoptr (i64 16 to ptr)''', 'constant GEP')
        self.reject('''@g = global [3 x ptr] zeroinitializer
@p = global ptr getelementptr inrange(-16,8) ([3 x ptr], ptr addrspace(1) null, i64 0, i64 2)''')
        self.reject('''@g = global <2 x i64> zeroinitializer
@p = global ptr getelementptr inrange(0,8) (<2 x i64>, ptr @g, i64 0, i64 0)''')

    def test_defined_rtti_layout_relocations_and_catch(self):
        result = self.translate(RTTI + EH)
        self.assertIn('decl data @class_vtable', result)
        self.assertIn('decl data @si_vtable', result)
        self.assertIn('data @base = weak_odr const align 8 { ptr @class_vtable + 16, ptr @base_name }', result)
        self.assertIn('data @derived = weak_odr const align 8 { ptr @si_vtable + 16, ptr @derived_name, ptr @base }', result)
        self.assertIn('catch @derived', result)
        self.assertIn('i8 66, i8 0', result)

    def test_legacy_mode_remains_bounded(self):
        body = 'define linkonce_odr i64 @f(i64 %x) { ret i64 %x }'
        self.assertIn('cc(c) weak_odr', self.translate(body))
        self.assertIn('cc(c) local', self.translate(body, localize_odr=True))
        self.assertIn('cc(c) local', self.translate(body.replace('linkonce_odr', 'available_externally'), localize_odr=True))
        self.reject(body.replace('linkonce_odr', 'available_externally'))
        self.reject('@p = global ptr @f\n' + body, 'address escape', localize_odr=True)

    def test_hidden_rtti_and_exact_tagged_name_relocations(self):
        result = self.translate(HIDDEN_RTTI + EH.replace('@derived', '@_ZTI1D'))
        self.assertIn('data @_ZTI1B = weak_odr hidden const align 8 { ptr @_ZTVN10__cxxabiv117__class_type_infoE + 16, rtti_name @_ZTS1B }', result)
        self.assertIn('data @_ZTI1D = weak_odr hidden const align 8 { ptr @_ZTVN10__cxxabiv120__si_class_type_infoE + 16, rtti_name @_ZTS1D, ptr @_ZTI1B }', result)
        self.assertIn('data @_ZTS1B = weak_odr hidden const align 1 { i8 49, i8 66, i8 0 }', result)
        self.assertIn('catch @_ZTI1D', result)
        self.assertEqual(result.count('rtti_name '), 2)

    def test_hidden_rtti_is_not_a_generic_visibility_or_integer_escape(self):
        changes = [
            ('-9223372036854775808', '16'),
            ('-9223372036854775808', '-9223372036854775807'),
            ('ptr @_ZTS1B to i64', 'ptr @_ZTS1D to i64'),
            ('@_ZTI1B = linkonce_odr hidden constant', '@_ZTI1B = linkonce_odr hidden global'),
            ('@_ZTS1B = linkonce_odr hidden constant', '@_ZTS1B = linkonce_odr hidden global'),
            ('c"1B\\00"', 'c"1B!"'),
            ('i64 2)', 'i64 1)'),
            ('@_ZTVN10__cxxabiv117__class_type_infoE', '@arbitrary_runtime'),
            ('@_ZTI1B', '@not_typeinfo'),
        ]
        for old, new in changes:
            with self.subTest(change=(old, new)):
                self.reject(HIDDEN_RTTI.replace(old, new))
        expression = 'inttoptr (i64 add (i64 ptrtoint (ptr @_ZTS1B to i64), i64 -9223372036854775808) to ptr)'
        self.reject(HIDDEN_RTTI + '@extra = global ptr ' + expression, 'constant GEP')
        self.reject(HIDDEN_RTTI + 'define ptr @escape() { ret ptr ' + expression + ' }', 'constant GEP')
        self.reject('@_ZTS1B = linkonce_odr hidden constant [3 x i8] c"1B\\00"', 'hidden global')
        self.reject('@_ZTI1B = linkonce_odr hidden constant {ptr,ptr} zeroinitializer', 'hidden global')

    def test_rejected_linkage_and_storage(self):
        cases = [
            'define weak void @f() { ret void }',
            'define linkonce void @f() { ret void }',
            'declare extern_weak void @f()',
            '@g = extern_weak global i64',
            '@g = weak global i64 0',
            '@g = linkonce global i64 0',
            '@g = available_externally global i64 0',
            '@g = common global i64 0',
            '@g = linkonce_odr thread_local global i64 0',
            '@g = weak_odr externally_initialized global i64 0',
            '@g = weak_odr global i64 0, section "x"',
            '@g = linkonce_odr hidden global i64 0',
            '@g = weak_odr protected global i64 0',
            'define weak_odr hidden void @f() { ret void }',
            'define linkonce_odr protected void @f() { ret void }',
            'define weak_odr void @f() section "x" { ret void }',
            '@g = weak_odr global [0 x ptr] zeroinitializer',
        ]
        for selection in ('any', 'exactmatch', 'largest', 'noduplicates', 'samesize'):
            cases.extend([
                f'$c = comdat {selection}\ndefine linkonce_odr void @f() comdat($c) {{ ret void }}',
                f'$c = comdat {selection}\n@g = weak_odr global i64 0, comdat($c)',
            ])
        for body in cases:
            with self.subTest(body=body):
                self.reject(body)

    def test_rtti_does_not_bypass_initializer_checks(self):
        for old, new, diagnostic in [
            ('i64 2)', 'i64 -1)', 'unsigned u32'),
            ('i64 2)', 'i64 536870912)', 'unsigned u32'),
            ('ptr @derived_name', 'ptr inttoptr (i64 42 to ptr)', 'constant GEP'),
        ]:
            self.reject(RTTI.replace(old, new) + EH, diagnostic)
        # Explicit alignment 4 cannot reduce ABI alignment 8.
        self.assertIn('const align 8', self.translate(RTTI.replace('align 8', 'align 4') + EH))
        self.reject('''@g = weak_odr global [2 x ptr] zeroinitializer
@p = global ptr getelementptr (ptr, ptr @g, i64 3)''', 'past defined object end')
        self.reject('''define weak_odr void @f() { ret void }
@p = global ptr getelementptr (i8, ptr @f, i64 1)''', 'function symbol addends')
        self.reject('''@g = weak_odr global <{i8, ptr}> <{i8 0, ptr @external}>
@external = external global i64''', 'pointer relocation requires aligned')
        self.reject('''@g = weak_odr global [2 x ptr] zeroinitializer
@p = global ptr getelementptr inrange(0, 8) ([2 x ptr], ptr @g, i64 0, i64 3)''', 'past defined object end')

    def test_lifetime_markers_keep_fixed_slots_without_runtime_calls(self):
        result = self.translate(LIFETIMES)
        self.assertIn('stack_slot 8, align 8', result)
        self.assertIn('decl @llvm.lifetime.start.p0(ptr)', result)
        self.assertNotIn('call @llvm.lifetime.', result)

    def test_lifetime_markers_reject_unproven_storage(self):
        # Exercise LLVM 23's canonical form directly: its legacy upgrader may
        # delete old size+pointer markers on non-alloca storage as no-ops.
        declarations = ('declare void @llvm.lifetime.start.p0(ptr)\n'
                        'declare void @llvm.lifetime.end.p0(ptr)\n')
        for body in (
            'define void @f(ptr %p) { call void @llvm.lifetime.start.p0(i64 8, ptr %p)\nret void }',
            '@g = global i64 0\ndefine void @f() { call void @llvm.lifetime.end.p0(i64 8, ptr @g)\nret void }',
            'define void @f() { call void @llvm.lifetime.start.p0(i64 8, ptr null)\nret void }',
            'define void @f() { %p = alloca [2 x i64]\n%q = getelementptr i64, ptr %p, i64 1\ncall void @llvm.lifetime.start.p0(i64 8, ptr %q)\nret void }',
        ):
            with self.subTest(body=body):
                self.reject(declarations + body.replace('i64 8, ', ''))

    def test_lifetime_restart_and_masked_poison_native(self):
        if not ARGS.keel:
            self.skipTest('pass --keel for native lifetime assertions')
        with tempfile.TemporaryDirectory(prefix='keel-lifetime-native-') as tmp:
            root = Path(tmp)
            source, text = root / 'lifetimes.ll', root / 'lifetimes.keel'
            original = HEADER + LIFETIMES.encode()
            source.write_bytes(original)
            text.write_text(import_module(self.llvm, original))
            driver = root / 'driver.c'
            driver.write_text('''#include <assert.h>
#include <stdint.h>
extern uint64_t lifetime_roundtrip(uint64_t);
int main(void) {
 uint64_t values[] = {0, 1, UINT64_MAX, UINT64_C(0x8000000000000000),
                      UINT64_C(0x7fffffffffffffff), UINT64_C(0x8000000000000001)};
 for (unsigned i=0;i<sizeof(values)/sizeof(values[0]);i++) {
  uint64_t x=values[i]; assert(lifetime_roundtrip(x)==(x^(x+1)));
 }
 for (uint64_t x=0;x<1024;x++) assert(lifetime_roundtrip(x)==(x^(x+1)));
}
''')
            def run(command):
                result = subprocess.run(list(map(str, command)), capture_output=True, timeout=90)
                self.assertEqual(result.returncode, 0, result.stderr)
            for tier in ('reference', 'none', 'fast', 'full'):
                obj, exe = root / (tier + '.o'), root / tier
                if tier == 'reference':
                    run(['clang', '-O0', '-c', source, '-o', obj])
                else:
                    run([ARGS.keel, 'compile', '--opt-tier', tier, '--emit-object', text, '-o', obj])
                run(['clang', '-O0', driver, obj, '-o', exe])
                run([exe])
            self.assertEqual(source.read_bytes(), original)

    def test_real_cli_atomicity_and_preservation(self):
        if not ARGS.keel:
            self.skipTest('pass --keel for real verifier and CLI checks')
        with tempfile.TemporaryDirectory(prefix='keel-odr-policy-') as tmp:
            root = Path(tmp)
            source, dest = root / 'input.ll', root / 'output.keel'
            source.write_bytes(HEADER + b'define void @control() { ret void }')
            command = [sys.executable, str(Path(__file__).with_name('llvm_import.py')),
                       '--libllvm', ARGS.libllvm, '--keel', ARGS.keel, str(source), '-o', str(dest)]
            control = subprocess.run(command, capture_output=True)
            self.assertEqual(control.returncode, 0, control.stderr)
            # This checks the removed defined-typeinfo gate with today's real
            # verifier, independently of the future weak_odr syntax.
            strong_rtti = HEADER + (RTTI + EH).replace('linkonce_odr ', '').replace('weak_odr ', '').encode()
            source.write_bytes(strong_rtti)
            control = subprocess.run(command, capture_output=True)
            self.assertEqual(control.returncode, 0, control.stderr)
            self.assertEqual(source.read_bytes(), strong_rtti)
            self.assertIn(b'catch @derived', dest.read_bytes())
            strong_vtable = HEADER + b'''@vtable = constant [3 x ptr] zeroinitializer, align 8
define ptr @constructor(ptr returned %object) {
 store ptr getelementptr inbounds inrange(-16,8) ([3 x ptr], ptr @vtable, i64 0, i64 2), ptr %object, align 8
 ret ptr %object
}
'''
            source.write_bytes(strong_vtable)
            control = subprocess.run(command, capture_output=True)
            self.assertEqual(control.returncode, 0, control.stderr)
            self.assertEqual(source.read_bytes(), strong_vtable)
            self.assertNotIn(b'returned', dest.read_bytes())
            self.assertNotIn(b'inrange', dest.read_bytes())
            bodies = [('unsupported', '@g = weak global i64 0', False),
                      ('late', 'define weak_odr i64 @f(i64 %x) { %r = freeze i128 poison\nret i64 %x }', False),
                      ('odr', RTTI + EH, ARGS.verify_weak_odr),
                      ('tagged-odr', HIDDEN_RTTI + EH.replace('@derived', '@_ZTI1D'), ARGS.verify_weak_odr)]
            for name, body, success in bodies:
                with self.subTest(name=name):
                    original = HEADER + body.encode()
                    source.write_bytes(original)
                    dest.write_bytes(b'preserve existing destination\n')
                    result = subprocess.run(command, capture_output=True)
                    self.assertEqual(source.read_bytes(), original)
                    self.assertEqual(result.returncode == 0, success, result.stderr)
                    if success:
                        self.assertIn(b'weak_odr', dest.read_bytes())
                    else:
                        self.assertEqual(dest.read_bytes(), b'preserve existing destination\n')
                        self.assertEqual(result.stdout, b'')
                        self.assertEqual(list(root.glob('.llvm-import-*')), [])
                        if name in ('odr', 'tagged-odr'):
                            self.assertIn(b'Keel verification failed:', result.stderr)
                        dest.unlink()
                        retry = subprocess.run(command, capture_output=True)
                        self.assertNotEqual(retry.returncode, 0)
                        self.assertFalse(dest.exists())
                    # Test the stdout publication path and stdin input as well.
                    stdout_command = command[:-2]
                    stdout_command[-1] = '-'
                    result = subprocess.run(stdout_command, input=original, capture_output=True)
                    self.assertEqual(result.returncode == 0, success, result.stderr)
                    if not success:
                        self.assertEqual(result.stdout, b'')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--libllvm', required=True)
    parser.add_argument('--keel')
    parser.add_argument('--verify-weak-odr', action='store_true')
    ARGS, remaining = parser.parse_known_args()
    unittest.main(argv=[sys.argv[0], *remaining])
