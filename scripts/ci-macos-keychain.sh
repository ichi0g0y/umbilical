#!/usr/bin/env bash
# CI only: import the self-signed code signing certificate for macOS builds.
#
# macOS keeps permissions (Accessibility, Screen Recording, ...) per app. With an
# ad-hoc signature the app is known by its hash, so every update loses them. With
# the same certificate every time, the app stays the same app for macOS.
#
# Needs MACOS_CERTIFICATE (base64 .p12) and MACOS_CERTIFICATE_PASSWORD.
# Without them the build keeps the ad-hoc signature.
set -euo pipefail

if [ -z "${MACOS_CERTIFICATE:-}" ]; then
  echo "No MACOS_CERTIFICATE: using the ad-hoc signature."
  exit 0
fi

kc="$RUNNER_TEMP/umbilical-sign.keychain-db"
kc_pw="$(openssl rand -hex 16)"
p12="$RUNNER_TEMP/umbilical-sign.p12"

echo "$MACOS_CERTIFICATE" | base64 --decode > "$p12"
security create-keychain -p "$kc_pw" "$kc"
security set-keychain-settings -t 3600 -u "$kc"
security unlock-keychain -p "$kc_pw" "$kc"
security import "$p12" -k "$kc" -P "$MACOS_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$kc_pw" "$kc" > /dev/null
# shellcheck disable=SC2046
security list-keychains -d user -s "$kc" $(security list-keychains -d user | xargs)
rm -f "$p12"

# A self-signed certificate is "not trusted", so codesign finds it only by its hash.
identity="$(security find-identity -p codesigning "$kc" | grep -m1 -oE '[0-9A-F]{40}')"
echo "Signing identity: $identity"
echo "APPLE_SIGNING_IDENTITY=$identity" >> "$GITHUB_ENV"
