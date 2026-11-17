#!/usr/bin/env bash
# Compile en mode release et installe « Double ou Moitié.app » dans /Applications.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release

APP="/Applications/Double ou Moitié.app"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/double-ou-moitie "$APP/Contents/MacOS/double-ou-moitie"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Double ou Moitié</string>
  <key>CFBundleDisplayName</key><string>Double ou Moitié</string>
  <key>CFBundleIdentifier</key><string>local.double-ou-moitie</string>
  <key>CFBundleExecutable</key><string>double-ou-moitie</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.education</string>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
touch "$APP"
echo "Installé : $APP"
