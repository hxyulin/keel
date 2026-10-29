use keel_ir::{Layout, Type, VectorType};
#[test]
fn wide_shapes_have_exact_layout_lane_masks_and_roundtrips() {
    for v in VectorType::ALL_256 {
        let t = Type::Vector(v);
        assert_eq!(v.bytes(), 32);
        assert_eq!(t.bytes(), Some(32));
        assert_eq!(u32::from(v.lanes()) * v.lane_type().bytes().unwrap(), 32);
        assert_eq!(v.mask().bytes(), 32);
        assert_eq!(v.mask().lanes(), v.lanes());
        assert_eq!(v.to_string().parse::<VectorType>().unwrap(), v);
        let l = Layout::Scalar(t);
        assert_eq!((l.size(), l.align()), (32, 32));
        let l = Layout::Struct(vec![Layout::Scalar(Type::I8), l, Layout::Scalar(Type::I8)]);
        assert_eq!((l.size(), l.align()), (96, 32));
        assert_eq!(l.leaves()[1].0, 32);
        let lane = v.lane_type();
        let op = if lane.is_float() { "vfadd" } else { "viadd" };
        let m=keel_ir::parse(&format!("func @wide({v}, {v}) -> {v} {{\nblock0(v0: {v}, v1: {v}):\n v2 = {op} v0, v1\n return v2\n}}\n")).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let text = m.to_string();
        keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    }
    for v in VectorType::ALL_128 {
        assert_eq!(v.bytes(), 16);
    }
}
