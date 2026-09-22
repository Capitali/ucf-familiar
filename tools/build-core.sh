#!/bin/bash
# build-core.sh — the embeddable pilot core for the Apple shell.
# Produces ios/FamiliarCore/: generated Swift bindings + FamiliarCore.xcframework
# (device + simulator static libs) from THIS repo's `ucf-pilot`, so the doctrine the
# app answers with is the doctrine the fleet flies. Run from the repo root.
#
# One function crosses: whiskerAdvise(inputJson:) — see crates/core-ffi/src/lib.rs.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=ios/FamiliarCore
GEN="$OUT/Generated"

# The core's minimum OS is the app's declared floor (ios/project.yml deploymentTarget)
# — pinned here so the archive can never silently carry whatever the building machine's
# SDK defaulted to. Both rustc's Apple targets and cc-built C objects honor this.
FLOOR=26.0
export IPHONEOS_DEPLOYMENT_TARGET="$FLOOR"

# The generator lives behind the `cli` feature so the device archive never carries it
# (see crates/core-ffi/Cargo.toml). Only these two host steps ask for it.
echo "== host dylib (for binding generation) =="
cargo build -p ucf-core-ffi --release --features cli

echo "== swift bindings =="
rm -rf "$GEN" && mkdir -p "$GEN"
cargo run -p ucf-core-ffi --release --features cli --bin uniffi-bindgen -- generate \
  --library target/release/libfamiliar_core.dylib \
  --language swift --out-dir "$GEN"

echo "== device + simulator static libs (min iOS $FLOOR) =="
cargo build -p ucf-core-ffi --release --target aarch64-apple-ios
cargo build -p ucf-core-ffi --release --target aarch64-apple-ios-sim

echo "== xcframework =="
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
HDR="$WORK/headers"
mkdir -p "$HDR"
cp "$GEN"/familiar_coreFFI.h "$HDR"/
cp "$GEN"/familiar_coreFFI.modulemap "$HDR"/module.modulemap

# Strip DWARF from a COPY of each slice — never from cargo's own output, which cargo
# would then hand to the next build as if it were fresh. `-S` drops debug sections and
# keeps the symbol table, so the app's dSYM still names every Rust frame; what is lost
# is file-and-line inside the core, and that buys ~6 MB a slice on an archive that is
# checked in and rebuilt with every change to doctrine.
# (Each copy keeps the name libfamiliar_core.a — xcframework slices are named after
# the file they were built from, and the app links -lfamiliar_core.)
for arch in aarch64-apple-ios aarch64-apple-ios-sim; do
  mkdir -p "$WORK/$arch"
  cp "target/$arch/release/libfamiliar_core.a" "$WORK/$arch/libfamiliar_core.a"
  strip -S "$WORK/$arch/libfamiliar_core.a"
done

rm -rf "$OUT/FamiliarCore.xcframework"
xcodebuild -create-xcframework \
  -library "$WORK/aarch64-apple-ios/libfamiliar_core.a" -headers "$HDR" \
  -library "$WORK/aarch64-apple-ios-sim/libfamiliar_core.a" -headers "$HDR" \
  -output "$OUT/FamiliarCore.xcframework"

echo "== verify: no object in either slice requires newer than iOS $FLOOR =="
# Objects OLDER than the floor are fine (Rust ships its precompiled std at the
# toolchain's own minimum — those objects load anywhere at or above it). What must
# never happen is an object NEWER than the floor: an app declaring 26.0 that carries a
# 26.5-min object only draws a linker WARNING, and then fails on a 26.0 device.
for lib in "$OUT"/FamiliarCore.xcframework/ios-arm64/libfamiliar_core.a \
           "$OUT"/FamiliarCore.xcframework/ios-arm64-simulator/libfamiliar_core.a; do
  stray=$(otool -l "$lib" | awk -v floor="$FLOOR" \
    '/minos/ { if ($2 + 0 > floor + 0) print $2 }' | sort -u)
  if [ -n "$stray" ]; then
    echo "✗ $lib carries objects requiring newer than iOS $FLOOR: $stray" >&2
    exit 1
  fi
done
echo "✓ $OUT ready ($(du -sh "$OUT/FamiliarCore.xcframework" | cut -f1)) — link the xcframework + compile Generated/familiar_core.swift into the app"
