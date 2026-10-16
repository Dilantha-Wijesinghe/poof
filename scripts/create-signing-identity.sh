#!/usr/bin/env bash
# Creates Poof's self-signed code-signing identity (run once per project).
#
# Releases signed with the same certificate keep the same code identity, so
# macOS remembers permissions such as Full Disk Access across updates. (Ad-hoc
# signatures change with every build, which makes macOS forget them.)
#
# Output, in ~/.poof-release (keep it private and backed up):
#   codesign.p12            certificate + private key
#   codesign-password.txt   password protecting the .p12
# Sign a local build with it:
#   POOF_CODESIGN_P12=~/.poof-release/codesign.p12 \
#   POOF_CODESIGN_PASSWORD_FILE=~/.poof-release/codesign-password.txt scripts/bundle.sh
# Then add these GitHub secrets:
#   POOF_CODESIGN_P12            = base64 of codesign.p12  (see the command printed below)
#   POOF_CODESIGN_P12_PASSWORD   = contents of codesign-password.txt
set -euo pipefail

NAME="${POOF_CODESIGN_NAME:-Poof Release Signing}"
DIR="$HOME/.poof-release"
P12="$DIR/codesign.p12"
[[ -e "$P12" ]] && { echo "$P12 already exists; refusing to overwrite it" >&2; exit 1; }

mkdir -p "$DIR" && chmod 700 "$DIR"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cat > "$work/cert.cnf" <<CNF
[req]
distinguished_name = dn
prompt = no
x509_extensions = ext
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF

openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes \
  -config "$work/cert.cnf" -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
password="$(openssl rand -base64 24)"
# Legacy PBE algorithms keep the file importable by macOS `security`.
openssl pkcs12 -export -legacy -name "$NAME" -inkey "$work/key.pem" -in "$work/cert.pem" \
  -passout "pass:$password" -out "$P12"
(umask 077 && printf '%s' "$password" > "$DIR/codesign-password.txt")
chmod 600 "$P12"

echo "Created $P12 (valid for 10 years)."
echo "Add the GitHub secrets with:"
echo "  base64 -i \"$P12\" | gh secret set POOF_CODESIGN_P12"
echo "  gh secret set POOF_CODESIGN_P12_PASSWORD < \"$DIR/codesign-password.txt\""
