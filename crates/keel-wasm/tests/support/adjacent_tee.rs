// Independent byte-span oracle: copy original bytes except decoded adjacent
// set/get pairs. In particular, preserve declarations and all other immediates.
pub fn fold_body(raw: &[u8]) -> (Vec<u8>, usize) {
    use wasmparser::{BinaryReader, FunctionBody, Operator};
    let body = FunctionBody::new(BinaryReader::new(raw, 0));
    let mut reader = body.get_operators_reader().unwrap();
    let mut spans = Vec::new();
    let mut result = raw[..reader.original_position()].to_vec();
    while !reader.eof() {
        let start = reader.original_position();
        let op = reader.read().unwrap();
        spans.push((op, start, reader.original_position()));
    }
    let mut i = 0;
    let mut pairs = 0;
    while i < spans.len() {
        if let Operator::LocalSet { local_index: set } = spans[i].0 {
            if i + 1 < spans.len()
                && matches!(spans[i + 1].0, Operator::LocalGet { local_index } if local_index == set)
            {
                result.push(0x22); // local.tee has the identical raw index LEB.
                result.extend_from_slice(&raw[spans[i].1 + 1..spans[i].2]);
                pairs += 1;
                i += 2;
                continue;
            }
        }
        result.extend_from_slice(&raw[spans[i].1..spans[i].2]);
        i += 1;
    }
    (result, pairs)
}
