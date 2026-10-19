#!/usr/bin/env bash
# Runs the unchanged pattern-lab/page-backing harness for each family config:
# check-only gates, analysis, then the timed campaign and its analysis.
# Stops at the first failure; every output directory is new and retained.
# Usage (as root, e.g. `sudo -n`): linux_baseline_pagebacking.sh ROOT FAMILY...
# where ROOT/page/FAMILY.json exists for each FAMILY.
set -u
root=$1; shift
harness=$root/src/suite/pattern-lab/page-backing
log=$root/page/driver.log
note() { echo "$(date -u +%FT%T.%NZ) $*" | tee -a "$log"; }
note "start harness_sha=$(sha256sum "$harness"/run.py "$harness"/analyze.py "$harness"/map_text.c | awk '{print $1}' | tr '\n' ' ')"
for family in "$@"; do
  config=$root/page/$family.json
  for phase in check campaign; do
    out=$root/page/$family-$phase
    extra=(); [ "$phase" = check ] && extra=(--check-only)
    note "$family $phase begin config_sha=$(sha256sum "$config" | cut -d' ' -f1)"
    env -i PATH=/usr/bin:/bin LANG=C LC_ALL=C PYTHONDONTWRITEBYTECODE=1 \
      /usr/bin/python3 -u "$harness/run.py" --config "$config" --output "$out" --execute "${extra[@]}" \
      > "$out.stdout" 2> "$out.stderr"
    rc=$?
    note "$family $phase run rc=$rc"
    [ $rc -eq 0 ] || { note "STOP $family $phase"; exit 1; }
    env -i PATH=/usr/bin:/bin PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 "$harness/analyze.py" "$out" \
      > "$out.analysis" 2>&1
    rc=$?
    note "$family $phase analyze rc=$rc status=$(grep -o '"status": "[A-Z_]*"' "$out.analysis" | head -1)"
    [ $rc -eq 0 ] || { note "STOP $family $phase analysis"; exit 1; }
  done
done
note "ALL_COMPLETE"
