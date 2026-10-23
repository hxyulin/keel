use keel_link::{Input, elf};
use object::{Object, ObjectSection, ObjectSymbol};
fn object(s: &str) -> Vec<u8> {
    keel_asm::x86::assemble_object(s).unwrap()
}
fn link(objects: &[Vec<u8>], gc: bool) -> Result<elf::Executable, String> {
    elf::link(
        &objects
            .iter()
            .enumerate()
            .map(|(_, bytes)| Input {
                name: "gc.o",
                bytes,
            })
            .collect::<Vec<_>>(),
        elf::Options {
            gc_sections: gc,
            ..Default::default()
        },
    )
}
fn names(exe: &elf::Executable) -> Vec<String> {
    object::File::parse(exe.bytes.as_slice())
        .unwrap()
        .symbols()
        .map(|s| s.name().unwrap().to_owned())
        .collect()
}
const START: &str = ".text\n.globl _start\n_start: ret\n";
#[test]
fn unreachable_cycle_dead_undefined_and_default_compatibility() {
    let o = object(&format!(
        "{START}.section .data.a,\"aw\"\na: .quad b\n.section .data.b,\"aw\"\nb: .quad a\n.quad missing\n"
    ));
    assert!(
        link(&[o.clone()], false)
            .unwrap_err()
            .contains("undefined symbol")
    );
    let exe = link(&[o], true).unwrap();
    assert!(
        !names(&exe)
            .iter()
            .any(|n| n == "a" || n == "b" || n == "missing")
    );
}
#[test]
fn live_pointer_chain_and_shared_section_are_indivisible() {
    let o = object(
        ".text\n.globl _start\n_start: mov rax, [rip + p]\nret\n.section .data.p,\"aw\"\np: .quad q\n.section .data.q,\"aw\"\nq: .quad 42\nneighbor: .quad 13\n.section .data.dead,\"aw\"\ndead: .quad 9\n",
    );
    let exe = link(&[o], true).unwrap();
    let ns = names(&exe);
    for n in ["p", "q", "neighbor"] {
        assert!(ns.iter().any(|s| s == n));
    }
    assert!(!ns.iter().any(|s| s == "dead"));
}
#[test]
fn retain_roots_but_plain_used_section_does_not() {
    let exe=link(&[object(&format!("{START}.section .data.keep,\"awR\"\nkeep: .quad target\n.section .data.target,\"aw\"\ntarget: .quad 42\n.section .data.used,\"aw\"\nused: .quad missing\n"))],true).unwrap();
    let ns = names(&exe);
    assert!(ns.iter().any(|s| s == "target"));
    assert!(!ns.iter().any(|s| s == "used"));
}
#[test]
fn weak_loser_relocations_use_strong_winner() {
    let a = object(
        ".text\n.globl _start\n_start: call choice\nret\n.section .text.weak,\"ax\"\n.weak choice\nchoice: call absent\nret\n",
    );
    let b = object(".section .text.strong,\"ax\"\n.globl choice\nchoice: ret\n");
    let exe = link(&[a, b], true).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(f.section_by_name(".text.weak").is_none());
    assert!(f.section_by_name(".text.strong").is_some());
}
#[test]
fn live_undefined_still_errors_and_common_is_individual() {
    assert!(
        link(
            &[object(".text\n.globl _start\n_start: call absent\n")],
            true
        )
        .unwrap_err()
        .contains("undefined symbol")
    );
    let exe=link(&[object(".text\n.globl _start\n_start: mov rax, [rip + used]\nret\n.comm used,8,8\n.comm dead,1024,16\n")],true).unwrap();
    let ns = names(&exe);
    assert!(ns.iter().any(|s| s == "used"));
    assert!(!ns.iter().any(|s| s == "dead"));
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(f.section_by_name(".bss.common").unwrap().size(), 8);
}
#[test]
fn constructor_and_live_boundaries_root_selected_contributions() {
    let exe=link(&[object(".text\n.globl _start\n_start: mov rax, [rip + __start_registry]\nret\n.section .text.ctor,\"ax\"\nctor: ret\n.section .init_array,\"aw\",@init_array\n.p2align 3\n.quad ctor\n.section registry,\"aw\"\nitem: .quad 42\n"),object(".section registry,\"aw\"\nitem: .quad 13\n")],true).unwrap();
    let ns = names(&exe);
    assert!(ns.iter().any(|s| s == "ctor"));
    assert_eq!(ns.iter().filter(|s| s.as_str() == "item").count(), 2);
}
#[test]
fn live_and_dead_fdes_sharing_a_cie_are_pruned_before_merge() {
    let o = object(
        ".text\n.globl _start\n_start:\n.cfi_startproc\ncall live\nret\n.cfi_endproc\n.section .text.live,\"ax\"\nlive:\n.cfi_startproc\nret\n.cfi_endproc\n.section .text.dead,\"ax\"\ndead:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    let exe = link(&[o], true).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let hdr = f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap();
    assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 2);
    assert!(f.section_by_name(".text.dead").is_none());
}
#[test]
fn selected_comdat_group_liveness_is_atomic() {
    let exe=link(&[object(".text\n.globl _start\n_start: call choice\nret\n"),object(".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\nchoice: ret\n.section .data.choice,\"awG\",@progbits,key,comdat\npayload: .quad 42\n"),object(".section .text.choice,\"axG\",@progbits,key,comdat\n.globl choice\nchoice: call absent\nret\n")],true).unwrap();
    assert!(names(&exe).iter().any(|n| n == "payload"));
}

#[test]
fn weak_fde_loser_does_not_keep_dead_code_or_duplicate_winner_fde() {
    let a = object(
        ".text\n.globl _start\n_start: call choice\nret\n.section .text.weak,\"ax\"\n.weak choice\nchoice:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    let b = object(
        ".section .text.strong,\"ax\"\n.globl choice\nchoice:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    let exe = link(&[a, b], true).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let hdr = f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap();
    assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 1);
}

fn archive(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut bytes = b"!<arch>\n".to_vec();
    for (name, data) in members {
        bytes.extend_from_slice(
            format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("{name}/"),
                0,
                0,
                0,
                "100644",
                data.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(data);
        if !bytes.len().is_multiple_of(2) {
            bytes.push(b'\n');
        }
    }
    bytes
}

fn shared_interface() -> Vec<u8> {
    let obj = object(
        ".text\n.globl _start\n_start:\nret\n.globl answer\n.type answer,@function\nanswer:\nret\n.data\n.globl value\n.type value,@object\nvalue:\n.quad 42\n.section .dynamic,\"aw\",@progbits\n.quad 0\n.quad 0\n",
    );
    let mut image = link(&[obj], false).unwrap().bytes;
    let file = object::File::parse(image.as_slice()).unwrap();
    let dynamic = file.section_by_name(".dynamic").unwrap().index().0;
    let symbols = file.section_by_name(".symtab").unwrap().index().0;
    let strings = file.section_by_name(".strtab").unwrap().index().0;
    let shoff = u64::from_le_bytes(image[40..48].try_into().unwrap()) as usize;
    image[16..18].copy_from_slice(&3u16.to_le_bytes());
    image[shoff + symbols * 64 + 4..shoff + symbols * 64 + 8].copy_from_slice(&11u32.to_le_bytes());
    image[shoff + dynamic * 64 + 4..shoff + dynamic * 64 + 8].copy_from_slice(&6u32.to_le_bytes());
    image[shoff + dynamic * 64 + 40..shoff + dynamic * 64 + 44]
        .copy_from_slice(&(strings as u32).to_le_bytes());
    image[shoff + dynamic * 64 + 56..shoff + dynamic * 64 + 64]
        .copy_from_slice(&16u64.to_le_bytes());
    image
}

#[test]
fn extraction_precedes_gc_and_retain_does_not_extract() {
    let member =
        object(".section .data.retained,\"awR\"\n.globl retained\nretained: .quad absent\n");
    let exe = link(&[object(START), archive(&[("retain.o", member)])], true).unwrap();
    assert!(!names(&exe).iter().any(|s| s == "retained"));
    // Ordinary extraction still sees references from sections later discarded.
    // Once admitted, the archive member's RETAIN contract takes effect.
    let root = object(&format!(
        "{START}.section .data.dead,\"aw\"\n.quad extracted\n"
    ));
    let member = object(
        ".section .text.extracted,\"ax\"\n.globl extracted\nextracted: ret\n.section .data.retained,\"awR\"\nretained: .quad 42\n",
    );
    let exe = link(&[root, archive(&[("extract.o", member)])], true).unwrap();
    let ns = names(&exe);
    assert!(ns.iter().any(|s| s == "retained"));
    assert!(!ns.iter().any(|s| s == "extracted"));
}
#[test]
fn dead_import_and_got_do_not_allocate_but_visible_exports_are_roots() {
    let o = object(&format!(
        "{START}.section .text.dead,\"ax\"\ndead: call answer\nmov rax, [rip + value@GOTPCREL]\nret\n.section .text.export,\"ax\"\n.globl exported\nexported: ret\n"
    ));
    let exe = link(&[o, shared_interface()], true).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert_eq!(f.dynamic_symbols().filter(|s| s.is_undefined()).count(), 0);
    assert!(f.dynamic_symbols().any(|s| s.name().unwrap() == "exported"));
    assert_eq!(f.section_by_name(".got").unwrap().size(), 0);
    let o = object(".text\n.globl _start\n_start: call answer\nret\n");
    let exe = link(&[o, shared_interface()], true).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(
        f.dynamic_symbols()
            .any(|s| s.is_undefined() && s.name().unwrap() == "answer")
    );
}

#[test]
fn owner_drives_personality_and_lsda_liveness() {
    let o = object(
        ".text\n.globl _start\n_start:\n.cfi_startproc\n.cfi_personality 155,personality_ref\n.cfi_lsda 27,lsda\nret\n.cfi_endproc\n.section .text.dead,\"ax\"\ndead:\n.cfi_startproc\n.cfi_personality 155,dead_personality_ref\n.cfi_lsda 27,dead_lsda\nret\n.cfi_endproc\n.section .text.personality,\"ax\"\npersonality: ret\n.section .data.personality,\"aw\"\n.p2align 3\npersonality_ref: .quad personality\n.section .gcc_except_table.live,\"a\"\nlsda: .byte 0\n.section .data.dead_personality,\"aw\"\n.p2align 3\ndead_personality_ref: .quad missing_personality\n.section .gcc_except_table.dead,\"a\"\ndead_lsda: .quad missing_lsda\n",
    );
    let exe = link(&[o], true).unwrap();
    let ns = names(&exe);
    for n in ["lsda", "personality_ref", "personality"] {
        assert!(ns.iter().any(|s| s == n));
    }
    for n in ["dead", "dead_lsda", "dead_personality_ref"] {
        assert!(!ns.iter().any(|s| s == n));
    }
}

#[test]
fn shared_cie_personality_survives_dead_first_fde_with_distinct_lsdas_in_multiple_units() {
    let unit = |name: &str| {
        object(&format!(
            r#".section .text.dead,"ax"
dead:
.cfi_startproc
.cfi_personality 155,personality_ref
.cfi_lsda 27,dead_lsda
ret
.cfi_endproc
.section .text.{name},"ax"
.globl {name}
{name}:
.cfi_startproc
.cfi_personality 155,personality_ref
.cfi_lsda 27,lsda
ret
.cfi_endproc
.section .text.unrelated,"ax"
unrelated:
.cfi_startproc
.cfi_personality 155,missing_personality
.cfi_lsda 27,missing_lsda
ret
.cfi_endproc
.section .data.personality,"aw"
.p2align 3
personality_ref: .quad personality
.section .text.personality,"ax"
personality: ret
.section .gcc_except_table.live,"a"
lsda: .byte 0
.section .gcc_except_table.dead,"a"
dead_lsda: .quad missing
"#
        ))
    };
    let root = object(".text\n.globl _start\n_start: call one\ncall two\nret\n");
    let exe = link(&[root, unit("one"), unit("two")], true).unwrap();
    let ns = names(&exe);
    for n in ["lsda", "personality_ref", "personality"] {
        assert_eq!(ns.iter().filter(|s| s.as_str() == n).count(), 2, "{n}");
    }
    for n in [
        "dead",
        "dead_lsda",
        "unrelated",
        "missing",
        "missing_personality",
        "missing_lsda",
    ] {
        assert!(!ns.iter().any(|s| s == n), "{n}");
    }
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let hdr = f.section_by_name(".eh_frame_hdr").unwrap().data().unwrap();
    assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 2);
}

#[test]
fn same_named_local_targets_keep_their_original_object_identity() {
    let root = object(".text\n.globl _start\n_start: call one\ncall two\nret\n");
    let unit = |name: &str, n: u64| {
        object(&format!(
            ".section .text.{name},\"ax\"\n.globl {name}\n{name}: mov rax, [rip + pointer]\nret\n.section .data.{name},\"aw\"\npointer: .quad local\nlocal: .quad {n}\n"
        ))
    };
    let exe = link(&[root, unit("one", 11), unit("two", 22)], true).unwrap();
    let f = object::File::parse(exe.bytes.as_slice()).unwrap();
    let pointers: Vec<_> = f
        .symbols()
        .filter(|s| s.name().unwrap() == "pointer")
        .map(|s| s.address())
        .collect();
    assert_eq!(pointers.len(), 2);
    let read = |address: u64| {
        u64::from_le_bytes(
            exe.bytes[(address - 0x400000) as usize..(address - 0x400000 + 8) as usize]
                .try_into()
                .unwrap(),
        )
    };
    assert_eq!(read(read(pointers[0])), 11);
    assert_eq!(read(read(pointers[1])), 22);
}
#[test]
fn gc_does_not_hide_unsupported_tls_contracts() {
    let o = object(&format!(
        "{START}.section .tdata.dead,\"awT\",@progbits\n.long 1\n"
    ));
    assert!(link(&[o], true).unwrap_err().contains("TLS"));
}

#[test]
fn discarded_fdes_still_validate_original_code_ranges() {
    for (section, pc, range) in [
        (".text.dead,\"ax\"", "dead + 2", "1"),
        (".text.dead,\"ax\"", "dead - 1", "1"),
        (".text.dead,\"ax\"", "dead", "3"),
        (".text.dead,\"ax\"", "dead", "0xffffffffffffffff"),
        (".text.dead,\"ax\"", "dead + 1", "0xffffffffffffffff"),
        (".data.dead,\"aw\"", "dead", "1"),
    ] {
        let o = object(&format!(
            "{START}.section {section}\n.p2align 4\ndead: ret\nret\n\
             .section .eh_frame,\"a\",@progbits\n.p2align 2\n\
             .long 12,0\n.byte 1,0,1,120,16,0,0,0\n\
             .long 20,20\n.quad {pc},{range}\n"
        ));
        assert!(link(&[o.clone()], false).is_err(), "{pc} / {range}");
        let err = link(&[o], true).unwrap_err();
        assert!(err.contains("unwind FDE range"), "{pc} / {range}: {err}");
    }
}

#[test]
fn weak_boundary_for_non_identifier_section_does_not_root_it() {
    let o = object(
        ".text\n.globl _start\n.weak __start_bad.name\n_start: mov rax, [rip + __start_bad.name]\nret\n.section bad.name,\"aw\"\n.quad absent\n",
    );
    let exe = link(&[o], true).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(file.section_by_name("bad.name").is_none());
    assert!(!names(&exe).iter().any(|s| s == "absent"));
}

#[test]
fn common_resolution_keeps_maximum_size_and_alignment_only_for_live_names() {
    let root = object(
        ".text\n.globl _start\n_start: mov rax, [rip + shared]\nret\n.comm shared,8,8\n.comm unused,4096,64\n",
    );
    let other = object(".comm shared,64,32\n");
    let exe = link(&[root, other], true).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let common = file.section_by_name(".bss.common").unwrap();
    assert_eq!(common.size(), 64);
    assert_eq!(common.align(), 32);
    assert!(!names(&exe).iter().any(|s| s == "unused"));
}

#[test]
fn first_weak_winner_drops_loser_code_and_its_dependencies() {
    let root = object(".text\n.globl _start\n_start: call choice\nret\n");
    let first = object(".section .text.first,\"ax\"\n.weak choice\nchoice: ret\n");
    let second = object(".section .text.second,\"ax\"\n.weak choice\nchoice: call absent\nret\n");
    let exe = link(&[root, first, second], true).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(file.section_by_name(".text.first").is_some());
    assert!(file.section_by_name(".text.second").is_none());
}

#[test]
fn multiple_fdes_with_nonzero_code_addends_survive_as_one_contribution() {
    let o = object(
        ".text\n.globl _start\n_start:\n.cfi_startproc\nret\n.cfi_endproc\nneighbor:\n.cfi_startproc\nret\n.cfi_endproc\n.section .text.dead,\"ax\"\ndead:\n.cfi_startproc\nret\n.cfi_endproc\n",
    );
    let exe = link(&[o], true).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let hdr = file
        .section_by_name(".eh_frame_hdr")
        .unwrap()
        .data()
        .unwrap();
    assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 2);
    assert!(names(&exe).iter().any(|s| s == "neighbor"));
    assert!(file.section_by_name(".text.dead").is_none());
}

#[test]
fn deep_reverse_order_chain_reaches_cycle_and_discards_unreachable_cycle() {
    const COUNT: usize = 256;
    let mut source = String::from(".text\n.globl _start\n_start: mov rax, [rip + node0]\nret\n");
    // Reverse contribution order defeats a whole-graph traversal that happens
    // to discover the next section later in the same pass.
    for i in (0..COUNT).rev() {
        let next = (i + 1) % COUNT;
        source.push_str(&format!(
            ".section .data.node{i},\"aw\"\nnode{i}: .quad node{next},node{next}\n"
        ));
    }
    source.push_str(
        ".section .data.dead_a,\"aw\"\ndead_a: .quad dead_b\n\
         .section .data.dead_b,\"aw\"\ndead_b: .quad dead_a,absent\n",
    );
    let exe = link(&[object(&source)], true).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let addresses: std::collections::BTreeMap<_, _> = file
        .symbols()
        .map(|s| (s.name().unwrap(), s.address()))
        .collect();
    for i in 0..COUNT {
        let data = file
            .section_by_name(&format!(".data.node{i}"))
            .unwrap()
            .data()
            .unwrap();
        let next = addresses[format!("node{}", (i + 1) % COUNT).as_str()];
        assert_eq!(u64::from_le_bytes(data[..8].try_into().unwrap()), next);
        assert_eq!(u64::from_le_bytes(data[8..].try_into().unwrap()), next);
    }
    for name in ["dead_a", "dead_b", "absent"] {
        assert!(!addresses.contains_key(name), "{name}");
    }
}

#[test]
fn boundary_and_group_edges_propagate_across_units_to_another_boundary_and_cycle() {
    let root = object(
        ".text\n.globl _start\n_start: mov rax, [rip + __start_registry]\n\
         mov rax, [rip + __stop_registry]\nret\n",
    );
    let registry = object(
        ".section registry,\"awG\",@progbits,registry_group,comdat\n\
         registry_item: .quad 1\n\
         .section .data.group_peer,\"awG\",@progbits,registry_group,comdat\n\
         group_peer: .quad __start_secondary,__stop_secondary\n",
    );
    let secondary = object(
        ".section secondary,\"aw\"\nsecondary_item: .quad cycle_a\n\
         .section .data.cycle_a,\"aw\"\n.globl cycle_a\ncycle_a: .quad cycle_b\n\
         .section .data.cycle_b,\"aw\"\ncycle_b: .quad cycle_a,__stop_registry\n\
         .section unused,\"aw\"\nunused_item: .quad absent\n",
    );
    let other_registry = object(".section registry,\"aw\"\nother_item: .quad cycle_a\n");
    let exe = link(&[root, registry, secondary, other_registry], true).unwrap();
    let ns = names(&exe);
    for name in [
        "registry_item",
        "other_item",
        "group_peer",
        "secondary_item",
        "cycle_a",
        "cycle_b",
        "__start_registry",
        "__stop_registry",
        "__start_secondary",
        "__stop_secondary",
    ] {
        assert!(ns.iter().any(|s| s == name), "{name}");
    }
    for name in ["unused_item", "absent"] {
        assert!(!ns.iter().any(|s| s == name), "{name}");
    }
}

#[test]
fn defined_boundary_name_is_an_ordinary_edge_to_its_selected_definition() {
    let root = object(
        ".text\n.globl _start\n_start: mov rax, [rip + __start_registry]\nret\n\
         .section .data.boundary,\"aw\"\n.globl __start_registry\n\
         __start_registry: .quad target\n\
         .section .data.target,\"aw\"\ntarget: .quad 42\n\
         .section registry,\"aw\"\nunused_item: .quad absent\n",
    );
    let exe = link(&[root], true).unwrap();
    let ns = names(&exe);
    for name in ["__start_registry", "target"] {
        assert!(ns.iter().any(|s| s == name), "{name}");
    }
    for name in ["unused_item", "absent"] {
        assert!(!ns.iter().any(|s| s == name), "{name}");
    }
}

#[test]
fn group_indices_are_scoped_to_units_and_each_live_group_expands() {
    let group = |name: &str| {
        object(&format!(
            ".section .text.{name},\"axG\",@progbits,{name}_key,comdat\n\
             .globl {name}\n{name}: ret\n\
             .section .data.{name},\"awG\",@progbits,{name}_key,comdat\n\
             {name}_peer: .quad {name}_target\n\
             .section .data.{name}_target,\"aw\"\n{name}_target: .quad 42\n"
        ))
    };
    let first = group("first");
    let second = group("second");
    let group_index = |bytes: &[u8]| {
        object::File::parse(bytes)
            .unwrap()
            .sections()
            .find(|s| s.name().unwrap() == ".group")
            .unwrap()
            .index()
    };
    // Equal input group IDs must still represent two distinct liveness groups.
    assert_eq!(group_index(&first), group_index(&second));
    for both_live in [false, true] {
        let root = object(&format!(
            ".text\n.globl _start\n_start: call first\n{}ret\n",
            if both_live { "call second\n" } else { "" }
        ));
        let exe = link(&[root, first.clone(), second.clone()], true).unwrap();
        let ns = names(&exe);
        for suffix in ["", "_peer", "_target"] {
            assert!(ns.contains(&format!("first{suffix}")));
            assert_eq!(ns.contains(&format!("second{suffix}")), both_live);
        }
    }
}

#[test]
fn dead_boundary_edges_and_unreferenced_weak_boundaries_do_not_retain_sections() {
    let root = object(&format!(
        "{START}.weak __start_registry\n\
         .section .data.dead,\"aw\"\n.quad __stop_registry\n\
         .section registry,\"aw\"\nitem: .quad absent\n"
    ));
    let exe = link(&[root], true).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(file.section_by_name("registry").is_none());
    for name in ["__start_registry", "__stop_registry", "item", "absent"] {
        assert!(!names(&exe).iter().any(|s| s == name), "{name}");
    }
}

#[test]
fn multiple_unwind_sections_in_each_unit_keep_original_symbol_and_relocation_identities() {
    let root = object(".text\n.globl _start\n_start: call one\ncall two\nret\n");
    let unit = |name: &str| {
        let mut bytes = object(&format!(
            r#".section .text.{name},"ax"
.globl {name}
{name}: ret
.section .text.dead,"ax"
dead: ret
.section .eh_frame,"a",@progbits
.p2align 3
.long 12,0
.byte 1,0,1,120,16,0,0,0
first_dead_record: .long 20,20
.quad dead,1
.section .eh_frame.extra,"a",@progbits
.p2align 3
.long 12,0
.byte 1,0,1,120,16,0,0,0
second_dead_record: .long 20,20
.quad dead,1
live_record: .long 20,44
.quad {name},1
.size live_record,24
"#
        ));
        // Give distinct input sections the same supported unwind name. The
        // first loses all records; its retain shifts relocation positions in
        // this same unit before the second section is processed.
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let first = file.section_by_name(".eh_frame").unwrap().index().0;
        let second = file.section_by_name(".eh_frame.extra").unwrap().index().0;
        let shoff = u64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
        let name = bytes[shoff + first * 64..shoff + first * 64 + 4].to_vec();
        bytes[shoff + second * 64..shoff + second * 64 + 4].copy_from_slice(&name);
        bytes
    };
    let exe = link(
        &[
            root,
            unit("one"),
            object(".data\nunrelated: .quad 0\n"),
            unit("two"),
        ],
        true,
    )
    .unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    let frames = file.section_by_name(".eh_frame").unwrap();
    let records: Vec<_> = file
        .symbols()
        .filter(|s| s.name().unwrap() == "live_record")
        .collect();
    assert_eq!(records.len(), 2);
    for record in records {
        assert_eq!(record.size(), 24);
        assert_eq!(record.section_index(), Some(frames.index()));
        let offset = (record.address() - frames.address()) as usize;
        let data = frames.data().unwrap();
        assert_eq!(
            u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()),
            20
        );
    }
    let ns = names(&exe);
    for dead in [
        "dead",
        "first_dead_record",
        "second_dead_record",
        "unrelated",
    ] {
        assert!(!ns.iter().any(|s| s == dead), "{dead}");
    }
    let hdr = file
        .section_by_name(".eh_frame_hdr")
        .unwrap()
        .data()
        .unwrap();
    assert_eq!(u32::from_le_bytes(hdr[8..12].try_into().unwrap()), 2);
}

#[test]
fn rewritten_target_checks_preserve_reference_and_invalid_symbol_precedence() {
    for (target, invalid_first, expected) in [
        (
            "first",
            false,
            "--gc-sections references into rewritten .eh_frame are unsupported",
        ),
        ("first", true, "invalid GC relocation symbol"),
        // A reference preceding the error for the second target must not mask
        // that error while checking the first rewritten contribution.
        ("second", false, "invalid GC relocation symbol"),
    ] {
        let pointers = if invalid_first {
            format!("invalid,{target}")
        } else {
            format!("{target},invalid")
        };
        let mut source = String::from(START);
        source.push_str(".section .eh_frame,\"a\",@progbits\n.p2align 3\n.globl first\nfirst:\n");
        for pointer in pointers.split(',') {
            // Unused CIEs have no FDE owner edges. Selecting their source as
            // the entry contribution makes these valid pointer fields enter
            // the lazy reference check, including the malformed symbol below.
            source.push_str(&format!(
                ".long 24,0\n.byte 1,122,80,0,1,120,16,9,0\n.quad {pointer}\n.byte 0,0,0\n"
            ));
        }
        source.push_str(".section .eh_frame.extra,\"a\",@progbits\n.p2align 3\nsecond:\n.long 12,0\n.byte 1,0,1,120,16,0,0,0\n");
        let mut bytes = object(&source);
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let first = file.section_by_name(".eh_frame").unwrap().index().0;
        let second = file.section_by_name(".eh_frame.extra").unwrap().index().0;
        let invalid = file.symbol_by_name("invalid").unwrap().index();
        let offset = file
            .section_by_name(".eh_frame")
            .unwrap()
            .relocations()
            .position(|(_, r)| r.target() == object::RelocationTarget::Symbol(invalid))
            .unwrap();
        let (rela, _) = file
            .section_by_name(".rela.eh_frame")
            .unwrap()
            .file_range()
            .unwrap();
        let shoff = u64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
        let name = bytes[shoff + first * 64..shoff + first * 64 + 4].to_vec();
        bytes[shoff + second * 64..shoff + second * 64 + 4].copy_from_slice(&name);
        // Preserve the relocation type, replacing just its symbol index.
        let symbol = rela as usize + offset * 24 + 12;
        bytes[symbol..symbol + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            link(&[bytes.clone(), shared_interface()], true).unwrap_err(),
            "--gc-sections exports into rewritten .eh_frame are unsupported",
            "export must precede reference scanning: {target} / {invalid_first}"
        );
        assert_eq!(
            elf::link(
                &[Input {
                    name: "precedence.o",
                    bytes: &bytes
                }],
                elf::Options {
                    gc_sections: true,
                    entry: "first".into(),
                    ..Default::default()
                },
            )
            .unwrap_err(),
            expected,
            "{target} / {invalid_first}"
        );
    }
}

#[test]
fn live_raw_reference_to_discarded_weak_definition_keeps_selected_winner() {
    let loser = object(
        ".text\n.globl _start\n_start: ret\n\
         .section .data.keep,\"awR\"\npointer: .quad choice\n\
         .section .text.weak,\"ax\"\n.weak choice\nchoice: call absent\nret\n",
    );
    let winner = object(".section .text.strong,\"ax\"\n.globl choice\nchoice: ret\n");
    let exe = link(&[loser, winner], true).unwrap();
    let file = object::File::parse(exe.bytes.as_slice()).unwrap();
    assert!(file.section_by_name(".text.weak").is_none());
    assert!(!file.symbols().any(|s| s.name().unwrap() == "absent"));
    let pointer = file.section_by_name(".data.keep").unwrap().data().unwrap();
    assert_eq!(
        u64::from_le_bytes(pointer.try_into().unwrap()),
        file.symbol_by_name("choice").unwrap().address()
    );
}
