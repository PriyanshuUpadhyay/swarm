#!/bin/zsh
# Creates the self-signed code-signing identity `swarm-local` in the login keychain, once per Mac.
# build.sh signs the apps with it, so their designated requirement names this certificate, not
# the binary's cdhash, and macOS keeps privacy answers across updates.
set -euo pipefail

name=swarm-local
if security find-certificate -c "$name" >/dev/null 2>&1; then
  print "$name already exists"
  exit 0
fi
work="$(mktemp -d)"
trap 'rm -f "$work"/key.pem "$work"/cert.pem "$work"/id.p12; rmdir "$work"' EXIT
pass="$(openssl rand -hex 16)"
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=$name" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -keyout "$work/key.pem" -out "$work/cert.pem" 2>/dev/null
# The Keychain's PKCS#12 reader needs the legacy ciphers.
openssl pkcs12 -export -legacy -inkey "$work/key.pem" -in "$work/cert.pem" -name "$name" \
  -passout "pass:$pass" -out "$work/id.p12"
security import "$work/id.p12" -k "$HOME/Library/Keychains/login.keychain-db" -P "$pass" -T /usr/bin/codesign
print "created $name"
