#[path = "data_layout/f80.rs"]
mod f80;
#[path = "data_layout/targets.rs"]
mod targets;

use Layout::Scalar as scalar;
use keel_ir::{DataInit, DataItem, DataLayout, Layout, Type, VectorType, parse};

fn array(elem: Layout, count: u32) -> Layout {
    Layout::Array(Box::new(elem), count)
}

#[test]
fn explicit_pointer_models_and_native_compatibility() {
    const NATIVE_BYTES: u64 = DataLayout::NATIVE64.pointer_bytes();
    const WASM_ALIGN: u64 = DataLayout::WASM32.pointer_align();
    assert_eq!(NATIVE_BYTES, keel_ir::data::PTR_BYTES);
    assert_eq!(WASM_ALIGN, 4);
    let pair = Layout::Struct(vec![scalar(Type::Ptr), scalar(Type::I8)]);
    assert_eq!((pair.size(), pair.align()), (16, 8));
    assert_eq!(pair.leaves(), [(0, Type::Ptr), (8, Type::I8)]);
    assert_eq!(pair.checked_size_with_layout(DataLayout::WASM32), Some(8));
    assert_eq!(pair.align_with_layout(DataLayout::WASM32), 4);
    assert_eq!(
        pair.leaves_with_layout(DataLayout::WASM32),
        [(0, Type::Ptr), (4, Type::I8)]
    );
    assert_eq!(
        pair.checked_size(),
        pair.checked_size_with_layout(DataLayout::NATIVE64)
    );
    assert_eq!(pair.leaves(), pair.leaves_with_layout(DataLayout::NATIVE64));
    assert_eq!(Type::Ptr.bytes(), None);
}

#[test]
fn nested_arrays_use_the_same_model_for_padding_and_stride() {
    let record = Layout::Struct(vec![scalar(Type::I8), scalar(Type::Ptr), scalar(Type::I16)]);
    let nested = Layout::Struct(vec![
        scalar(Type::I8),
        array(array(record, 2), 2),
        scalar(Type::I64),
    ]);
    for (layout, size, align, pointers, last) in [
        (DataLayout::WASM32, 64, 8, [8, 20, 32, 44], 56),
        (DataLayout::NATIVE64, 112, 8, [16, 40, 64, 88], 104),
    ] {
        assert_eq!(nested.checked_size_with_layout(layout), Some(size));
        assert_eq!(nested.align_with_layout(layout), align);
        let leaves = nested.leaves_with_layout(layout);
        let offsets: Vec<_> = leaves
            .iter()
            .filter_map(|&(offset, ty)| (ty == Type::Ptr).then_some(offset))
            .collect();
        assert_eq!(offsets, pointers);
        assert_eq!(leaves.last(), Some(&(last, Type::I64)));
    }
}

#[test]
fn empty_members_keep_alignment_without_leaves() {
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        let empty = Layout::Struct(vec![]);
        let many_empty = array(array(empty.clone(), u32::MAX), u32::MAX);
        assert_eq!(many_empty.checked_size_with_layout(layout), Some(0));
        assert_eq!(many_empty.align_with_layout(layout), 1);
        assert!(many_empty.leaves_with_layout(layout).is_empty());
        let aligned_empty = array(scalar(Type::Ptr), 0);
        assert_eq!(aligned_empty.checked_size_with_layout(layout), Some(0));
        assert_eq!(
            aligned_empty.align_with_layout(layout),
            layout.pointer_align()
        );
        assert!(aligned_empty.leaves_with_layout(layout).is_empty());
        let record = Layout::Struct(vec![scalar(Type::I8), aligned_empty, empty]);
        assert_eq!(
            record.checked_size_with_layout(layout),
            Some(layout.pointer_align())
        );
        assert_eq!(record.leaves_with_layout(layout), [(0, Type::I8)]);
    }
}

#[test]
fn checked_sizes_reject_products_additions_and_alignment_overflow() {
    // (2^32 - 1)^2 + 2(2^32 - 1) = u64::MAX, with byte alignment.
    let max = Layout::Struct(vec![
        array(array(scalar(Type::I8), u32::MAX), u32::MAX),
        array(scalar(Type::I8), u32::MAX),
        array(scalar(Type::I8), u32::MAX),
    ]);
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert_eq!(max.checked_size_with_layout(layout), Some(u64::MAX));
        let addition = Layout::Struct(vec![max.clone(), scalar(Type::I8)]);
        let field_rounding = Layout::Struct(vec![max.clone(), array(scalar(Type::I64), 0)]);
        let trailing_rounding = Layout::Struct(vec![array(scalar(Type::I64), 0), max.clone()]);
        let product = array(array(scalar(Type::I64), u32::MAX), u32::MAX);
        for overflow in [
            addition,
            field_rounding,
            trailing_rounding,
            product.clone(),
            array(product, 0),
        ] {
            assert_eq!(overflow.checked_size_with_layout(layout), None);
        }
    }
    let pointers = array(array(scalar(Type::Ptr), 1 << 30), u32::MAX);
    assert_eq!(
        pointers.checked_size_with_layout(DataLayout::WASM32),
        Some((1u64 << 32) * u64::from(u32::MAX))
    );
    assert_eq!(pointers.checked_size(), None);
}

#[test]
fn non_pointer_storage_is_unchanged_and_does_not_authorize_values() {
    for ty in [
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
        Type::I128,
        Type::F32,
        Type::F64,
        Type::F80,
    ] {
        let value = scalar(ty);
        assert_eq!(
            value.checked_size_with_layout(DataLayout::WASM32),
            value.checked_size()
        );
        assert_eq!(value.align_with_layout(DataLayout::WASM32), value.align());
    }
    assert_eq!(
        scalar(Type::F80).checked_size_with_layout(DataLayout::WASM32),
        Some(16)
    );
    for vector in VectorType::ALL {
        let value = Layout::Struct(vec![scalar(Type::I8), scalar(Type::Vector(vector))]);
        let width = u64::from(vector.bytes());
        for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
            assert_eq!(value.checked_size_with_layout(layout), Some(width * 2));
            assert_eq!(value.align_with_layout(layout), width);
            assert_eq!(
                value.leaves_with_layout(layout),
                [(0, Type::I8), (width, Type::Vector(vector))]
            );
        }
    }
}

#[test]
fn pointer_initializer_cursors_match_consecutive_wasm32_records() {
    // Two { char tag; char *p; i32 n; } records, then a byte sentinel.
    // Padding is explicit in the data stream, never supplied by the query.
    let mut items = Vec::new();
    for tag in [1, 2] {
        items.extend([
            DataItem::Bytes(vec![tag, 0, 0, 0]),
            DataItem::Ptr {
                symbol: "bytes".into(),
                addend: 3,
            },
            DataItem::Int {
                ty: Type::I32,
                value: 7,
            },
        ]);
    }
    items.push(DataItem::Bytes(vec![0xa5]));
    let init = DataInit {
        constant: false,
        align: 4,
        items,
    };
    for (layout, expected_offsets, expected_size) in [
        (DataLayout::WASM32, [0, 4, 8, 12, 16, 20, 24], 25),
        (DataLayout::NATIVE64, [0, 4, 12, 16, 20, 28, 32], 33),
    ] {
        let mut cursor = 0;
        for (item, expected) in init.items.iter().zip(expected_offsets) {
            assert_eq!(cursor, expected);
            cursor += item.size_with_layout(layout);
        }
        assert_eq!(cursor, expected_size);
        assert_eq!(init.checked_size_with_layout(layout), Some(expected_size));
    }
    assert_eq!(init.size(), 33);
}

#[test]
fn extended_scalar_padding_survives_pointer_layout_changes() {
    let value = array(
        Layout::Struct(vec![
            scalar(Type::Ptr),
            scalar(Type::F80),
            scalar(Type::Ptr),
        ]),
        2,
    );
    for layout in [DataLayout::WASM32, DataLayout::NATIVE64] {
        assert_eq!(value.checked_size_with_layout(layout), Some(96));
        assert_eq!(value.align_with_layout(layout), 16);
        assert_eq!(
            value.leaves_with_layout(layout),
            [
                (0, Type::Ptr),
                (16, Type::F80),
                (32, Type::Ptr),
                (48, Type::Ptr),
                (64, Type::F80),
                (80, Type::Ptr),
            ]
        );
    }
}

#[test]
fn initializers_are_packed_and_rtti_names_stay_tagged_eight_bytes() {
    let ptr = DataItem::Ptr {
        symbol: "data".into(),
        addend: 3,
    };
    let int = DataItem::Int {
        ty: Type::I32,
        value: 7,
    };
    let mut init = DataInit {
        constant: true,
        align: 8,
        items: vec![ptr.clone(), int],
    };
    assert_eq!(ptr.size(), 8);
    assert_eq!(ptr.size_with_layout(DataLayout::WASM32), 4);
    assert_eq!(init.size(), 12);
    assert_eq!(init.checked_size_with_layout(DataLayout::WASM32), Some(8));
    init.items.extend([
        DataItem::Bytes(vec![1, 2, 3]),
        DataItem::Zero(5),
        DataItem::RttiName {
            symbol: "name".into(),
        },
    ]);
    assert_eq!(init.size(), 28);
    assert_eq!(
        init.checked_size_with_layout(DataLayout::NATIVE64),
        Some(28)
    );
    assert_eq!(init.checked_size_with_layout(DataLayout::WASM32), Some(24));
    init.items.clear();
    assert_eq!(init.checked_size_with_layout(DataLayout::WASM32), Some(0));
}

#[test]
fn layout_queries_do_not_change_native_text_serialization() {
    let source = "data @p = const align 8 { ptr @bytes + 1, i32 7, rtti_name @bytes }\ndata @bytes = const align 1 { b\"abc\\0\" }\n";
    let module = parse(source).unwrap();
    let before = module.to_string();
    assert_eq!(
        module.data[0]
            .init
            .as_ref()
            .unwrap()
            .checked_size_with_layout(DataLayout::WASM32),
        Some(16)
    );
    assert_eq!(module.to_string(), before);
    assert_eq!(parse(&before).unwrap().to_string(), before);
}
