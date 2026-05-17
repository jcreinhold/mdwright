#!/usr/bin/env bash
# Run one round of every fuzz target and tally new artifacts.
#
# Usage: scripts/fuzz-round.sh [seconds_per_target]
#   seconds_per_target — defaults to 600 (10 min). Use 300 during
#   fix-discovery rounds, 900 for the final three zero-rounds.
#
# Exit code: 0 if every target ended clean (no new artifact under
# fuzz/artifacts/<target>/), 1 otherwise.

set -u
cd "$(dirname "$0")/.." || exit 1

SECS="${1:-600}"
TARGETS=(
	fuzz_parse_format
	fuzz_idempotence
	fuzz_structured_idempotence
	fuzz_lint
	fuzz_verbatim_identity
)

fail=0

for t in "${TARGETS[@]}"; do
	echo "=== ${t}: ${SECS}s ==="
	cargo +nightly fuzz run "${t}" -- -max_total_time="${SECS}" 2>&1 | tail -8
done

echo
echo "=== artifact tally ==="
for t in "${TARGETS[@]}"; do
	if [ -d "fuzz/artifacts/${t}" ]; then
		n=$(find "fuzz/artifacts/${t}" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')
	else
		n=0
	fi
	echo "${t}: ${n}"
	if [ "${n}" -gt 0 ]; then
		fail=1
	fi
done

echo
if [ "${fail}" -eq 0 ]; then
	echo "round CLEAN — zero artifacts across all targets"
	exit 0
fi
echo "round DIRTY — triage required"
exit 1
