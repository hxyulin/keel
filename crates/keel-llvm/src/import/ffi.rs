//! Private LLVM C API boundary. Handles stay inside an import session.
use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
};
type Raw = *mut c_void;
pub(super) type Ref = usize;
pub(super) struct Api {
    get_version: unsafe extern "C" fn(*mut u32, *mut u32, *mut u32) -> (),
    context_create: unsafe extern "C" fn() -> Raw,
    context_dispose: unsafe extern "C" fn(Raw) -> (),
    create_memory_buffer_with_memory_range_copy:
        unsafe extern "C" fn(*const c_char, usize, *const c_char) -> Raw,
    parse_ir_in_context: unsafe extern "C" fn(Raw, Raw, *mut Raw, *mut Raw) -> i32,
    verify_module: unsafe extern "C" fn(Raw, i32, *mut Raw) -> i32,
    dispose_message: unsafe extern "C" fn(Raw) -> (),
    dispose_module: unsafe extern "C" fn(Raw) -> (),
    get_target: unsafe extern "C" fn(Raw) -> *const c_char,
    get_data_layout_str: unsafe extern "C" fn(Raw) -> *const c_char,
    get_module_inline_asm: unsafe extern "C" fn(Raw, *mut usize) -> Raw,
    get_first_function: unsafe extern "C" fn(Raw) -> Raw,
    get_next_function: unsafe extern "C" fn(Raw) -> Raw,
    get_first_global: unsafe extern "C" fn(Raw) -> Raw,
    get_first_global_alias: unsafe extern "C" fn(Raw) -> Raw,
    get_first_global_i_func: unsafe extern "C" fn(Raw) -> Raw,
    get_value_name2: unsafe extern "C" fn(Raw, *mut usize) -> Raw,
    get_named_metadata: unsafe extern "C" fn(Raw, *const c_char, usize) -> Raw,
    print_value_to_string: unsafe extern "C" fn(Raw) -> Raw,
    global_get_value_type: unsafe extern "C" fn(Raw) -> Raw,
    type_of: unsafe extern "C" fn(Raw) -> Raw,
    get_type_kind: unsafe extern "C" fn(Raw) -> u32,
    get_int_type_width: unsafe extern "C" fn(Raw) -> u32,
    get_type_context: unsafe extern "C" fn(Raw) -> Raw,
    int_type_in_context: unsafe extern "C" fn(Raw, u32) -> Raw,
    is_a_constant_fp: unsafe extern "C" fn(Raw) -> Raw,
    is_a_insert_value_inst: unsafe extern "C" fn(Raw) -> Raw,
    const_bit_cast: unsafe extern "C" fn(Raw, Raw) -> Raw,
    get_f_cmp_predicate: unsafe extern "C" fn(Raw) -> u32,
    get_fast_math_flags: unsafe extern "C" fn(Raw) -> u32,
    can_value_use_fast_math_flags: unsafe extern "C" fn(Raw) -> i32,
    get_pointer_address_space: unsafe extern "C" fn(Raw) -> u32,
    get_return_type: unsafe extern "C" fn(Raw) -> Raw,
    is_function_var_arg: unsafe extern "C" fn(Raw) -> i32,
    get_function_call_conv: unsafe extern "C" fn(Raw) -> u32,
    get_instruction_call_conv: unsafe extern "C" fn(Raw) -> u32,
    get_linkage: unsafe extern "C" fn(Raw) -> u32,
    get_visibility: unsafe extern "C" fn(Raw) -> u32,
    has_personality_fn: unsafe extern "C" fn(Raw) -> i32,
    get_personality_fn: unsafe extern "C" fn(Raw) -> Raw,
    get_first_use: unsafe extern "C" fn(Raw) -> Raw,
    get_next_use: unsafe extern "C" fn(Raw) -> Raw,
    get_user: unsafe extern "C" fn(Raw) -> Raw,
    get_num_clauses: unsafe extern "C" fn(Raw) -> u32,
    get_clause: unsafe extern "C" fn(Raw, u32) -> Raw,
    is_cleanup: unsafe extern "C" fn(Raw) -> i32,
    get_num_indices: unsafe extern "C" fn(Raw) -> u32,
    get_indices: unsafe extern "C" fn(Raw) -> *mut u32,
    get_gc: unsafe extern "C" fn(Raw) -> *const c_char,
    get_section: unsafe extern "C" fn(Raw) -> *const c_char,
    get_comdat: unsafe extern "C" fn(Raw) -> Raw,
    has_prefix_data: unsafe extern "C" fn(Raw) -> i32,
    has_prologue_data: unsafe extern "C" fn(Raw) -> i32,
    get_dll_storage_class: unsafe extern "C" fn(Raw) -> u32,
    get_num_operand_bundles: unsafe extern "C" fn(Raw) -> u32,
    get_alignment: unsafe extern "C" fn(Raw) -> u32,
    count_params: unsafe extern "C" fn(Raw) -> u32,
    get_param: unsafe extern "C" fn(Raw, u32) -> Raw,
    get_enum_attribute_kind_for_name: unsafe extern "C" fn(*const c_char, usize) -> u32,
    get_enum_attribute_at_index: unsafe extern "C" fn(Raw, u32, u32) -> Raw,
    get_call_site_enum_attribute: unsafe extern "C" fn(Raw, u32, u32) -> Raw,
    get_attribute_count_at_index: unsafe extern "C" fn(Raw, u32) -> u32,
    get_attributes_at_index: unsafe extern "C" fn(Raw, u32, *mut Raw) -> (),
    get_call_site_attribute_count: unsafe extern "C" fn(Raw, u32) -> u32,
    get_call_site_attributes: unsafe extern "C" fn(Raw, u32, *mut Raw) -> (),
    is_string_attribute: unsafe extern "C" fn(Raw) -> i32,
    get_enum_attribute_kind: unsafe extern "C" fn(Raw) -> u32,
    get_enum_attribute_value: unsafe extern "C" fn(Raw) -> u64,
    get_type_attribute_value: unsafe extern "C" fn(Raw) -> Raw,
    count_param_types: unsafe extern "C" fn(Raw) -> u32,
    get_param_types: unsafe extern "C" fn(Raw, *mut Raw) -> (),
    is_packed_struct: unsafe extern "C" fn(Raw) -> i32,
    get_string_attribute_kind: unsafe extern "C" fn(Raw, *mut u32) -> Raw,
    get_first_basic_block: unsafe extern "C" fn(Raw) -> Raw,
    get_next_basic_block: unsafe extern "C" fn(Raw) -> Raw,
    get_first_instruction: unsafe extern "C" fn(Raw) -> Raw,
    get_next_instruction: unsafe extern "C" fn(Raw) -> Raw,
    get_instruction_opcode: unsafe extern "C" fn(Raw) -> u32,
    get_num_operands: unsafe extern "C" fn(Raw) -> i32,
    get_operand: unsafe extern "C" fn(Raw, u32) -> Raw,
    is_a_constant_int: unsafe extern "C" fn(Raw) -> Raw,
    is_a_constant_pointer_null: unsafe extern "C" fn(Raw) -> Raw,
    is_a_function: unsafe extern "C" fn(Raw) -> Raw,
    is_a_instruction: unsafe extern "C" fn(Raw) -> Raw,
    const_int_get_z_ext_value: unsafe extern "C" fn(Raw) -> u64,
    const_int_get_s_ext_value: unsafe extern "C" fn(Raw) -> i64,
    get_i_cmp_predicate: unsafe extern "C" fn(Raw) -> u32,
    get_called_value: unsafe extern "C" fn(Raw) -> Raw,
    get_called_function_type: unsafe extern "C" fn(Raw) -> Raw,
    get_tail_call_kind: unsafe extern "C" fn(Raw) -> u32,
    get_num_arg_operands: unsafe extern "C" fn(Raw) -> u32,
    get_volatile: unsafe extern "C" fn(Raw) -> i32,
    get_ordering: unsafe extern "C" fn(Raw) -> u32,
    _get_nuw: unsafe extern "C" fn(Raw) -> i32,
    _get_nsw: unsafe extern "C" fn(Raw) -> i32,
    _get_exact: unsafe extern "C" fn(Raw) -> i32,
    create_target_data: unsafe extern "C" fn(*const c_char) -> Raw,
    dispose_target_data: unsafe extern "C" fn(Raw) -> (),
    byte_order: unsafe extern "C" fn(Raw) -> u32,
    pointer_size: unsafe extern "C" fn(Raw) -> u32,
    count_incoming: unsafe extern "C" fn(Raw) -> u32,
    get_incoming_value: unsafe extern "C" fn(Raw, u32) -> Raw,
    get_incoming_block: unsafe extern "C" fn(Raw, u32) -> Raw,
    get_num_successors: unsafe extern "C" fn(Raw) -> u32,
    get_successor: unsafe extern "C" fn(Raw, u32) -> Raw,
    get_condition: unsafe extern "C" fn(Raw) -> Raw,
    get_switch_case_value: unsafe extern "C" fn(Raw, u32) -> Raw,
    get_gep_source_element_type: unsafe extern "C" fn(Raw) -> Raw,
    get_allocated_type: unsafe extern "C" fn(Raw) -> Raw,
    get_element_type: unsafe extern "C" fn(Raw) -> Raw,
    get_array_length2: unsafe extern "C" fn(Raw) -> u64,
    is_opaque_struct: unsafe extern "C" fn(Raw) -> i32,
    count_struct_element_types: unsafe extern "C" fn(Raw) -> u32,
    struct_get_type_at_index: unsafe extern "C" fn(Raw, u32) -> Raw,
    abi_size_of_type: unsafe extern "C" fn(Raw, Raw) -> u64,
    abi_alignment_of_type: unsafe extern "C" fn(Raw, Raw) -> u32,
    offset_of_element: unsafe extern "C" fn(Raw, Raw, u32) -> u64,
    get_next_global: unsafe extern "C" fn(Raw) -> Raw,
    get_initializer: unsafe extern "C" fn(Raw) -> Raw,
    is_global_constant: unsafe extern "C" fn(Raw) -> i32,
    is_thread_local: unsafe extern "C" fn(Raw) -> i32,
    is_externally_initialized: unsafe extern "C" fn(Raw) -> i32,
    is_a_constant_aggregate_zero: unsafe extern "C" fn(Raw) -> Raw,
    is_a_constant_expr: unsafe extern "C" fn(Raw) -> Raw,
    get_const_opcode: unsafe extern "C" fn(Raw) -> u32,
    get_aggregate_element: unsafe extern "C" fn(Raw, u32) -> Raw,
    is_a_call_inst: unsafe extern "C" fn(Raw) -> Raw,
    is_a_constant: unsafe extern "C" fn(Raw) -> Raw,
    is_a_global_variable: unsafe extern "C" fn(Raw) -> Raw,
    _library: libloading::Library,
}
impl Api {
    pub(super) fn load(path: &Path) -> Result<Self, String> {
        // SAFETY: the explicitly supplied path must name the trusted LLVM library.
        let library = unsafe { libloading::Library::new(path) }.map_err(|e| e.to_string())?;
        // SAFETY: these signatures match LLVM 23's C API, as in the Python importer.
        let api = Self {
            get_version: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetVersion\0")
                        .map_err(|e| e.to_string())?
                }
            },
            context_create: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMContextCreate\0")
                        .map_err(|e| e.to_string())?
                }
            },
            context_dispose: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMContextDispose\0")
                        .map_err(|e| e.to_string())?
                }
            },
            create_memory_buffer_with_memory_range_copy: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMCreateMemoryBufferWithMemoryRangeCopy\0")
                        .map_err(|e| e.to_string())?
                }
            },
            parse_ir_in_context: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMParseIRInContext\0")
                        .map_err(|e| e.to_string())?
                }
            },
            verify_module: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMVerifyModule\0")
                        .map_err(|e| e.to_string())?
                }
            },
            dispose_message: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMDisposeMessage\0")
                        .map_err(|e| e.to_string())?
                }
            },
            dispose_module: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMDisposeModule\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_target: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetTarget\0").map_err(|e| e.to_string())? }
            },
            get_data_layout_str: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetDataLayoutStr\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_module_inline_asm: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetModuleInlineAsm\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_first_function: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFirstFunction\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_next_function: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNextFunction\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_first_global: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFirstGlobal\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_first_global_alias: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFirstGlobalAlias\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_first_global_i_func: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFirstGlobalIFunc\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_value_name2: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetValueName2\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_named_metadata: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNamedMetadata\0")
                        .map_err(|e| e.to_string())?
                }
            },
            print_value_to_string: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMPrintValueToString\0")
                        .map_err(|e| e.to_string())?
                }
            },
            global_get_value_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGlobalGetValueType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            type_of: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMTypeOf\0").map_err(|e| e.to_string())? }
            },
            get_type_kind: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetTypeKind\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_int_type_width: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetIntTypeWidth\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_type_context: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetTypeContext\0")
                        .map_err(|e| e.to_string())?
                }
            },
            int_type_in_context: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIntTypeInContext\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_constant_fp: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAConstantFP\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_insert_value_inst: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAInsertValueInst\0")
                        .map_err(|e| e.to_string())?
                }
            },
            const_bit_cast: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMConstBitCast\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_f_cmp_predicate: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFCmpPredicate\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_fast_math_flags: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFastMathFlags\0")
                        .map_err(|e| e.to_string())?
                }
            },
            can_value_use_fast_math_flags: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMCanValueUseFastMathFlags\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_pointer_address_space: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetPointerAddressSpace\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_return_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetReturnType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_function_var_arg: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsFunctionVarArg\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_function_call_conv: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFunctionCallConv\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_instruction_call_conv: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetInstructionCallConv\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_linkage: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetLinkage\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_visibility: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetVisibility\0")
                        .map_err(|e| e.to_string())?
                }
            },
            has_personality_fn: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMHasPersonalityFn\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_personality_fn: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetPersonalityFn\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_first_use: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFirstUse\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_next_use: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNextUse\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_user: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetUser\0").map_err(|e| e.to_string())? }
            },
            get_num_clauses: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNumClauses\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_clause: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetClause\0").map_err(|e| e.to_string())? }
            },
            is_cleanup: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMIsCleanup\0").map_err(|e| e.to_string())? }
            },
            get_num_indices: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNumIndices\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_indices: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetIndices\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_gc: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetGC\0").map_err(|e| e.to_string())? }
            },
            get_section: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetSection\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_comdat: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetComdat\0").map_err(|e| e.to_string())? }
            },
            has_prefix_data: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMHasPrefixData\0")
                        .map_err(|e| e.to_string())?
                }
            },
            has_prologue_data: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMHasPrologueData\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_dll_storage_class: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetDLLStorageClass\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_num_operand_bundles: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNumOperandBundles\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_alignment: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetAlignment\0")
                        .map_err(|e| e.to_string())?
                }
            },
            count_params: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMCountParams\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_param: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetParam\0").map_err(|e| e.to_string())? }
            },
            get_enum_attribute_kind_for_name: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetEnumAttributeKindForName\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_enum_attribute_at_index: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetEnumAttributeAtIndex\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_call_site_enum_attribute: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetCallSiteEnumAttribute\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_attribute_count_at_index: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetAttributeCountAtIndex\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_attributes_at_index: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetAttributesAtIndex\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_call_site_attribute_count: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetCallSiteAttributeCount\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_call_site_attributes: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetCallSiteAttributes\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_string_attribute: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsStringAttribute\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_enum_attribute_value: {
                // SAFETY: this is LLVM 23's enum attribute integer accessor.
                unsafe {
                    *library
                        .get(b"LLVMGetEnumAttributeValue\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_enum_attribute_kind: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetEnumAttributeKind\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_type_attribute_value: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetTypeAttributeValue\0")
                        .map_err(|e| e.to_string())?
                }
            },
            count_param_types: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMCountParamTypes\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_param_types: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetParamTypes\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_packed_struct: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsPackedStruct\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_string_attribute_kind: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetStringAttributeKind\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_first_basic_block: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFirstBasicBlock\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_next_basic_block: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNextBasicBlock\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_first_instruction: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetFirstInstruction\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_next_instruction: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNextInstruction\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_instruction_opcode: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetInstructionOpcode\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_num_operands: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNumOperands\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_operand: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetOperand\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_constant_int: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAConstantInt\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_constant_pointer_null: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAConstantPointerNull\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_function: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAFunction\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_instruction: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAInstruction\0")
                        .map_err(|e| e.to_string())?
                }
            },
            const_int_get_z_ext_value: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMConstIntGetZExtValue\0")
                        .map_err(|e| e.to_string())?
                }
            },
            const_int_get_s_ext_value: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMConstIntGetSExtValue\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_i_cmp_predicate: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetICmpPredicate\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_called_value: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetCalledValue\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_called_function_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetCalledFunctionType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_tail_call_kind: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetTailCallKind\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_num_arg_operands: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNumArgOperands\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_volatile: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetVolatile\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_ordering: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetOrdering\0")
                        .map_err(|e| e.to_string())?
                }
            },
            _get_nuw: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetNUW\0").map_err(|e| e.to_string())? }
            },
            _get_nsw: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetNSW\0").map_err(|e| e.to_string())? }
            },
            _get_exact: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMGetExact\0").map_err(|e| e.to_string())? }
            },
            create_target_data: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMCreateTargetData\0")
                        .map_err(|e| e.to_string())?
                }
            },
            dispose_target_data: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMDisposeTargetData\0")
                        .map_err(|e| e.to_string())?
                }
            },
            byte_order: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe { *library.get(b"LLVMByteOrder\0").map_err(|e| e.to_string())? }
            },
            pointer_size: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMPointerSize\0")
                        .map_err(|e| e.to_string())?
                }
            },
            count_incoming: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMCountIncoming\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_incoming_value: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetIncomingValue\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_incoming_block: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetIncomingBlock\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_num_successors: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNumSuccessors\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_successor: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetSuccessor\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_condition: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetCondition\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_switch_case_value: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetSwitchCaseValue\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_gep_source_element_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetGEPSourceElementType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_allocated_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetAllocatedType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_element_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetElementType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_array_length2: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetArrayLength2\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_opaque_struct: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsOpaqueStruct\0")
                        .map_err(|e| e.to_string())?
                }
            },
            count_struct_element_types: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMCountStructElementTypes\0")
                        .map_err(|e| e.to_string())?
                }
            },
            struct_get_type_at_index: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMStructGetTypeAtIndex\0")
                        .map_err(|e| e.to_string())?
                }
            },
            abi_size_of_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMABISizeOfType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            abi_alignment_of_type: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMABIAlignmentOfType\0")
                        .map_err(|e| e.to_string())?
                }
            },
            offset_of_element: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMOffsetOfElement\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_next_global: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetNextGlobal\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_initializer: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetInitializer\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_global_constant: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsGlobalConstant\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_thread_local: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsThreadLocal\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_externally_initialized: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsExternallyInitialized\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_constant_aggregate_zero: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAConstantAggregateZero\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_constant_expr: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAConstantExpr\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_const_opcode: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetConstOpcode\0")
                        .map_err(|e| e.to_string())?
                }
            },
            get_aggregate_element: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMGetAggregateElement\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_call_inst: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsACallInst\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_constant: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAConstant\0")
                        .map_err(|e| e.to_string())?
                }
            },
            is_a_global_variable: {
                // SAFETY: the symbol uses its LLVM 23 C API signature and the library stays loaded.
                unsafe {
                    *library
                        .get(b"LLVMIsAGlobalVariable\0")
                        .map_err(|e| e.to_string())?
                }
            },
            _library: library,
        };
        let mut version = [0; 3];
        // SAFETY: all version outputs point to valid writable u32 values.
        unsafe {
            (api.get_version)(&mut version[0], &mut version[1], &mut version[2]);
        }
        if version[0] != 23 {
            return Err(format!("validated LLVM major is 23; loaded {version:?}"));
        }
        Ok(api)
    }
    pub(super) fn get_first_function(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_first_function)(a0 as Raw) as Ref }
    }
    pub(super) fn get_next_function(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_next_function)(a0 as Raw) as Ref }
    }
    pub(super) fn get_first_global(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_first_global)(a0 as Raw) as Ref }
    }
    pub(super) fn get_first_global_alias(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_first_global_alias)(a0 as Raw) as Ref }
    }
    pub(super) fn get_first_global_i_func(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_first_global_i_func)(a0 as Raw) as Ref }
    }
    pub(super) fn global_get_value_type(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.global_get_value_type)(a0 as Raw) as Ref }
    }
    pub(super) fn type_of(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.type_of)(a0 as Raw) as Ref }
    }
    pub(super) fn get_type_kind(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_type_kind)(a0 as Raw) }
    }
    pub(super) fn get_int_type_width(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_int_type_width)(a0 as Raw) }
    }
    pub(super) fn get_type_context(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_type_context)(a0 as Raw) as Ref }
    }
    pub(super) fn int_type_in_context(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.int_type_in_context)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn is_a_constant_fp(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_constant_fp)(a0 as Raw) as Ref }
    }
    pub(super) fn is_a_insert_value_inst(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_insert_value_inst)(a0 as Raw) as Ref }
    }
    pub(super) fn const_bit_cast(&self, a0: Ref, a1: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.const_bit_cast)(a0 as Raw, a1 as Raw) as Ref }
    }
    pub(super) fn get_f_cmp_predicate(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_f_cmp_predicate)(a0 as Raw) }
    }
    pub(super) fn get_fast_math_flags(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_fast_math_flags)(a0 as Raw) }
    }
    pub(super) fn can_value_use_fast_math_flags(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.can_value_use_fast_math_flags)(a0 as Raw) }
    }
    pub(super) fn get_pointer_address_space(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_pointer_address_space)(a0 as Raw) }
    }
    pub(super) fn get_return_type(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_return_type)(a0 as Raw) as Ref }
    }
    pub(super) fn is_function_var_arg(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_function_var_arg)(a0 as Raw) }
    }
    pub(super) fn get_function_call_conv(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_function_call_conv)(a0 as Raw) }
    }
    pub(super) fn get_instruction_call_conv(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_instruction_call_conv)(a0 as Raw) }
    }
    pub(super) fn get_linkage(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_linkage)(a0 as Raw) }
    }
    pub(super) fn get_visibility(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_visibility)(a0 as Raw) }
    }
    pub(super) fn has_personality_fn(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.has_personality_fn)(a0 as Raw) }
    }
    pub(super) fn get_personality_fn(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_personality_fn)(a0 as Raw) as Ref }
    }
    pub(super) fn get_first_use(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_first_use)(a0 as Raw) as Ref }
    }
    pub(super) fn get_next_use(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_next_use)(a0 as Raw) as Ref }
    }
    pub(super) fn get_user(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_user)(a0 as Raw) as Ref }
    }
    pub(super) fn get_num_clauses(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_num_clauses)(a0 as Raw) }
    }
    pub(super) fn get_clause(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_clause)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn is_cleanup(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_cleanup)(a0 as Raw) }
    }
    pub(super) fn get_num_indices(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_num_indices)(a0 as Raw) }
    }
    pub(super) fn get_comdat(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_comdat)(a0 as Raw) as Ref }
    }
    pub(super) fn has_prefix_data(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.has_prefix_data)(a0 as Raw) }
    }
    pub(super) fn has_prologue_data(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.has_prologue_data)(a0 as Raw) }
    }
    pub(super) fn get_dll_storage_class(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_dll_storage_class)(a0 as Raw) }
    }
    pub(super) fn get_num_operand_bundles(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_num_operand_bundles)(a0 as Raw) }
    }
    pub(super) fn get_alignment(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_alignment)(a0 as Raw) }
    }
    pub(super) fn count_params(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.count_params)(a0 as Raw) }
    }
    pub(super) fn get_param(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_param)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn get_enum_attribute_at_index(&self, a0: Ref, a1: u32, a2: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_enum_attribute_at_index)(a0 as Raw, a1, a2) as Ref }
    }
    pub(super) fn get_call_site_enum_attribute(&self, a0: Ref, a1: u32, a2: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_call_site_enum_attribute)(a0 as Raw, a1, a2) as Ref }
    }
    pub(super) fn get_attribute_count_at_index(&self, a0: Ref, a1: u32) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_attribute_count_at_index)(a0 as Raw, a1) }
    }
    pub(super) fn get_call_site_attribute_count(&self, a0: Ref, a1: u32) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_call_site_attribute_count)(a0 as Raw, a1) }
    }
    pub(super) fn is_string_attribute(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_string_attribute)(a0 as Raw) }
    }
    pub(super) fn get_enum_attribute_value(&self, attribute: Ref) -> u64 {
        // SAFETY: attribute was returned by LLVM's enum attribute lookup for the live module.
        unsafe { (self.get_enum_attribute_value)(attribute as Raw) }
    }
    pub(super) fn get_enum_attribute_kind(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_enum_attribute_kind)(a0 as Raw) }
    }
    pub(super) fn get_type_attribute_value(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_type_attribute_value)(a0 as Raw) as Ref }
    }
    pub(super) fn count_param_types(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.count_param_types)(a0 as Raw) }
    }
    pub(super) fn is_packed_struct(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_packed_struct)(a0 as Raw) }
    }
    pub(super) fn get_first_basic_block(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_first_basic_block)(a0 as Raw) as Ref }
    }
    pub(super) fn get_next_basic_block(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_next_basic_block)(a0 as Raw) as Ref }
    }
    pub(super) fn get_first_instruction(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_first_instruction)(a0 as Raw) as Ref }
    }
    pub(super) fn get_next_instruction(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_next_instruction)(a0 as Raw) as Ref }
    }
    pub(super) fn get_instruction_opcode(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_instruction_opcode)(a0 as Raw) }
    }
    pub(super) fn get_num_operands(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_num_operands)(a0 as Raw) }
    }
    pub(super) fn get_operand(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_operand)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn is_a_constant_int(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_constant_int)(a0 as Raw) as Ref }
    }
    pub(super) fn is_a_constant_pointer_null(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_constant_pointer_null)(a0 as Raw) as Ref }
    }
    pub(super) fn is_a_function(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_function)(a0 as Raw) as Ref }
    }
    pub(super) fn is_a_instruction(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_instruction)(a0 as Raw) as Ref }
    }
    pub(super) fn const_int_get_z_ext_value(&self, a0: Ref) -> u64 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.const_int_get_z_ext_value)(a0 as Raw) }
    }
    pub(super) fn const_int_get_s_ext_value(&self, a0: Ref) -> i64 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.const_int_get_s_ext_value)(a0 as Raw) }
    }
    pub(super) fn get_i_cmp_predicate(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_i_cmp_predicate)(a0 as Raw) }
    }
    pub(super) fn get_called_value(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_called_value)(a0 as Raw) as Ref }
    }
    pub(super) fn get_called_function_type(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_called_function_type)(a0 as Raw) as Ref }
    }
    pub(super) fn get_tail_call_kind(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_tail_call_kind)(a0 as Raw) }
    }
    pub(super) fn get_num_arg_operands(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_num_arg_operands)(a0 as Raw) }
    }
    pub(super) fn get_volatile(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_volatile)(a0 as Raw) }
    }
    pub(super) fn get_ordering(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_ordering)(a0 as Raw) }
    }
    pub(super) fn byte_order(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.byte_order)(a0 as Raw) }
    }
    pub(super) fn pointer_size(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.pointer_size)(a0 as Raw) }
    }
    pub(super) fn count_incoming(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.count_incoming)(a0 as Raw) }
    }
    pub(super) fn get_incoming_value(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_incoming_value)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn get_incoming_block(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_incoming_block)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn get_num_successors(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_num_successors)(a0 as Raw) }
    }
    pub(super) fn get_successor(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_successor)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn get_condition(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_condition)(a0 as Raw) as Ref }
    }
    pub(super) fn get_switch_case_value(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_switch_case_value)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn get_gep_source_element_type(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_gep_source_element_type)(a0 as Raw) as Ref }
    }
    pub(super) fn get_allocated_type(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_allocated_type)(a0 as Raw) as Ref }
    }
    pub(super) fn get_element_type(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_element_type)(a0 as Raw) as Ref }
    }
    pub(super) fn get_array_length2(&self, a0: Ref) -> u64 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_array_length2)(a0 as Raw) }
    }
    pub(super) fn is_opaque_struct(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_opaque_struct)(a0 as Raw) }
    }
    pub(super) fn count_struct_element_types(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.count_struct_element_types)(a0 as Raw) }
    }
    pub(super) fn struct_get_type_at_index(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.struct_get_type_at_index)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn abi_size_of_type(&self, a0: Ref, a1: Ref) -> u64 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.abi_size_of_type)(a0 as Raw, a1 as Raw) }
    }
    pub(super) fn abi_alignment_of_type(&self, a0: Ref, a1: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.abi_alignment_of_type)(a0 as Raw, a1 as Raw) }
    }
    pub(super) fn offset_of_element(&self, a0: Ref, a1: Ref, a2: u32) -> u64 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.offset_of_element)(a0 as Raw, a1 as Raw, a2) }
    }
    pub(super) fn get_next_global(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_next_global)(a0 as Raw) as Ref }
    }
    pub(super) fn get_initializer(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_initializer)(a0 as Raw) as Ref }
    }
    pub(super) fn is_global_constant(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_global_constant)(a0 as Raw) }
    }
    pub(super) fn is_thread_local(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_thread_local)(a0 as Raw) }
    }
    pub(super) fn is_externally_initialized(&self, a0: Ref) -> i32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_externally_initialized)(a0 as Raw) }
    }
    pub(super) fn is_a_constant_aggregate_zero(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_constant_aggregate_zero)(a0 as Raw) as Ref }
    }
    pub(super) fn is_a_constant_expr(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_constant_expr)(a0 as Raw) as Ref }
    }
    pub(super) fn get_const_opcode(&self, a0: Ref) -> u32 {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_const_opcode)(a0 as Raw) }
    }
    pub(super) fn get_aggregate_element(&self, a0: Ref, a1: u32) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.get_aggregate_element)(a0 as Raw, a1) as Ref }
    }
    pub(super) fn is_a_call_inst(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_call_inst)(a0 as Raw) as Ref }
    }
    pub(super) fn is_a_constant(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_constant)(a0 as Raw) as Ref }
    }
    pub(super) fn is_a_global_variable(&self, a0: Ref) -> Ref {
        // SAFETY: handles originate from the verified module owned by this import session.
        unsafe { (self.is_a_global_variable)(a0 as Raw) as Ref }
    }
}

impl Api {
    pub(super) fn kind(&self, name: &str) -> u32 {
        // SAFETY: the string is readable for its supplied length.
        unsafe { (self.get_enum_attribute_kind_for_name)(name.as_ptr().cast(), name.len()) }
    }
    pub(super) fn name(&self, value: Ref) -> String {
        let mut len = 0;
        // SAFETY: value belongs to the live module; LLVM returns len readable bytes.
        unsafe {
            let ptr = (self.get_value_name2)(value as Raw, &mut len);
            if len == 0 {
                return String::new();
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(ptr.cast(), len)).into_owned()
        }
    }
    pub(super) fn printed(&self, value: Ref) -> String {
        // SAFETY: LLVM returns an owned NUL-terminated message for a live value.
        unsafe {
            let ptr = (self.print_value_to_string)(value as Raw);
            let text = CStr::from_ptr(ptr.cast()).to_string_lossy().into_owned();
            (self.dispose_message)(ptr);
            text
        }
    }
    pub(super) fn attributes(&self, value: Ref, index: u32, call: bool) -> Vec<Ref> {
        let count = if call {
            self.get_call_site_attribute_count(value, index)
        } else {
            self.get_attribute_count_at_index(value, index)
        };
        let mut attrs = vec![0; count as usize];
        // SAFETY: buffer has LLVM's reported attribute count; Ref has pointer size/alignment.
        unsafe {
            let get = if call {
                self.get_call_site_attributes
            } else {
                self.get_attributes_at_index
            };
            get(value as Raw, index, attrs.as_mut_ptr().cast());
        }
        attrs
    }
    pub(super) fn params(&self, fty: Ref) -> Vec<Ref> {
        let mut params = vec![0; self.count_param_types(fty) as usize];
        // SAFETY: buffer has exactly LLVM's reported parameter count.
        unsafe {
            (self.get_param_types)(fty as Raw, params.as_mut_ptr().cast());
        }
        params
    }
    pub(super) fn indices(&self, inst: Ref) -> Vec<u32> {
        let count = self.get_num_indices(inst) as usize;
        if count == 0 {
            return Vec::new();
        }
        // SAFETY: LLVM owns count indices in the live verified instruction.
        unsafe { std::slice::from_raw_parts((self.get_indices)(inst as Raw), count).to_vec() }
    }
    pub(super) fn string_attribute(&self, attr: Ref) -> String {
        let mut len = 0;
        // SAFETY: LLVM returns len readable bytes for a string attribute.
        unsafe {
            let ptr = (self.get_string_attribute_kind)(attr as Raw, &mut len);
            if len == 0 {
                return String::new();
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(ptr.cast(), len as usize))
                .into_owned()
        }
    }
    pub(super) fn attribute(&self, value: Ref, index: u32, name: &str, call: bool) -> Ref {
        let kind = self.kind(name);
        if call {
            self.get_call_site_enum_attribute(value, index, kind)
        } else {
            self.get_enum_attribute_at_index(value, index, kind)
        }
    }
    pub(super) fn text_property(&self, value: Ref, name: &str) -> String {
        let get = match name {
            "target" => self.get_target,
            "layout" => self.get_data_layout_str,
            "gc" => self.get_gc,
            _ => self.get_section,
        };
        // SAFETY: LLVM returns a borrowed NUL-terminated property or null.
        unsafe {
            let ptr = get(value as Raw);
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }
    pub(super) fn has_inline_asm(&self, module: Ref) -> bool {
        let mut len = 0;
        // SAFETY: the module is live and the length output is writable.
        unsafe {
            (self.get_module_inline_asm)(module as Raw, &mut len);
        }
        len != 0
    }
    pub(super) fn has_linker_options(&self, module: Ref) -> bool {
        // SAFETY: name has exactly the supplied length; module is live.
        unsafe {
            !(self.get_named_metadata)(module as Raw, c"llvm.linker.options".as_ptr(), 19).is_null()
        }
    }
}

pub(super) struct Session<'a> {
    pub(super) api: &'a Api,
    context: Raw,
    pub(super) module: Ref,
    pub(super) td: Ref,
}
impl<'a> Session<'a> {
    pub(super) fn parse(api: &'a Api, source: &[u8]) -> Result<Self, String> {
        // SAFETY: context and buffer are created through the loaded LLVM API.
        let context = unsafe { (api.context_create)() };
        let mut session = Self {
            api,
            context,
            module: 0,
            td: 0,
        };
        let mut module = std::ptr::null_mut();
        let mut error = std::ptr::null_mut();
        // SAFETY: LLVM copies source and takes ownership of the buffer; outputs are valid.
        let status = unsafe {
            let buffer = (api.create_memory_buffer_with_memory_range_copy)(
                source.as_ptr().cast(),
                source.len(),
                c"input.ll".as_ptr(),
            );
            (api.parse_ir_in_context)(context, buffer, &mut module, &mut error)
        };
        session.module = module as Ref;
        if status != 0 {
            return Err(api.message(error));
        }
        // SAFETY: parser succeeded; verifier returns diagnostics rather than aborting (action 2).
        let status = unsafe { (api.verify_module)(module, 2, &mut error) };
        let diagnostic = api.message(error);
        if status != 0 {
            return Err(diagnostic);
        }
        let layout =
            CString::new(api.text_property(session.module, "layout")).map_err(|e| e.to_string())?;
        // SAFETY: layout comes from the parsed LLVM module and is NUL-terminated.
        session.td = unsafe { (api.create_target_data)(layout.as_ptr()) as Ref };
        Ok(session)
    }
}
impl Api {
    fn message(&self, ptr: Raw) -> String {
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: parser/verifier returned an owned NUL-terminated diagnostic.
        unsafe {
            let text = CStr::from_ptr(ptr.cast()).to_string_lossy().into_owned();
            (self.dispose_message)(ptr);
            text
        }
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        // SAFETY: each resource belongs to this session and is disposed once, in dependency order.
        unsafe {
            if self.td != 0 {
                (self.api.dispose_target_data)(self.td as Raw);
            }
            if self.module != 0 {
                (self.api.dispose_module)(self.module as Raw);
            }
            (self.api.context_dispose)(self.context);
        }
    }
}
