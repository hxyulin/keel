use keel_ir::{Layout, Type, VectorType};

#[test]
fn narrow_shapes_have_exact_layout_lane_masks_and_roundtrips() {
    for v in VectorType::ALL_64 {
        let ty = Type::Vector(v);
        assert_eq!(v.bytes(), 8);
        assert_eq!(ty.bytes(), Some(8));
        assert_eq!(u32::from(v.lanes()) * v.lane_type().bytes().unwrap(), 8);
        assert_eq!(v.mask().bytes(), 8);
        assert_eq!(v.mask().lanes(), v.lanes());
        assert_eq!(v.to_string().parse::<VectorType>().unwrap(), v);
        let member = Layout::Scalar(ty);
        assert_eq!((member.size(), member.align()), (8, 8));
        let aggregate = Layout::Struct(vec![
            Layout::Scalar(Type::I8),
            member,
            Layout::Scalar(Type::I8),
        ]);
        assert_eq!((aggregate.size(), aggregate.align()), (24, 8));
        assert_eq!(aggregate.leaves()[1].0, 8);
        let op = if v.lane_type().is_float() {
            "vfadd"
        } else {
            "viadd"
        };
        let source = format!(
            "func @narrow({v}, {v}) -> {v} {{\nblock0(v0: {v}, v1: {v}):\n v2 = {op} v0, v1\n return v2\n}}\n"
        );
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_ir::verify_module(&keel_ir::parse(&module.to_string()).unwrap()).unwrap();
    }
}
