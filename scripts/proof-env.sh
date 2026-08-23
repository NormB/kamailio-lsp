#!/usr/bin/env bash
# Provision the real Kamailio tree, wiki checkout and binary that the
# proof suite runs against.
#
# A skipped test is a failed test in this repo: the suite refuses to
# run without this environment rather than quietly reporting green
# while proving nothing.  This script is how you satisfy it, and CI
# runs this same script — the gate and its fixer derive from one rule,
# so a green CI means the proofs actually ran.
#
#   eval "$(scripts/proof-env.sh)"   # local shell
#   scripts/proof-env.sh             # in CI: appends to $GITHUB_ENV
#
# Everything lands under .proof/ (gitignored) and is reused on the
# next run; CI caches that directory keyed on the tag below.
set -euo pipefail

TAG="${KAMAILIO_TAG:-6.1.4}"
ROOT="${PROOF_ROOT:-${PWD}/.proof}"
SRC="$ROOT/kamailio-$TAG"
WIKI="$ROOT/kamailio-wiki"
INST="$ROOT/inst-$TAG"

# The only modules any test loads.  Building the full set is minutes
# of compile for no extra coverage.
MODULES="sl tm pv htable"

log() { printf '%s\n' "$*" >&2; }

if [ ! -d "$WIKI/docs/cookbooks" ]; then
	log "cloning kamailio-wiki into $ROOT"
	rm -rf "$WIKI"
	mkdir -p "$ROOT"
	git clone -q --depth 1 \
		https://github.com/kamailio/kamailio-wiki.git "$WIKI"
fi

if [ ! -x "$INST/sbin/kamailio" ]; then
	log "provisioning Kamailio $TAG into $ROOT"
	rm -rf "$SRC" "$INST"
	mkdir -p "$ROOT"
	git clone -q --depth 1 --branch "$TAG" \
		https://github.com/kamailio/kamailio.git "$SRC"

	# `skip_modules` is appended to the default exclusion list, so
	# listing everything we do not need leaves exactly the core plus
	# $MODULES
	skip=""
	for m in $(ls "$SRC/src/modules"); do
		case " $MODULES " in
		*" $m "*) ;;
		*) skip="$skip $m" ;;
		esac
	done

	make -C "$SRC" FLAVOUR=kamailio PREFIX="$INST" \
		skip_modules="$skip" cfg >/dev/null
	make -C "$SRC" -j"$(nproc)" all >/dev/null
	make -C "$SRC" install >/dev/null
	log "built $("$INST/sbin/kamailio" -V 2>&1 | head -1)"
fi

# lib vs lib64 varies by platform; ask the filesystem rather than guess
TM="$(find "$INST" -name tm.so -print -quit)"
[ -n "$TM" ] || {
	log "tm.so missing under $INST — the build did not install modules"
	exit 1
}
MPATH="$(dirname "$TM")/"

emit() {
	printf 'KAMAILIO_LSP_TEST_TREE=%s\n' "$SRC"
	printf 'KAMAILIO_LSP_TEST_WIKI=%s\n' "$WIKI"
	printf 'KAMAILIO_LSP_TEST_BIN=%s\n' "$INST/sbin/kamailio"
	printf 'KAMAILIO_LSP_TEST_MPATH=%s\n' "$MPATH"
}

if [ -n "${GITHUB_ENV:-}" ]; then
	emit >>"$GITHUB_ENV"
	log "proof environment written to \$GITHUB_ENV"
else
	emit | sed 's/^/export /'
fi
