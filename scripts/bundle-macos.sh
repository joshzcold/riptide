#!/usr/bin/env bash
# Builds riptide.app around a riptide binary, as CEF expects on macOS:
#   scripts/bundle-macos.sh <riptide binary> <CEF dir> <out dir> [version]
#     -> <out dir>/riptide.app, ad-hoc signed
# The helper apps get riptide-helper from beside the binary.
# The CEF dir holds "Chromium Embedded Framework.framework" (cef-dll-sys's
# download, or CEF_PATH).
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin=$1 cef=$2 out=$3
helper_bin=$(dirname "$bin")/riptide-helper
version=${4:-$(cargo metadata --no-deps --format-version 1 --manifest-path "$root/Cargo.toml" |
    sed -n 's/.*"name":"riptide","version":"\([^"]*\)".*/\1/p')}
# CFBundleVersion takes only numbers and dots, so a nightly's name becomes 0.0.0.
[[ $version =~ ^[0-9]+(\.[0-9]+)*$ ]] && number=$version || number=0.0.0
id=io.github.joshzcold.riptide
framework="Chromium Embedded Framework.framework"

[[ -f $bin ]] || { echo "bundle-macos.sh: no binary at $bin" >&2; exit 1; }
[[ -f $helper_bin ]] || { echo "bundle-macos.sh: no riptide-helper beside $bin" >&2; exit 1; }
[[ -d $cef/$framework ]] || { echo "bundle-macos.sh: no $framework in $cef" >&2; exit 1; }

app=$out/riptide.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/riptide"
cp -R "$cef/$framework" "$app/Contents/Frameworks/"
cp "$root/packaging/macos/riptide.icns" "$app/Contents/Resources/"
# For a `riptide` command: link to this, not to the binary.
mkdir -p "$app/Contents/Resources/bin"
cp "$root/packaging/macos/riptide-cli" "$app/Contents/Resources/bin/riptide"

# <name> <executable> <identifier> <extra plist keys>
plist() {
    cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$1</string>
  <key>CFBundleDisplayName</key><string>$1</string>
  <key>CFBundleExecutable</key><string>$2</string>
  <key>CFBundleIdentifier</key><string>$3</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleShortVersionString</key><string>$number</string>
  <key>CFBundleVersion</key><string>$number</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>LSEnvironment</key><dict><key>MallocNanoZone</key><string>0</string></dict>
$4
</dict>
</plist>
EOF
}

plist riptide riptide "$id" "$(cat <<'EOF'
  <key>CFBundleIconFile</key><string>riptide.icns</string>
  <key>NSCameraUsageDescription</key><string>Sites you allow can use the camera.</string>
  <key>NSMicrophoneUsageDescription</key><string>Sites you allow can use the microphone.</string>
  <key>NSBluetoothAlwaysUsageDescription</key><string>Sites you allow can use Bluetooth devices.</string>
  <key>NSWebBrowserPublicKeyCredentialUsageDescription</key><string>Sites can ask you to sign in with a passkey.</string>
  <key>CFBundleURLTypes</key>
  <array><dict>
    <key>CFBundleURLName</key><string>Web address</string>
    <key>CFBundleURLSchemes</key><array><string>http</string><string>https</string></array>
  </dict></array>
  <key>CFBundleDocumentTypes</key>
  <array><dict>
    <key>CFBundleTypeName</key><string>HTML document</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSItemContentTypes</key><array><string>public.html</string></array>
  </dict></array>
EOF
)" >"$app/Contents/Info.plist"

# Chromium starts each kind of subprocess from its own helper app. They run
# riptide-helper, which leaves out the browser's code.
for kind in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
    name="riptide Helper$kind"
    helper="$app/Contents/Frameworks/$name.app"
    mkdir -p "$helper/Contents/MacOS"
    cp "$helper_bin" "$helper/Contents/MacOS/$name"
    suffix=$(tr -d ' ()' <<<"$kind" | tr '[:upper:]' '[:lower:]')
    plist "$name" "$name" "$id.helper${suffix:+.$suffix}" \
        "  <key>LSUIElement</key><string>1</string>" >"$helper/Contents/Info.plist"
done

# Apple silicon runs only signed code; an ad-hoc signature needs no certificate.
# Inside out: the framework and helpers before the app that contains them.
codesign --force --sign - "$app/Contents/Frameworks/$framework"
for helper in "$app/Contents/Frameworks/"*.app; do
    codesign --force --sign - "$helper"
done
codesign --force --sign - "$app"
echo "$app"
