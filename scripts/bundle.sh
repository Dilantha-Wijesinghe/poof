#!/usr/bin/env bash
# Builds Poof.app (universal: Apple silicon + Intel), ad-hoc signs it, and
# packages it as a .dmg and a .zip in ./dist.
#
#   scripts/bundle.sh            # universal release build
#   ARCHS=aarch64 scripts/bundle.sh   # faster, Apple silicon only
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="poof"))')"
ARCHS="${ARCHS:-aarch64 x86_64}"
DIST="$ROOT/dist"
APP="$DIST/Poof.app"

echo "==> Building Poof $VERSION for: $ARCHS"
binaries=()
for arch in $ARCHS; do
  target="$arch-apple-darwin"
  rustup target add "$target" >/dev/null 2>&1 || true
  cargo build --release --locked --package poof --target "$target"
  binaries+=("$ROOT/target/$target/release/poof")
done

echo "==> Assembling $APP"
rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${binaries[@]}" -output "$APP/Contents/MacOS/poof"
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp packaging/macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# Releases are signed with Poof's own certificate (scripts/create-signing-identity.sh)
# so macOS keeps permissions such as Full Disk Access across updates. rcodesign
# signs straight from the .p12: no keychain or trust settings are involved.
# Local builds fall back to an ad-hoc signature.
if [[ -n "${POOF_CODESIGN_P12:-}" ]]; then
  echo "==> Signing with $(basename "$POOF_CODESIGN_P12")"
  rcodesign sign --p12-file "$POOF_CODESIGN_P12" \
    --p12-password-file "${POOF_CODESIGN_PASSWORD_FILE:?set POOF_CODESIGN_PASSWORD_FILE}" \
    --code-signature-flags runtime "$APP" >/dev/null
else
  echo "==> Signing (ad-hoc)"
  codesign --force --deep --options runtime --sign - "$APP"
fi
codesign --verify --deep --strict "$APP"
codesign --display --requirements - "$APP" 2>&1 | sed -n 's/^designated => /    identity: /p'

echo "==> Packaging"
staging="$(mktemp -d)"
cp -R "$APP" "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -quiet -volname "Poof $VERSION" -srcfolder "$staging" -fs HFS+ -format UDZO "$DIST/Poof-$VERSION.dmg"
rm -rf "$staging"
(cd "$DIST" && ditto -c -k --keepParent Poof.app "Poof-$VERSION.zip")
(cd "$DIST" && shasum -a 256 "Poof-$VERSION.dmg" "Poof-$VERSION.zip" > SHA256SUMS.txt)

echo "==> Done"
ls -lh "$DIST"
