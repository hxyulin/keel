use super::oracle;

#[test]
fn decoded_snapshot_oracle_preserves_literal_nonmatches_and_immediates() {
    // A 33 byte immediate contains opcode-looking bytes; never scan raw bytes
    // for a set/get pair. Declaration bytes and mismatched locals stay intact.
    let nonmatching = [
        1, 2, 0x7f, 0x41, 0x21, 0x21, 0, 0x20, 1, 0x1a, 0x20, 0, 0x21, 1, 0x01, 0x20, 1, 0x1a, 0x0b,
    ];
    assert_eq!(oracle::fold_body(&nonmatching), (nonmatching.to_vec(), 0));
    // Valid padded index LEBs are preserved from the set, while the matching
    // get uses a different encoding of that same raw index. No re-encoding.
    let original = [1, 1, 0x7f, 0x41, 7, 0x21, 0x80, 0, 0x20, 0, 0x0b];
    let expected = [1, 1, 0x7f, 0x41, 7, 0x22, 0x80, 0, 0x0b];
    assert_eq!(oracle::fold_body(&original), (expected.to_vec(), 1));
    // Existing tee is a barrier, and all bytes around an eligible pair remain.
    let original = [
        1, 2, 0x7f, 0x41, 9, 0x21, 0, 0x22, 1, 0x20, 0, 0x21, 1, 0x20, 1, 0x0b,
    ];
    let expected = [
        1, 2, 0x7f, 0x41, 9, 0x21, 0, 0x22, 1, 0x20, 0, 0x22, 1, 0x0b,
    ];
    assert_eq!(oracle::fold_body(&original), (expected.to_vec(), 1));
}
