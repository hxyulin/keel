use super::{archive, options};
use keel_link::{Input, ObjectInput};

fn compare(sources: &[&str], dead_strip: bool) {
    let assemblies: Vec<_> = sources
        .iter()
        .map(|s| keel_asm::assemble(s).unwrap())
        .collect();
    let bytes: Vec<_> = assemblies.iter().map(|a| a.to_macho().unwrap()).collect();
    let inputs: Vec<_> = bytes
        .iter()
        .map(|bytes| Input {
            name: "unit",
            bytes,
        })
        .collect();
    let mut opts = options();
    opts.dead_strip = dead_strip;
    let expected = keel_link::link(&inputs, opts).unwrap();
    let objects: Vec<_> = assemblies
        .into_iter()
        .map(|a| a.into_object().unwrap())
        .collect();
    let cached: Vec<_> = objects.iter().map(|o| o.encode().unwrap()).collect();
    for (object, bytes) in objects.iter().zip(&cached) {
        assert_eq!(&keel_object::Object::decode(bytes).unwrap(), object);
    }
    let mut opts = options();
    opts.dead_strip = dead_strip;
    let units = objects
        .into_iter()
        .map(|object| ObjectInput {
            name: "unit".into(),
            object,
        })
        .collect();
    let actual = keel_link::link_objects(units, &[], opts).unwrap();
    assert_eq!(expected.bytes, actual.bytes);
    let cache_inputs: Vec<_> = cached
        .iter()
        .map(|bytes| Input {
            name: "unit",
            bytes,
        })
        .collect();
    let snapshots = cached.clone();
    let mut opts = options();
    opts.dead_strip = dead_strip;
    let actual = keel_link::link_caches(&cache_inputs, &[], opts).unwrap();
    assert_eq!(expected.bytes, actual.bytes);
    assert_eq!(expected.warnings, actual.warnings);
    assert_eq!(cached, snapshots, "linking must not modify cached bytes");
}

#[test]
fn owned_and_cached_units_preserve_images() {
    for gc in [false, true] {
        compare(
            &[
                ".subsections_via_symbols\n.globl _main\n_main:\n.cfi_startproc\nstp x29,x30,[sp,#-16]!\n.cfi_def_cfa_offset 16\n.cfi_offset x29,-16\n.cfi_offset x30,-8\nadrp x0,Lmsg@PAGE\nadd x0,x0,Lmsg@PAGEOFF\nbl _puts\nbl _helper\nldp x29,x30,[sp],#16\nret\n.cfi_endproc\n.section __TEXT,__cstring,cstring_literals\nLmsg:\n.asciz \"hello\"\n",
                ".subsections_via_symbols\n.globl _helper\n.private_extern _helper\n_helper:\nadrp x0,_common@PAGE\nadd x0,x0,_common@PAGEOFF\nret\n.comm _common,32,4\n.data\n_pointer:\n.quad Linside+8\n_difference:\n.long _helper-Linside+4\n.text\nLinside:\nret\n.zerofill __DATA,__bss,_buffer,32,4\n",
            ],
            gc,
        );
        compare(
            &[
                ".globl _main\n_main:\nadrp x0,_local@GOTPAGE\nldr x0,[x0,_local@GOTPAGEOFF]\nret\n.data\n_local:\n.quad 1\n",
                ".globl _other\n_other:\nadrp x0,_local@GOTPAGE\nldr x0,[x0,_local@GOTPAGEOFF]\nret\n.data\n_local:\n.quad 2\n",
            ],
            gc,
        );
    }
}

#[test]
fn owned_demand_extracts_regular_archive_members() {
    let source = ".globl _main\n_main:\nb _helper\n";
    let assembly = keel_asm::assemble(source).unwrap();
    let bytes = assembly.to_macho().unwrap();
    let archive = archive(&[(
        "helper.o",
        keel_asm::assemble_object(".globl _helper\n_helper:\nret\n").unwrap(),
    )]);
    let archive_input = Input {
        name: "helper.a",
        bytes: &archive,
    };
    let expected = keel_link::link(
        &[
            Input {
                name: "main",
                bytes: &bytes,
            },
            archive_input,
        ],
        options(),
    )
    .unwrap();
    let cached = assembly.clone().into_object().unwrap().encode().unwrap();
    let cached_image = keel_link::link_caches(
        &[Input {
            name: "main",
            bytes: &cached,
        }],
        &[Input {
            name: "helper.a",
            bytes: &archive,
        }],
        options(),
    )
    .unwrap();
    assert_eq!(expected.bytes, cached_image.bytes);
    let actual = keel_link::link_objects(
        vec![ObjectInput {
            name: "main".into(),
            object: assembly.into_object().unwrap(),
        }],
        &[Input {
            name: "helper.a",
            bytes: &archive,
        }],
        options(),
    )
    .unwrap();
    assert_eq!(expected.bytes, actual.bytes);
}

#[test]
fn cache_and_owned_input_errors_are_explicit() {
    let object = keel_asm::assemble(".globl _main\n_main:\nret\n")
        .unwrap()
        .into_object()
        .unwrap();
    let bytes = object.encode().unwrap();
    for end in 0..bytes.len() {
        assert!(keel_object::Object::decode(&bytes[..end]).is_err());
    }
    let mut bad = bytes.clone();
    bad[0] ^= 1;
    assert!(keel_object::Object::decode(&bad).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(keel_object::Object::decode(&trailing).is_err());
    let mut bad = object.clone();
    bad.symbols[0].definition = keel_object::Definition::Atom(usize::MAX);
    assert!(
        keel_link::link_objects(
            vec![ObjectInput {
                name: "bad".into(),
                object: bad
            }],
            &[],
            options()
        )
        .unwrap_err()
        .contains("invalid native symbol atom")
    );
    let mut opts = options();
    opts.debug_directory = Some("/tmp/debug".into());
    assert!(
        keel_link::link_objects(
            vec![ObjectInput {
                name: "debug".into(),
                object
            }],
            &[],
            opts
        )
        .unwrap_err()
        .contains("Darwin debug maps")
    );
}

#[test]
fn semantic_addends_and_pairs_preserve_relocations_and_atom_boundaries() {
    let source = ".subsections_via_symbols\n.globl _main\n_main:\nadrp x0,_data@PAGE+8\nadd x0,x0,_data@PAGEOFF+8\nbl _callee+4\nret\n_callee:\nnop\nret\n.data\n_data:\n.quad _callee+4\n.long _callee-_main+12\n";
    for gc in [false, true] {
        compare(&[source], gc);
    }
    let object = keel_asm::assemble(source).unwrap().into_object().unwrap();
    assert_eq!(
        object.symbols.len(),
        1,
        "local names must not become linkage symbols"
    );
    assert!(
        object.sections[0]
            .fixups
            .iter()
            .any(|f| matches!(f.kind, keel_object::FixupKind::PagePair { .. }))
    );
    let split = ".subsections_via_symbols\n.globl _main\n_main:\nadrp x0,_data@PAGE\n_low:\n.no_dead_strip _low\nadd x0,x0,_data@PAGEOFF\nret\n.data\n_data:\n.quad 0\n";
    let object = keel_asm::assemble(split).unwrap().into_object().unwrap();
    assert!(
        !object.sections[0]
            .fixups
            .iter()
            .any(|f| f.kind.low_offset().is_some()),
        "pairs must not span independently movable atoms"
    );
}

#[test]
fn semantic_unwind_keeps_weak_copy_ownership_and_live_dependencies() {
    let weak = ".subsections_via_symbols\n.globl _main\n_main:\nbl _shared\nret\n.globl _shared\n.weak_definition _shared\n_shared:\n.cfi_startproc\n.cfi_def_cfa x28,16\nret\n.cfi_endproc\n";
    let strong = ".subsections_via_symbols\n.globl _shared\n_shared:\n.cfi_startproc\n.cfi_def_cfa x28,16\nnop\nret\n.cfi_endproc\n";
    for gc in [false, true] {
        compare(&[weak, strong], gc);
        compare(&[strong, weak], gc);
    }
    let personality = ".subsections_via_symbols\n.globl _main\n_main:\n.cfi_startproc\nstp x29,x30,[sp,#-16]!\nmov x29,sp\n.cfi_def_cfa x29,16\n.cfi_offset x29,-16\n.cfi_offset x30,-8\n.cfi_personality 155,_puts\n.cfi_lsda 16,_lsda\nldp x29,x30,[sp],#16\nret\n.cfi_endproc\n.data\n_lsda:\n.quad 0\n";
    for gc in [false, true] {
        compare(&[personality], gc);
    }
    let compact = ".globl _main\n_main:\nret\n.data\n_lsda:\n.quad 0\n.section __LD,__compact_unwind\n.quad _main\n.long 4\n.long 0x42001000\n.quad _puts\n.quad _lsda\n";
    for gc in [false, true] {
        compare(&[compact], gc);
    }
    let object = keel_asm::assemble(compact).unwrap().into_object().unwrap();
    assert_eq!(object.unwind.len(), 1);
    assert!(object.unwind[0].personality.is_some());
    assert!(object.unwind[0].lsda.is_some());
    assert!(!object.sections.iter().any(|s| s.name == "__compact_unwind"));
    let dead = ".subsections_via_symbols\n.globl _dead\n_dead:\n.cfi_startproc\n.cfi_personality 155,_missing\n.cfi_def_cfa x28,16\nret\n.cfi_endproc\n.globl _main\n_main:\n.cfi_startproc\n.cfi_def_cfa x28,16\nret\n.cfi_endproc\n";
    compare(&[dead], true);
}

#[test]
fn local_pointer_addend_follows_the_destination_atom() {
    let source = ".subsections_via_symbols\n.globl _main\n_main:\nret\n_previous:\nb _missing\nret\n_destination:\nret\n.data\n_pointer:\n.no_dead_strip _pointer\n.quad _previous+8\n";
    let mut assembly = keel_asm::assemble(source).unwrap();
    // Supply an interior assembler label after source-level alias folding, so
    // the Mach-O oracle exercises its section-relative local-pointer route.
    let mut label = assembly
        .symbols
        .iter()
        .find(|s| s.name == "_previous")
        .unwrap()
        .clone();
    label.name = "Lhead".into();
    assembly.symbols.push(label);
    assembly
        .relocations
        .iter_mut()
        .find(|r| r.symbol == "_previous")
        .unwrap()
        .symbol = "Lhead".into();
    let bytes = assembly.to_macho().unwrap();
    let mut opts = options();
    opts.dead_strip = true;
    let expected = keel_link::link(
        &[Input {
            name: "unit",
            bytes: &bytes,
        }],
        opts,
    )
    .unwrap();
    let object = assembly.into_object().unwrap();
    let cached = object.encode().unwrap();
    let mut opts = options();
    opts.dead_strip = true;
    let actual = keel_link::link_objects(
        vec![ObjectInput {
            name: "unit".into(),
            object,
        }],
        &[],
        opts,
    )
    .unwrap();
    assert_eq!(expected.bytes, actual.bytes);
    let mut opts = options();
    opts.dead_strip = true;
    let actual = keel_link::link_caches(
        &[Input {
            name: "unit",
            bytes: &cached,
        }],
        &[],
        opts,
    )
    .unwrap();
    assert_eq!(expected.bytes, actual.bytes);
}

#[test]
fn dead_native_page_pairs_are_validated_before_pruning() {
    let mut object = keel_asm::assemble(
        ".subsections_via_symbols\n.globl _main\n_main:\nb _live\n_dead:\nadrp x0,_missing@PAGE\nadd x0,x0,_missing@PAGEOFF\nbl _missing\n_live:\nret\n"
    ).unwrap().into_object().unwrap();
    let mut opts = options();
    opts.dead_strip = true;
    keel_link::link_objects(
        vec![ObjectInput {
            name: "dead".into(),
            object: object.clone(),
        }],
        &[],
        opts,
    )
    .unwrap();
    // Move the live atom boundary between the two words of a dead page pair.
    // Pruning the dead high word must not hide its write into the live atom.
    let keel_object::Target::Atom(live) = object.sections[0].fixups[0].target else {
        panic!("local branch must use an atom target");
    };
    object.atoms[live as usize].offset = 8;
    let cache = object.encode().unwrap();
    for cached in [false, true] {
        let mut opts = options();
        opts.dead_strip = true;
        let error = if cached {
            keel_link::link_caches(
                &[Input {
                    name: "dead",
                    bytes: &cache,
                }],
                &[],
                opts,
            )
        } else {
            keel_link::link_objects(
                vec![ObjectInput {
                    name: "dead".into(),
                    object: object.clone(),
                }],
                &[],
                opts,
            )
        }
        .expect_err("crossing pair must fail before marking");
        assert!(
            error.contains("paired relocation crosses a linker atom"),
            "{error}"
        );
    }
}

#[test]
fn interned_names_do_not_merge_dead_strong_import_demand() {
    let main = keel_asm::assemble(".weak_reference _optional\n.globl _main\n_main:\nb _optional\n")
        .unwrap()
        .into_object()
        .unwrap();
    let dead = keel_asm::assemble(".globl _dead\n_dead:\nb _optional\nb _unique_dead\n")
        .unwrap()
        .into_object()
        .unwrap();
    let mut opts = options();
    opts.dead_strip = true;
    let expected = keel_link::link_objects(
        vec![ObjectInput {
            name: "main".into(),
            object: main.clone(),
        }],
        &[],
        opts,
    )
    .unwrap();
    let caches: Vec<_> = [&main, &dead].iter().map(|o| o.encode().unwrap()).collect();
    for cached in [false, true] {
        let mut opts = options();
        opts.dead_strip = true;
        let actual = if cached {
            let inputs: Vec<_> = caches
                .iter()
                .map(|bytes| Input {
                    name: "unit",
                    bytes,
                })
                .collect();
            keel_link::link_caches(&inputs, &[], opts)
        } else {
            keel_link::link_objects(
                vec![
                    ObjectInput {
                        name: "main".into(),
                        object: main.clone(),
                    },
                    ObjectInput {
                        name: "dead".into(),
                        object: dead.clone(),
                    },
                ],
                &[],
                opts,
            )
        }
        .expect("dead strong reference must not override a live weak import");
        assert_eq!(actual.bytes, expected.bytes);
    }
}
