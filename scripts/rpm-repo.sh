#!/usr/bin/env bash
# Builds the signed dnf repository Iris publishes on GitHub Pages,
# under /rpm beside the apt repository.
#   scripts/rpm-repo.sh <rpms> <out>
# <rpms>: a folder of iris-*.rpm files, every version the
#         repository should offer. The release workflow signs each one.
# <out>:  the folder to publish. It gets the packages, repodata/ with a
#         signed repomd.xml, the public key and a .repo file.
# Signs with the key in $GNUPGHOME, which must hold only the repository's
# signing key; the workflow imports it there from a secret.
set -euo pipefail

rpms=$1
out=$2
here="$(cd "$(dirname "$0")/.." && pwd)"

shopt -s nullglob
packages=("$rpms"/iris-*.x86_64.rpm)
if [ ${#packages[@]} -eq 0 ]; then
    echo "rpm-repo.sh: no iris-*.x86_64.rpm in $rpms" >&2
    exit 1
fi

rm -rf "$out"
mkdir -p "$out/packages"
cp "${packages[@]}" "$out/packages/"
createrepo_c --quiet "$out"

# dnf checks this signature before it trusts anything repomd.xml lists,
# which is what repo_gpgcheck=1 in the .repo file asks for.
gpg --batch --yes --pinentry-mode loopback --passphrase '' \
    --armor --detach-sign -o "$out/repodata/repomd.xml.asc" "$out/repodata/repomd.xml"

cp "$here/packaging/apt/iris-archive-keyring.asc" "$out/RPM-GPG-KEY-iris"
# The rpm's own .repo file names the key it installs under /etc. Someone
# adding the repository by hand has no such file yet, so theirs fetches
# the key from here.
sed 's|^gpgkey=.*|gpgkey=https://albertobarrago.github.io/iris/rpm/RPM-GPG-KEY-iris|' \
    "$here/packaging/rpm/iris.repo" > "$out/iris.repo"
