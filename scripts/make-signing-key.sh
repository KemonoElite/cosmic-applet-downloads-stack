#!/bin/sh
# Optional: creates a throwaway GPG key for signing your Flatpak repo.
# Paste the printed private key into a GitHub repo secret named FLATPAK_GPG_PRIVATE_KEY
# (Settings -> Secrets and variables -> Actions). Keep it: if you lose it and make a new
# key, everyone has to remove and re-add the remote.
set -eu
GNUPGHOME=$(mktemp -d)
export GNUPGHOME
gpg --batch --passphrase '' --quick-gen-key "Downloads Stack Flatpak repo" rsa4096 sign never
echo
echo "----- copy everything between the lines into the FLATPAK_GPG_PRIVATE_KEY secret -----"
gpg --armor --export-secret-keys
echo "----- end -----"
rm -rf "$GNUPGHOME"
