# x86 coverage ledger definitions

The [generated ledger](../../docs/targets/x86_64/coverage/README.md) and
[JSON records](../../docs/targets/x86_64/coverage/coverage.json) track structural facts
from the pinned import, public identity/adapter schemas, feature policy,
profiles and system-intrinsic recipes. Regenerate them with the existing
`python3 tools/x86-isa/generate.py` command. Its `--check` mode checks both
reports along with Rust outputs. Ordinary Cargo builds use the checked-in data.

## What each layer means

| Layer | Recorded fact | What it does not establish |
|---|---|---|
| Import | Every pinned Zydis record and local extension has an origin, source index and content-derived ID. | Completeness against the full upstream ISA, or encodings outside the import. |
| Catalogue | A source record is emitted into `isa::FORMS`, with its exact table index, or has explicit exclusion reasons. | Architectural validity or acceptance by every operand handler. |
| Decoder | The form is registered in the decode index and enabled or disabled by the current decode policy. | A successful byte witness, complete operand decoding or rejection of every invalid encoding. |
| Assembler candidates | Declarative identities/adapters name the same source mnemonic and satisfy the recorded encoding/selector constraints. | Actual wrapper-dispatch precedence, all dialect normalization, operand matching, canonical encoding choice or successful assembly of that form. |
| Compiler candidates | One of the 45 declared system recipes structurally refers to the form, or to a component of STI;HLT. | Compiler coverage outside that schema, the entire input domain, privilege, entry/return ABI or successful execution. |
| Feature policy | An explicit requirement set, null policy or missing classification, plus built-in profile containment when declared. | Host detection, enabled OS state, privilege or completeness of a CPU profile's instruction APIs. |
| Validation | `untracked`. No per-form results are ingested in this increment. | Absence of existing tests or historical qualification. Test-file presence is never converted into a pass claim. |

Candidate links deliberately remain candidates. A shared mnemonic can name
legacy and VEX interpretations or several operand forms. The `V3Opcode` API
family includes legacy instructions, so its name is not an encoding tag. Fixed
adapter selection may choose one of several source alternatives. Matching
records alone must not turn these cases into claimed form coverage.

Some aliases and the parameterized Core grammar still normalize instructions
in handwritten code. The ledger records dynamic identity shapes separately.
An empty candidate list means no link was derived from these declarative
sources; it does not mean the assembler rejects the instruction. Likewise,
forms without tracked compiler candidates may already be emitted by handwritten
lowering, ordinary IR selection or owned assembly adapters.

## Source identity and reproducibility

Each record ID is `origin:sha256`, using sorted-key compact JSON and excluding
only the optional comment field. It remains stable if unrelated records are
reordered. `source_index` points into the original `zydis.json` or
`keel-extensions.json` array. Changing a form's actual fields changes its ID.
Duplicate IDs fail generation instead of silently merging records.

The JSON includes the upstream pin and hashes of all consumed data/policy
files. Passed input data must match those files before a ledger is produced.
No timestamps, local build paths or current Git HEAD enter generated output.
`table_index` follows the producer's stable mnemonic ordering, including ties.

The catalogue generator and ledger share `exclusion_reasons()`. The current
filters exclude requested alternate AMD/KNC/MPX interpretations. REX2
permission is retained in the generated forms rather than excluded.
All applicable reasons are retained, so per-reason totals may overlap. Forms
outside the pinned selections, including other EVEX families, MVEX/XOP/3DNow encodings and explicit
non-64-bit source records, are named as out of scope rather than assigned
invented missing-form counts. ISA labels and encoding-family tags are separate.

The short UD0 compatibility form remains in the catalogue and decode index,
but is disabled by decoder policy. Thus indexed and enabled are distinct facts.
An absent upstream CPL field remains null/unknown, including LGDT. It is never
interpreted as permission to execute at any privilege level.

## Feature-policy audit

Requirements use the same prerequisite closure as the generated feature code,
including the XMM VPCLMULQDQ policy override. Profile columns show containment
of that declared requirement set. An empty profile list can simply mean an
optional capability is not implied by v1/v2/v3.

A missing policy key is `unclassified`, not baseline. An explicit empty list is
baseline optional-capability policy. A null value is `excluded_policy`, which
is distinct from exclusion from the production catalogue. The ledger does not
change those policies: existing generated runtime lookup currently falls back
to `Features::NONE` for missing/null cases. For example, KNC-labelled records
can remain registered where the source lacks an exclusion filter. The table
shows both dimensions so that policy and import decisions can be audited.

The first report finds 85 unclassified policy keys. That count includes baseline
system labels and newer optional families; it is not a count of unsupported
instruction sets. Classifying those keys requires separate policy review.

## Queries and validation

For example, inspect imported RSM records without guessing their support:

```sh
python3 - <<'PY'
import json
from pathlib import Path
ledger = json.loads(Path('docs/targets/x86_64/coverage/coverage.json').read_text())
print(json.dumps([f for f in ledger['forms'] if f['mnemonic'] == 'rsm'], indent=2))
PY
```

The query uses source mnemonics, not every public alias. Numeric compiler
candidate IDs join to `compiler_bindings`; assembler candidate IDs join to
`assembler_bindings`. Those IDs identify policy declarations, not necessarily
the wrapper selected by `Opcode::parse`. An adjacent recipe has `components`
rather than a claim that one source form implements the whole pair.

Python tests check complete source accounting, stable IDs, exclusion reasons,
ordering, bidirectional links, profile/feature distinctions, unknown privilege
and validation states, and recipe/component coverage. The Rust integration test
checks every included record's locator and key encoding/operand fields against
the actual `isa::FORMS` array and decode index. It also checks every recorded
static spelling with `Opcode::parse` and the tracked compiler names/IDs with
`Intrinsic::from_name`. These are structural checks, not per-form byte or runtime
qualification.

Existing semantic evidence stays in [system support](../../docs/targets/README.md)
and [transition contracts](../../docs/targets/x86_64/system/transitions.md). A future result
import should attach explicit witness/campaign scope and artifact digests to
form IDs. It must distinguish encoding comparisons, actual decode witnesses,
emulated execution and native execution, including expected negative controls.
No such results are inferred from prose or file names here.

The [remaining system instruction audit](../../docs/targets/x86_64/system/README.md) uses
`python3 tools/x86-isa/audit_system.py` to join a manual system roster and a
category/CPL sweep to this ledger. That read-only report keeps the original
candidate/validation distinctions; handwritten API and contract findings live
in the audit document rather than being inferred from empty links.

## Implementation verification

The workspace run passed 2,711 tests with zero failures and 13 ignored across
497 reported groups. All 79 generator tests, both freshness checks, workspace
compilation and diff checks passed. Existing generated Rust outputs remain
unchanged. Logs are `/tmp/keel-coverage-ledger-workspace.log`,
`/tmp/keel-coverage-ledger-python.log` and `/tmp/keel-coverage-ledger-check.log`.
These global checks do not populate individual form execution verdicts.

## Separate AVX-512F/VL table

Schema version 2 adds `avx512_forms`, with locators into `evex.json` and indices
into `isa::EVEX_FORMS`. The original `forms` array and its summary counters
continue to describe the legacy/VEX table. The new section contains 1,884 EVEX
records and 15 VEX-encoded mask records also present in the original import.
Those 15 records retain identical content IDs; they are reused, not additional
unique imported instructions. Requirements and the public `EvexMnemonic`
binding are recorded per entry. Decoder registration names the EVEX index or
the shared VEX index. Validation remains `untracked`; generated source witnesses
are not ingested as per-form qualification verdicts.
