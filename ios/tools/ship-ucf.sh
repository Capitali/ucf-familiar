#!/bin/bash
# ship-ucf.sh <build-number> — archive → export (App Store) → upload UCF Familiar to TestFlight.
# UCF Familiar (scheme UCFFamiliar, bundle io.river.familiar.ucf). Needs the App
# Store Connect record for that bundle id (the owner's act, once) and an App Store Connect key.
# Xcode must be one App Store Connect accepts (26.x until an Xcode 27 RC exists).
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IOS="$REPO/ios"
BUILD="${1:?usage: ship-ucf.sh <build-number>}"
ASC_KEY_ID="${ASC_KEY_ID:-SUZJSXVS25}"
ASC_ISSUER_ID="${ASC_ISSUER_ID:-69a6de82-89e3-47e3-e053-5b8c7c11a4d1}"
ASC_KEY_PATH="${ASC_KEY_PATH:-$HOME/.appstoreconnect/private_keys/AuthKey_${ASC_KEY_ID}.p8}"
[ -f "$ASC_KEY_PATH" ] || { echo "ASC key not found at $ASC_KEY_PATH"; exit 1; }
cd "$IOS"
python3 - "$BUILD" <<'PY'
import re, sys
n = sys.argv[1]
p = open("project.yml").read()
# One app, one version line — but pin the count so a second one appearing later
# cannot be bumped silently or missed.
new, count = re.subn(r'CURRENT_PROJECT_VERSION: "\d+"', f'CURRENT_PROJECT_VERSION: "{n}"', p)
assert count == 1, f"expected one CURRENT_PROJECT_VERSION in project.yml, found {count}"
open("project.yml", "w").write(new)
PY
xcodegen > /dev/null
echo "✓ UCF Familiar build $BUILD"
cd "$REPO"
git add ios/project.yml
git diff --cached --quiet || git commit -m "UCF Familiar build $BUILD
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
# Rebase before pushing: a rejected push under `set -e` kills the ship between
# claiming the build number in git and building anything, leaving the number
# burned and nothing on TestFlight. Two Macs
# and several sessions land on this repo now; a racing push is the normal case.
git pull --rebase --autostash --quiet origin "$(git branch --show-current)"
git push origin "$(git branch --show-current)" 2>&1 | tail -1
cd "$IOS"
ARCHIVE=/tmp/UCFFamiliar.xcarchive
EXPORT=/tmp/UCFFamiliar-export
rm -rf "$ARCHIVE" "$EXPORT"
xcodebuild -project UCFFamiliar.xcodeproj -scheme UCFFamiliar -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$ARCHIVE" archive \
  -allowProvisioningUpdates -authenticationKeyPath "$ASC_KEY_PATH" \
  -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID"
# App Store export with automatic signing for the new bundle id (its profile is minted on the fly).
cat > /tmp/UCFFamiliar-export.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>app-store-connect</string>
  <key>teamID</key><string>8GHXL328AR</string>
  <!-- Manual signing, learned the hard way: this key cannot do cloud-managed
       distribution ("Cloud signing permission error"), so we pin the Apple Distribution
       cert and the App Store profile created for this bundle through the ASC API. -->
  <key>signingStyle</key><string>manual</string>
  <key>signingCertificate</key><string>Apple Distribution</string>
  <key>provisioningProfiles</key><dict>
    <key>io.river.familiar.ucf</key><string>UCF Familiar AppStore io.river.familiar.ucf</string>
  </dict>
  <key>uploadSymbols</key><true/>
</dict></plist>
PLIST
xcodebuild -exportArchive -archivePath "$ARCHIVE" -exportOptionsPlist /tmp/UCFFamiliar-export.plist -exportPath "$EXPORT" \
  -allowProvisioningUpdates -authenticationKeyPath "$ASC_KEY_PATH" \
  -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID"
IPA=$(ls "$EXPORT"/*.ipa | head -1)
xcrun altool --upload-app --type ios --file "$IPA" --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
# Keep the archive (and its dSYMs) where Xcode Organizer and xcsym look, so a TestFlight
# crash from this build can be symbolicated months later. /tmp is wiped on reboot.
KEEP="$HOME/Library/Developer/Xcode/Archives/$(date +%Y-%m-%d)/UCFFamiliar $BUILD $(date +%H.%M).xcarchive"
mkdir -p "$(dirname "$KEEP")" && cp -R "$ARCHIVE" "$KEEP" && echo "✓ archive kept at $KEEP"
echo "✓ UCF Familiar $BUILD uploaded — TestFlight after processing"

# Direct install to the developer's own devices. Without this the standalone ship's
# computer reached a device ONLY via TestFlight, so it was invisible on the iPad
# while every build of the sibling app walked straight on (reported 2026-09-07:
# "don't see it deployed to iPad"). Everything on Apple's side was correct —
# three VALID builds, in beta testing, iPhone AND iPad in
# UIDeviceFamily — and the app still was not on the device, because nothing ever put
# it there. Discover what is actually paired rather than trusting a hardcoded list,
# and say WHY when an install fails.
# shellcheck disable=SC2016
DISCOVERED=$(xcrun devicectl list devices --json-output /tmp/ucf-devices.json >/dev/null 2>&1 \
  && python3 -c '
import json
try:
    devs = json.load(open("/tmp/ucf-devices.json"))["result"]["devices"]
except Exception:
    raise SystemExit
for d in devs:
    if d.get("connectionProperties", {}).get("transportType") == "sameMachine":
        continue
    print(d.get("hardwareProperties", {}).get("udid", ""))
' 2>/dev/null || true)

# The archive is App Store-signed, and an App Store profile carries no devices — it
# installs nowhere. The direct install needs a SEPARATE development build, with
# the archive kept separately for Apple; this script only ever archived, which is
# the whole reason the standalone app never reached a device on its own.
# Automatic signing with the ASC key mints the development profile as needed.
if ! xcodebuild -project UCFFamiliar.xcodeproj -scheme UCFFamiliar -configuration Release \
  -destination 'generic/platform=iOS' -allowProvisioningUpdates \
  -authenticationKeyPath "$ASC_KEY_PATH" \
  -authenticationKeyID "$ASC_KEY_ID" \
  -authenticationKeyIssuerID "$ASC_ISSUER_ID" \
  -derivedDataPath build/ucf-dev build \
  > /tmp/ucf-dev-build.log 2>&1; then
  echo "⚠ the device build failed (/tmp/ucf-dev-build.log) — TestFlight still has $BUILD"
fi
UCFAPP=$(ls -d "$IOS/build/ucf-dev/Build/Products/Release-iphoneos/"*.app 2>/dev/null | head -1)
if [ -z "$DISCOVERED" ]; then
  echo "⚠ no physical device is paired with this Mac — TestFlight is the only route today."
  echo "  Pair once per device: connect by USB, tap Trust, enter the passcode."
fi
for D in $DISCOVERED; do
  ok=""; why=""
  for try in 1 2 3; do
    if why=$(xcrun devicectl device install app --device "$D" "$UCFAPP" 2>&1); then
      ok=1; echo "✓ $D installed"; break
    fi
    sleep 5
  done
  if [ -z "$ok" ]; then
    echo "⚠ $D not installed — $(printf '%s' "$why" | tail -3 | tr '\n' ' ' | cut -c1-200)"
    echo "  (TestFlight will still cover it)"
  fi
done
