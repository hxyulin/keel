#!/usr/bin/env python3
"""Keep Rust/Python parity normalization limited to the new retained facts."""
import unittest

from test_llvm_import_rust import without_preserved_facts


class PreservedFactsNormalization(unittest.TestCase):
    def test_strips_function_header_attributes_and_access_alignment(self):
        source = ('decl @callee() nounwind\n'
                  'func @f(ptr) noinline optnone cold {\n'
                  'block0(v0: ptr):\n'
                  '    v1 = load.i64 volatile align 32 v0\n'
                  '    store align 16 v1, v0\n'
                  '}\n')
        expected = ('decl @callee()\n'
                    'func @f(ptr) {\n'
                    'block0(v0: ptr):\n'
                    '    v1 = load.i64 volatile v0\n'
                    '    store v1, v0\n'
                    '}\n')
        self.assertEqual(without_preserved_facts(source), expected)

    def test_keeps_global_stack_and_other_instruction_properties(self):
        source = ('data @nounwind = align 32 { i64 0 }\n'
                  'decl data @cold\n'
                  'func @hot(ptr) align 16 optimize("hot cold") {\n'
                  '    ss0 = stack_slot 8, align 8\n'
                  'block0(v0: ptr):\n'
                  '    v1 = symbol_addr @nounwind\n'
                  '    v2 = load.i64 volatile notrap v0\n'
                  '    return v2\n'
                  '}\n')
        self.assertEqual(without_preserved_facts(source), source)

    def test_strips_only_new_contract_and_lifetime_transport(self):
        source = ('func @f(ptr) contract(memory(argmem: read), param(0, nonnull)) restrict_params(0) {\n'
                  'block0(v0: ptr):\n'
                  '    intrinsic lifetime_start(v0)\n'
                  '    call @external(v0) contract(nofree, param(0, align 8))\n'
                  '    intrinsic lifetime_end(v0)\n'
                  '    return\n}\n')
        expected = ('func @f(ptr) {\nblock0(v0: ptr):\n'
                    '    call @external(v0)\n    return\n}\n')
        self.assertEqual(without_preserved_facts(source), expected)

    def test_keeps_quoted_contracts_and_non_call_operands(self):
        source = ('func @f() optimize(" contract(memory(none))") {\n'
                  'block0:\n    v0 = symbol_addr @contract\n    return\n}\n')
        self.assertEqual(without_preserved_facts(source), source)


if __name__ == '__main__':
    unittest.main()
