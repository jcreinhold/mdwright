#!/usr/bin/env bash
# Run every fuzz target and save inspectable release evidence.
#
# Usage: scripts/fuzz-round.sh [seconds_per_target] [rounds]
#   seconds_per_target  defaults to 600 (10 min).
#   rounds              defaults to 1. Use 3 for release evidence.
#
# Evidence:
#   target/mdwright/release/fuzz-sustained.md
#   target/mdwright/release/fuzz-sustained/logs/*.log
#
# Exit code: 0 if every target exits cleanly and fuzz/artifacts is empty,
# 1 otherwise.

set -u
cd "$(dirname "$0")/.." || exit 1

SECS="${1:-600}"
ROUNDS="${2:-1}"
OUT_DIR="target/mdwright/release/fuzz-sustained"
SUMMARY="target/mdwright/release/fuzz-sustained.md"
TARGETS=(
	fuzz_parse_format
	fuzz_idempotence
	fuzz_structured_idempotence
	fuzz_lint
	fuzz_verbatim_identity
	fuzz_latex_render
	fuzz_latex_translate
	fuzz_markdown_math_translate
	fuzz_unicode_latex_roundtrip
)

fail=0

mkdir -p "${OUT_DIR}/logs"

{
	echo "# Sustained fuzz rounds"
	echo
	echo "Started: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
	echo "Commit: $(git rev-parse HEAD)"
	echo "Rounds: ${ROUNDS}"
	echo "Seconds per target: ${SECS}"
	echo
} >"${SUMMARY}"

for round in $(seq 1 "${ROUNDS}"); do
	echo "## Round ${round}" | tee -a "${SUMMARY}"

	for t in "${TARGETS[@]}"; do
		log="${OUT_DIR}/logs/round-${round}-${t}.log"
		echo "- Running ${t}" | tee -a "${SUMMARY}"

		cargo +nightly fuzz run "${t}" -- -max_total_time="${SECS}" 2>&1 | tee "${log}"
		status="${PIPESTATUS[0]}"

		{
			echo "  - exit: ${status}"
			echo "  - log: ${log}"
		} >>"${SUMMARY}"

		if [ "${status}" -ne 0 ]; then
			fail=1
		fi
	done
done

{
	echo
	echo "Finished: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
	echo
	echo "## Artifact check"
} >>"${SUMMARY}"

echo
echo "=== artifact tally ===" | tee -a "${SUMMARY}"
if find fuzz/artifacts -type f 2>/dev/null | sort | tee -a "${SUMMARY}" | grep -q .; then
	fail=1
else
	echo "No fuzz artifacts found." | tee -a "${SUMMARY}"
fi

echo
if [ "${fail}" -eq 0 ]; then
	echo "round CLEAN - zero artifacts across all targets"
	echo "Evidence written to ${SUMMARY}"
	exit 0
fi
echo "round DIRTY - triage required"
echo "Evidence written to ${SUMMARY}"
exit 1
