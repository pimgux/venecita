#!/usr/bin/env bash
# Builds "Venecita Dock.app" and a DMG in dist/.
#
#   scripts/bundle-macos.sh                 # this Mac's architecture
#   UNIVERSAL=1 scripts/bundle-macos.sh     # Intel + Apple Silicon (what releases use)
#
# Signing: ad-hoc by default (fine to run it on your own Mac). With a Developer ID
# certificate in the keychain, set MACOS_SIGN_IDENTITY="Developer ID Application: …" to
# sign it for distribution (hardened runtime).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP_NAME="Venecita Dock"
BUNDLE_ID="io.github.pimgux.venecita-dock"
DIST=dist
APP="$DIST/$APP_NAME.app"

if [[ "${UNIVERSAL:-0}" == "1" ]]; then
    TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
else
    TARGETS=("$(rustc -vV | sed -n 's/^host: //p')")
fi

echo "==> Building $APP_NAME $VERSION for ${TARGETS[*]}"
for target in "${TARGETS[@]}"; do
    rustup target add "$target" >/dev/null
    cargo build --release --target "$target" \
        -p venecita-app --bin venecita-dock -p venecita-cli --bin venecita
done

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
for bin in venecita-dock venecita; do
    inputs=()
    for target in "${TARGETS[@]}"; do inputs+=("target/$target/release/$bin"); done
    lipo -create -output "$APP/Contents/MacOS/$bin" "${inputs[@]}"
done

echo "==> Icon"
ICONSET="$DIST/AppIcon.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
SRC=crates/venecita-app/assets/icons/AppIcon.png
for size in 16 32 128 256 512; do
    sips -z $size $size "$SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$APP_NAME</string>
    <key>CFBundleDisplayName</key><string>$APP_NAME</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleExecutable</key><string>venecita-dock</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <!-- A menu-bar app: no Dock icon. -->
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Venecita Dock contributors · GPL-3.0-or-later</string>
    <!-- Why Venecita needs Accessibility (macOS's own permission prompt doesn't show it). -->
    <key>NSAccessibilityUsageDescription</key>
    <string>Venecita Dock moves and resizes the windows of your apps into the places you chose.</string>
</dict>
</plist>
PLIST

echo "==> Signing"
if [[ -n "${MACOS_SIGN_IDENTITY:-}" ]]; then
    codesign --force --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" \
        "$APP/Contents/MacOS/venecita" "$APP/Contents/MacOS/venecita-dock"
    codesign --force --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" "$APP"
else
    codesign --force --sign - "$APP/Contents/MacOS/venecita" "$APP/Contents/MacOS/venecita-dock"
    codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"

echo "==> DMG"
DMG="$DIST/Venecita-Dock-$VERSION-macos.dmg"
STAGE="$DIST/dmg"
rm -rf "$STAGE" "$DMG" && mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"

echo "==> Done"
echo "    $APP"
echo "    $DMG"
lipo -archs "$APP/Contents/MacOS/venecita-dock"
