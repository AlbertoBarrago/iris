#!/usr/bin/env bash
# Builds Iris for Fedora and packs it into an rpm that installs
# under /usr. Run it on the oldest Fedora the rpm should install on: rpm
# reads the libraries the binaries link against and requires each one, so
# they must be Fedora's own.
#
#   scripts/package-rpm.sh <version> <out>
#   RPM_SIGN_KEY=<fingerprint> ...   also signs the rpm with that key,
#                                    which must be in gpg's keyring
#
# The build turns on the packaging-rpm feature, so the app leaves updates
# to dnf instead of offering to install a .deb. Besides the staged tree,
# the rpm carries the repository's public key and a .repo file, so a
# person who installs it once gets later versions from `dnf upgrade`, as
# the .deb does with apt.
set -euo pipefail
# Packages must not inherit a group-writable umask from whoever builds them.
umask 022

cd "$(dirname "$0")/.."
version=${1:?usage: scripts/package-rpm.sh <version> <out>}
out=${2:?usage: scripts/package-rpm.sh <version> <out>}
mkdir -p "$out"
out=$(realpath "$out")
here=$(pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cargo build --release --locked -p mailrs -p mailrs-cli --features mailrs/packaging-rpm
tree="$work/tree"
scripts/stage.sh "$tree"

mkdir -p "$work/SPECS" "$work/RPMS" "$work/BUILD"
cat > "$work/SPECS/iris.spec" <<SPEC
# The binaries arrive built, so there is nothing to compile and no
# debug information to split out.
%global debug_package %{nil}
%global _build_id_links none

Name:           iris
Version:        $version
Release:        1
Summary:        Mail and calendar for Linux
License:        GPL-3.0-or-later
URL:            https://github.com/AlbertoBarrago/iris
ExclusiveArch:  x86_64

Requires:       gtk4 >= 4.20
Requires:       libadwaita >= 1.8
Requires:       webkitgtk6.0
Requires:       gnupg2
Recommends:     gnupg2-smime
Recommends:     bubblewrap
Recommends:     gnome-shell-extension-appindicator

%description
Reads, sorts and sends mail for several Gmail accounts, keeps them in
sync from the system tray, and signs and encrypts with OpenPGP or S/MIME.

%install
mkdir -p %{buildroot}/usr
cp -r $tree/. %{buildroot}/usr/
install -Dm644 $here/packaging/apt/iris-archive-keyring.asc \\
    %{buildroot}/etc/pki/rpm-gpg/RPM-GPG-KEY-iris
install -Dm644 $here/packaging/rpm/iris.repo \\
    %{buildroot}/etc/yum.repos.d/iris.repo
install -Dm644 $here/LICENSE %{buildroot}/usr/share/licenses/iris/LICENSE

%files
%license /usr/share/licenses/iris/LICENSE
/usr/bin/iris
/usr/bin/iris-cli
/usr/share/applications/io.github.AlbertoBarrago.Iris.desktop
/usr/share/metainfo/io.github.AlbertoBarrago.Iris.metainfo.xml
/usr/share/icons/hicolor/scalable/apps/io.github.AlbertoBarrago.Iris.svg
/usr/share/icons/hicolor/symbolic/apps/io.github.AlbertoBarrago.Iris-symbolic.svg
/usr/share/icons/hicolor/16x16/apps/io.github.AlbertoBarrago.Iris.svg
/usr/share/locale/*/LC_MESSAGES/iris.mo
/etc/pki/rpm-gpg/RPM-GPG-KEY-iris
# dnf keeps a .repo file the person edited or removed, as dpkg keeps a
# conffile.
%config(noreplace) /etc/yum.repos.d/iris.repo
SPEC

rpmbuild --quiet -bb \
    --define "_topdir $work" \
    --define "_rpmdir $work/RPMS" \
    "$work/SPECS/iris.spec"
rpm="iris-$version-1.x86_64.rpm"
mv "$work/RPMS/x86_64/$rpm" "$out/$rpm"

if [ -n "${RPM_SIGN_KEY:-}" ]; then
    rpmsign --addsign \
        --define "_gpg_name $RPM_SIGN_KEY" \
        --define "_gpg_sign_cmd_extra_args --pinentry-mode loopback --passphrase ''" \
        "$out/$rpm" >/dev/null
fi

echo "Packed $out/$rpm"
