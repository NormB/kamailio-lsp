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
# Everything lands under .proof/ (gitignored). A local run reuses what
# is already there, so the build happens once. CI does NOT: the only
# cache in the workflow is Swatinem/rust-cache, which caches Cargo
# artefacts and not this directory, so every CI run re-clones all
# three trees and rebuilds $TAG from scratch. Said the other way
# round, adding a release costs CI one shallow clone per run — which
# is affordable precisely because the older ones are never built.
set -euo pipefail

TAG="${KAMAILIO_TAG:-6.1.4}"
# The releases the versioned catalogue covers: the newest of each live
# line, oldest first. Only $TAG is ever BUILT — the older ones are
# harvested from source alone, which is all the catalogue needs, so
# adding a release costs a shallow clone rather than a compile.
OLDER_TAGS="${KAMAILIO_OLDER_TAGS:-5.8.8 6.0.7}"
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

# Source-only checkouts of the older supported releases. The
# versioned catalogue is base-plus-deltas across these, and its
# round-trip proof needs every one of them present.
TREES=""
for t in $OLDER_TAGS; do
	d="$ROOT/kamailio-$t"
	if [ ! -d "$d/src/modules" ]; then
		log "cloning Kamailio $t (source only) into $ROOT"
		rm -rf "$d"
		git clone -q --depth 1 --branch "$t" \
			https://github.com/kamailio/kamailio.git "$d"
	fi
	TREES="${TREES:+$TREES,}$t=$d"
done
TREES="${TREES:+$TREES,}$TAG=$SRC"

emit() {
	printf 'KAMAILIO_LSP_TEST_TREES=%s\n' "$TREES"
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
