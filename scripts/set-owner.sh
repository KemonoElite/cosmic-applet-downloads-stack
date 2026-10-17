#!/bin/sh
# Replaces the "yourname" placeholders with your GitHub username.
# Usage: scripts/set-owner.sh <github-username> [repo-name]
set -eu

user="${1:?usage: $0 <github-username> [repo-name]}"
repo="${2:-cosmic-applet-downloads-stack}"

# Flatpak IDs can't contain hyphens in the middle elements, and are lowercase by convention.
id_user=$(printf '%s' "$user" | tr 'A-Z' 'a-z' | tr '-' '_')
old_id="io.github.yourname.DownloadsStack"
new_id="io.github.${id_user}.DownloadsStack"

cd "$(dirname "$0")/.."

for f in "resources/$old_id.desktop" "resources/$old_id.svg" "resources/$old_id.metainfo.xml" "$old_id.json"; do
    mv "$f" "$(printf '%s' "$f" | sed "s/$old_id/$new_id/")"
done

sed -i \
    -e "s/io\.github\.yourname\.DownloadsStack/$new_id/g" \
    -e "s#github.com/yourname/cosmic-applet-downloads-stack#github.com/$user/$repo#g" \
    -e "s#<name>yourname</name>#<name>$user</name>#" \
    -e "s/io\.github\.yourname/io.github.$id_user/g" \
    src/app.rs justfile README.md "$new_id.json" \
    resources/"$new_id".desktop resources/"$new_id".metainfo.xml

echo "App ID is now $new_id"
