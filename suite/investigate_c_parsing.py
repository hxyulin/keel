"""Offline retained-stack attribution and copied-source parser counter generator.

Neither command builds, runs a compiler, accesses a remote host, nor times code.
Generated Rust is a diagnostic prototype: compile/parity validation is deferred.
"""
import argparse
import collections
import gzip
import hashlib
import io
import json
import pathlib
import subprocess
import tarfile
import tempfile

from decode_frontend_refresh import decode

BASE = "e85816ff651e912580bb8bd800c6c4884047a73d"
ROOT = pathlib.Path(__file__).resolve().parent.parent
RETAINED = ROOT / "suite/reports/c-frontend-refresh-20261016"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def analyze(output):
    output.mkdir(parents=True, exist_ok=True)
    provenance = json.loads((RETAINED / "provenance.json").read_text())
    saved = json.loads(gzip.decompress((RETAINED / "decoded.json.gz").read_bytes()))
    seal = json.loads((RETAINED / "seal.json").read_text())
    archive = RETAINED / "raw-evidence.tar.gz"
    assert sha(archive.read_bytes()) == seal["raw_archive_sha256"]
    corpus_manifest = {}
    assert sha((RETAINED / "corpus.tar.gz").read_bytes()) == seal["corpus_archive_sha256"]
    with tarfile.open(RETAINED / "corpus.tar.gz") as corpus:
        for member in corpus:
            if member.isfile():
                corpus_manifest[member.name] = {"sha256": sha(corpus.extractfile(member).read()), "bytes": member.size}
    assert corpus_manifest == {n: v for group in provenance["input_manifest"].values() for n, v in group.items()}
    result = {
        "analysis_source_revision": BASE,
        "frontend_source_sha256": {str(p.relative_to(ROOT)): sha(p.read_bytes()) for p in sorted((ROOT / "crates/keel-c/src").glob("*.rs"))},
        "corpus_manifest_verified_files": len(corpus_manifest),
        "analysis_harness_sha256": sha(pathlib.Path(__file__).read_bytes()),
        "decoder_sha256": sha((ROOT / "suite/decode_frontend_refresh.py").read_bytes()),
        "retained_harness_revision": provenance["harness_head"],
        "retained_source_revision": provenance["base"],
        "retained_production_binary": provenance["production_binary"],
        "retained_binaries": provenance["binaries"],
        "raw_archive_sha256": seal["raw_archive_sha256"],
        "corpus_archive_sha256": seal["corpus_archive_sha256"],
        "retained_input_manifest": provenance["input_manifest"],
        "current_frontend_source_matches_retained": not bool(git(
            "diff", provenance["base"], BASE, "--", "crates/keel-c/src")),
        "captures": {},
    }
    needles = {
        "decl_spec": "<keel_c::parse::Parser>::decl_spec",
        "declarator_layer": "<keel_c::parse::Parser>::declarator_layer",
        "intern": "<keel_c::strings::Strings>::intern",
        "init_list": "<keel_c::parse::Parser>::init_list",
        "lookup_value": "<keel_c::parse::Parser>::lookup_value",
        "cast": "<keel_c::parse::Parser>::cast",
        "type_eq": "<keel_c::types::Ty as core::cmp::PartialEq>::eq",
        "type_clone": "<keel_c::types::Ty as core::clone::Clone>::clone",
        "type_drop": "core::ptr::drop_glue::<keel_c::types::Ty>",
        "type_node": "<keel_c::types::TypeNodes>::node",
        "member_find": "<keel_c::types::StructDef>::find",
        "type_size": "<keel_c::types::Types>::size",
        "type_align": "<keel_c::types::Types>::align",
        "layout_struct": "keel_c::types::layout_struct",
        "expr_clone": "<keel_c::ast::Expr as core::clone::Clone>::clone",
        "insert_inst": "<keel_ir::function::Function>::insert_inst",
        "ssa_read": "<keel_frontend::FunctionBuilder>::read",
    }
    with tarfile.open(archive) as raw, tempfile.TemporaryDirectory() as tmp:
        for key, expected in saved.items():
            name = expected["raw_script"]
            data = raw.extractfile("evidence/" + name).read()
            path = pathlib.Path(tmp) / name
            path.write_bytes(data)
            actual = decode(path, "preprocess_linux::main" if key.startswith("preprocess-") else "profile::main")
            for field in ["raw_script", "totals", "periods"]:
                assert actual[field] == expected[field], (key, field)
            for field in ["self_weight", "self_samples", "inclusive_weight", "folded_weight"]:
                assert dict(actual[field]) == dict(expected[field]), (key, field)
            weights = collections.Counter()
            # These are overlapping observed-prefix selectors except the
            # interner partition, which is explicitly mutually exclusive.
            for chain, weight in actual["folded_weight"]:
                frames = chain.split(";")  # Stored leaf first.
                parser = any(f.startswith("<keel_c::parse::Parser>::") for f in frames)
                lexer = "keel_c::lex::tokenize" in frames
                irgen = any(f.startswith("<keel_c::irgen::") for f in frames)
                for label, condition in [("parser_observed", parser), ("lexer_observed", lexer), ("irgen_observed", irgen)]:
                    if condition:
                        weights[label] += weight
                if needles["intern"] in frames:
                    context = "mixed" if lexer and parser else "lexer" if lexer else "parser" if parser else "unclassified"
                    weights["intern_" + context] += weight
                if "hash" in frames[0].lower() or "sip" in frames[0].lower():
                    weights["hash_leaf_all"] += weight
                    if parser:
                        weights["hash_leaf_parser_observed"] += weight
                for label in ["type_eq", "type_clone", "type_drop", "type_size", "type_align", "expr_clone"]:
                    if needles[label] in frames and parser:
                        weights[label + "_parser_observed"] += weight
            total = actual["totals"]["weight"]
            selfw, incw = dict(actual["self_weight"]), dict(actual["inclusive_weight"])
            assert sum(weights["intern_" + c] for c in ["mixed", "lexer", "parser", "unclassified"]) == incw.get(needles["intern"], 0)
            result["captures"][key] = {
                "raw_script": name, "raw_script_sha256": sha(data),
                "totals": actual["totals"],
                "selectors": {n: {"weight": w, "whole_capture_pct": 100*w/total} for n,w in sorted(weights.items())},
                "symbols": {n: {"symbol": s, "self_weight": selfw.get(s, 0),
                    "inclusive_weight": incw.get(s, 0),
                    "self_pct": 100*selfw.get(s, 0)/total,
                    "inclusive_pct": 100*incw.get(s, 0)/total} for n,s in needles.items()},
            }
    write_json(output / "attribution.json", result)
    print("Re-decoded and matched all 12 retained captures; wrote", output / "attribution.json")


def generate(destination):
    """Export committed source and instrument ONLY that new private copy."""
    if destination.exists():
        raise ValueError("destination must not exist")
    # Refuse to put generated copies anywhere inside this working tree.
    destination = destination.resolve()
    if destination == ROOT or ROOT in destination.parents:
        raise ValueError("diagnostic destination must be outside the repository")
    archive = git("archive", BASE, "Cargo.toml", "Cargo.lock", ".cargo", "crates")
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        for m in source:
            p = pathlib.PurePosixPath(m.name)
            if p.is_absolute() or ".." in p.parts or not (m.isfile() or m.isdir()):
                raise ValueError("unexpected archive member: " + m.name)
        destination.mkdir(parents=True)
        for m in source:
            path = destination / m.name
            if m.isdir():
                path.mkdir(parents=True, exist_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(source.extractfile(m).read())
    pristine = {str(p.relative_to(destination)): sha(p.read_bytes()) for p in destination.rglob("*") if p.is_file()}
    labels = []
    box_sites = {}

    def counter(label, amount="1"):
        if label not in labels:
            labels.append(label)
        return f"crate::parse_probe::add({labels.index(label)}, {amount});"

    def replace(file, old, new):
        p = destination / "crates/keel-c/src" / file
        text = p.read_text()
        assert text.count(old) == 1, (file, old, text.count(old))
        p.write_text(text.replace(old, new))

    entries = {
        "parse.rs": [
            ("fn lookup(&self, name: &str) -> Option<&Sym> {", "name_lookup"),
            ("fn bind_at(&mut self, depth: usize, name: &'a str, sym: Sym) {", "name_bind"),
            ("pub(crate) fn pop_scope(&mut self) {", "scope_pop"),
            ("pub(crate) fn is_typename(&self) -> bool {", "is_typename"),
            ("pub(crate) fn typedef_type(&self, name: &str) -> Option<Ty> {", "typedef_presence_query"),
            ("fn decl_spec(&mut self) -> R<DeclSpec> {", "decl_spec"),
        ],
        "expr.rs": [
            ("pub(crate) fn mk<'a>(kind: ExprKind<'a>, ty: Ty, loc: Loc) -> Expr<'a> {", "expr_mk_not_all_constructors"),
            ("pub(crate) fn cast(&mut self, e: Expr<'a>, ty: Ty) -> Expr<'a> {", "implicit_cast_attempt"),
            ("pub(crate) fn convert_for_assign(&mut self, e: Expr<'a>, ty: &Ty) -> R<Expr<'a>> {", "assignment_conversion"),
        ],
        "types.rs": [
            ("pub fn node(&mut self, ty: Ty) -> Rc<Ty> {", "type_node_request"),
            ("pub fn size(&self, ty: &Ty) -> u64 {", "type_size_recursive_entry"),
            ("pub fn align(&self, ty: &Ty) -> u64 {", "type_align_recursive_entry"),
        ],
    }
    for file, pairs in entries.items():
        for anchor, label in pairs:
            replace(file, anchor, anchor + "\n        " + counter(label))
    replace("parse.rs", "Some(Sym::Typedef(i)) => Some(self.typedefs[*i].ty.clone()),",
        "Some(Sym::Typedef(i)) => { " + counter("typedef_presence_clone") + " Some(self.typedefs[*i].ty.clone()) },")
    replace("parse.rs", "find_map(|s| s.tags.get(&tag))",
        "find_map(|s| { " + counter("tag_scope_probe") + " s.tags.get(&tag) })")
    replace("parse.rs", "find_map(|s| s.enums.get(t))",
        "find_map(|s| { " + counter("enum_scope_probe") + " s.enums.get(t) })")
    replace("types.rs", "other => return Rc::new(other),", "other => { " + counter("type_node_composite_allocation") + " return Rc::new(other) },")
    replace("types.rs", "slot.get_or_insert_with(|| Rc::new(ty)).clone()",
        "if slot.is_none() { " + counter("type_node_leaf_allocation") + " }\n        slot.get_or_insert_with(|| Rc::new(ty)).clone()")
    replace("types.rs", "for m in &self.members {", "for m in &self.members {\n            " + counter("member_scan_comparison"))
    replace("expr.rs", "if e.ty == ty {\n            return e;", "if e.ty == ty {\n            " + counter("implicit_cast_equal_return") + "\n            return e;")
    replace("expr.rs", "return mk(ExprKind::Int(normalize(v, &ty)), ty, e.loc);",
        counter("implicit_cast_constant_fold") + "\n            return mk(ExprKind::Int(normalize(v, &ty)), ty, e.loc);")
    # Two equal spellings (explicit and implicit cast). Anchor the implicit tail.
    replace("expr.rs", "mk(ExprKind::Cast(Box::new(e)), ty, loc).with_sugar(sugar)\n    }\n\n    /// The integer promotions.",
        counter("implicit_cast_ast_created") + "\n        mk(ExprKind::Cast(Box::new(e)), ty, loc).with_sugar(sugar)\n    }\n\n    /// The integer promotions.")
    replace("init.rs", "let one: Vec<InitItem<'a>> = items[first..].to_vec();",
        counter("initializer_snapshot_items", "(items.len() - first) as u64") + "\n            "
        + counter("initializer_snapshot_regions") + "\n            "
        + counter("initializer_extra_ranges", "n.saturating_sub(1) as u64")
        + "\n            let one: Vec<InitItem<'a>> = items[first..].to_vec();")
    replace("strings.rs", "pub fn intern(&mut self, text: &str) -> &'a str {",
        "pub fn intern(&mut self, text: &str) -> &'a str {\n        " + counter("intern_calls") + "\n        " + counter("intern_input_bytes", "text.len() as u64"))
    replace("strings.rs", "let name = self.arena.alloc_str(text);", counter("intern_misses") + "\n        let name = self.arena.alloc_str(text);")
    # AST boxes at explicit sites only: no claims about Vecs, Rc, or derived Clone.
    for file in ["parse.rs", "expr.rs", "init.rs"]:
        p = destination / "crates/keel-c/src" / file
        text = p.read_text()
        chunks = text.split("Box::new(")
        source_text = git("show", BASE + ":crates/keel-c/src/" + file).decode()
        source_chunks = source_text.split("Box::new(")
        assert len(chunks) == len(source_chunks)
        text = chunks[0]
        for i, tail in enumerate(chunks[1:]):
            label = f"explicit_box:{file}:{i}"
            prefix = "Box::new(".join(source_chunks[:i+1])
            box_sites[label] = {"source_line": prefix.count("\n") + 1,
                "argument_prefix": source_chunks[i+1][:100]}
            idx = len(labels)
            labels.append(label)
            text += f"crate::parse_probe::boxed({idx}, " + tail
        p.write_text(text)
    # Set phases in the copied public API. Guard changes phase before automatic
    # parser cleanup on success; error-path cleanup remains in its failing phase.
    replace("lib.rs", "mod ast;", "mod ast;\npub mod parse_probe;")
    replace("lib.rs", "pub fn compile(src: &str, path: &str, opts: Options) -> Result<keel_ir::Module, Diagnostic> {",
        "pub fn compile(src: &str, path: &str, opts: Options) -> Result<keel_ir::Module, Diagnostic> {\n    crate::parse_probe::phase(0);")
    replace("lib.rs", "let mut parser = parse::Parser::new(toks, opts.target, text);",
        "crate::parse_probe::phase(1);\n    let mut parser = parse::Parser::new(toks, opts.target, text);")
    replace("lib.rs", "    irgen::Gen::new(&parser, &files, opts.debug_info)",
        "    crate::parse_probe::phase(2);\n    let result = irgen::Gen::new(&parser, &files, opts.debug_info)")
    replace("lib.rs", "        .map_err(|e| diagnostic(&files, e))\n}",
        "        .map_err(|e| diagnostic(&files, e));\n    crate::parse_probe::phase(3);\n    result\n}")
    # Fixed arrays keep the source counters themselves allocation-free.
    probe = '''//! Generated diagnostic only. No clocks; never use for latency.
use std::cell::RefCell;
const N: usize = @N@;
thread_local! { static COUNTS: RefCell<(usize, [[u64; N]; 4])> = const { RefCell::new((0, [[0; N]; 4])) }; }
pub fn phase(p: usize) { COUNTS.with(|c| c.borrow_mut().0 = p); }
pub fn add(i: usize, n: u64) { COUNTS.with(|c| { let mut c = c.borrow_mut(); let p = c.0; c.1[p][i] += n; }); }
pub fn boxed<T>(i: usize, value: T) -> Box<T> { add(i, 1); Box::new(value) }
pub fn take() -> [[u64; N]; 4] { COUNTS.with(|c| std::mem::replace(&mut c.borrow_mut().1, [[0; N]; 4])) }
'''.replace("@N@", str(len(labels)))
    (destination / "crates/keel-c/src/parse_probe.rs").write_text(probe)
    helper = '''//! No timing. Print counters, verify IR, retain complete Debug IR/diagnostics.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 { return Err("usage: parse_counts TARGET DEBUG INPUT OUTPUT".into()); }
    let target = match args[1].as_str() {
        "x86_64" => keel_ir::Target::X86_64LinuxGnu,
        "aarch64" => keel_ir::Target::Aarch64AppleDarwin,
        _ => return Err("unknown target".into()),
    };
    let opts = keel_c::Options { target, debug_info: args[2].parse()? };
    let source = std::fs::read_to_string(&args[3])?;
    let result = keel_c::compile(&source, &args[3], opts);
    let counts = keel_c::parse_probe::take();
    println!("{:?}", counts);
    match result {
        Ok(module) => {
            keel_ir::verify_module(&module).map_err(|e| format!("{e:?}"))?;
            std::fs::write(&args[4], format!("{module:?}"))?;
        }
        Err(d) => { std::fs::write(&args[4], format!("{d:?}"))?; }
    }
    Ok(())
}
'''
    # Target spelling comes from committed IR source, checked without compiling.
    target_source = (destination / "crates/keel-ir/src/target.rs").read_text()
    assert "X86_64LinuxGnu" in target_source and "Aarch64AppleDarwin" in target_source
    (destination / "crates/keel-c/examples/parse_counts.rs").write_text(helper)
    modified = {str(p.relative_to(destination)): sha(p.read_bytes()) for p in destination.rglob("*") if p.is_file()}
    write_json(destination / "parse-probe-manifest.json", {
        "source_revision": BASE, "generator_sha256": sha(pathlib.Path(__file__).read_bytes()),
        "phases": ["lex", "parse", "irgen", "cleanup"], "labels": labels, "explicit_box_sites": box_sites,
        "changed_files": sorted(n for n in modified if pristine.get(n) != modified[n]),
        "pristine_sha256": pristine, "generated_sha256": modified,
        "validation": "UNBUILT: requires an idle host, normal/debug IR and diagnostic parity",
        "limitations": ["Error-path cleanup remains in the failing phase", "Counts are not time or allocation totals",
            "Explicit AST boxes exclude derived clones; mk excludes direct Expr constructors",
            "Ty derived clone/equality and Rc increments are not counted", "Tag probes count visited scopes, not failed hash comparisons",
            "Array size/align recursive calls count separately", "Public compile only; do not use profile::compile"],
    })
    print("Generated unbuilt diagnostic copy:", destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("analyze").add_argument("output", type=pathlib.Path)
    sub.add_parser("generate").add_argument("destination", type=pathlib.Path)
    args = parser.parse_args()
    if args.command == "analyze":
        analyze(args.output)
    else:
        generate(args.destination)
