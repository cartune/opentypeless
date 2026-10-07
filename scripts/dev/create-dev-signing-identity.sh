#!/usr/bin/env bash
# Creates a local self-signed code-signing identity named "OpenTypeless Dev"
# in a dedicated keychain, so repeated local builds keep the same signature
# and macOS keeps the Accessibility / Microphone grants across rebuilds.
#
# Run once, by hand:   bash scripts/dev/create-dev-signing-identity.sh
# Nothing here needs sudo. A macOS dialog may appear once asking to trust the
# certificate for code signing; click "Always Allow"/"Add".
set -euo pipefail

NAME="OpenTypeless Dev"
KC="$HOME/Library/Keychains/opentypeless-dev.keychain-db"
KC_PASS="opentypeless-dev"

if security find-identity -v -p codesigning 2>/dev/null | grep -q "$NAME"; then
  echo "Identity '$NAME' already exists:"
  security find-identity -v -p codesigning | grep "$NAME"
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

cat > cert.cnf <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
subjectKeyIdentifier = hash
CNF

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config cert.cnf \
  -keyout key.pem -out cert.pem >/dev/null 2>&1
# -legacy is needed on OpenSSL 3 for a PKCS#12 that macOS `security import` accepts.
openssl pkcs12 -export -inkey key.pem -in cert.pem -name "$NAME" \
  -out dev.p12 -passout pass:p12pass -legacy 2>/dev/null \
  || openssl pkcs12 -export -inkey key.pem -in cert.pem -name "$NAME" \
       -out dev.p12 -passout pass:p12pass

if [ ! -f "$KC" ]; then
  security create-keychain -p "$KC_PASS" "$KC"
fi
security set-keychain-settings -lut 21600 "$KC"   # stays unlocked 6h after use
security unlock-keychain -p "$KC_PASS" "$KC"
security import dev.p12 -k "$KC" -P p12pass -T /usr/bin/codesign -T /usr/bin/security >/dev/null
# Let codesign use the key without a password prompt.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KC_PASS" "$KC" >/dev/null

# Add the keychain to the user search list, keeping the existing ones.
existing=$(security list-keychains -d user | tr -d '" ')
# shellcheck disable=SC2086
security list-keychains -d user -s "$KC" $existing

# Trust the self-signed cert for code signing (user trust domain; may show one dialog).
security add-trusted-cert -r trustRoot -p codeSign -k "$KC" cert.pem || true

echo
echo "Done. Identity:"
security find-identity -v -p codesigning | grep "$NAME" || {
  echo "Identity not listed as valid yet. If a trust dialog appeared, accept it and re-run this script."
  exit 1
}
