#!/bin/bash
set -euo pipefail

# Builds a local, ad-hoc signed VelaApp.app. macOS TCC only reads the microphone
# and Speech usage descriptions from a real bundle's Info.plist, so push-to-talk
# requires this launch path; a bare `swift run` executable disables it instead.

usage() {
    cat <<'USAGE'
Usage: scripts/run-app.sh [--attached] [--bundle-only]

  (default)      build the bundle and launch it detached via LaunchServices,
                 so macOS attributes privacy prompts to Vela itself
  --attached     exec the bundled binary in this terminal, keeping app and Core
                 logs on stdout/stderr (privacy prompts are attributed to the
                 terminal that owns this shell)
  --bundle-only  build and sign the bundle without launching it
USAGE
}

mode="detached"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --attached) mode="attached" ;;
        --bundle-only) mode="bundle-only" ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
    shift
done

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
app_package="$repo_root/app"
bundle="$app_package/.build/VelaApp.app"

cargo build --manifest-path "$repo_root/core/Cargo.toml" --workspace
swift build --package-path "$app_package"

rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS"
cp "$app_package/.build/debug/VelaApp" "$bundle/Contents/MacOS/VelaApp"
cp "$repo_root/core/target/debug/vela-core" "$bundle/Contents/MacOS/vela-core"
cp "$app_package/Sources/VelaApp/Info.plist" "$bundle/Contents/Info.plist"

# Nested executables must carry their own signature before the bundle is sealed.
codesign --force --sign - "$bundle/Contents/MacOS/vela-core"
codesign --force --sign - --identifier dev.vela.app "$bundle"
codesign --verify --strict "$bundle"

# Launching without these keys makes TCC abort the process, so fail here instead.
for key in NSMicrophoneUsageDescription NSSpeechRecognitionUsageDescription; do
    if ! /usr/libexec/PlistBuddy -c "Print :$key" "$bundle/Contents/Info.plist" >/dev/null 2>&1; then
        echo "error: $key missing from $bundle/Contents/Info.plist" >&2
        exit 1
    fi
done

echo "Bundle ready: $bundle"

case "$mode" in
    bundle-only) ;;
    attached)
        exec "$bundle/Contents/MacOS/VelaApp"
        ;;
    detached)
        open -n "$bundle"
        echo "Launched detached, so macOS attributes privacy prompts to Vela."
        echo "App and Core logs are discarded in this mode; use --attached to keep them."
        ;;
esac
