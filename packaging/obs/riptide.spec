# The openSUSE Build Service package (home:joshzcold/riptide): it repackages a
# release's Linux tarball. scripts/update-packages.sh sets the version.
%global debug_package %{nil}
%global __os_install_post %{nil}
%global _build_id_links none
%global __provides_exclude_from ^/opt/riptide/.*$
%global __requires_exclude ^(libcef|libvk_swiftshader|libvulkan)\\.so.*$

Name:           riptide
Version:        0.22.0
Release:        0
Summary:        Keyboard-driven web browser with vim-like bindings
License:        GPL-3.0-or-later
URL:            https://joshzcold.github.io/riptide/
Source0:        https://github.com/joshzcold/riptide/releases/download/v%{version}/riptide-%{version}-linux-x86_64.tar.gz
Source1:        https://github.com/joshzcold/riptide/releases/download/v%{version}/riptide-%{version}-linux-aarch64.tar.gz
ExclusiveArch:  x86_64 aarch64
Requires:       xdg-utils
# Chromium aborts without a font; minimal installs of openSUSE have none.
Requires:       (dejavu-fonts or font(dejavusans))
%if 0%{?suse_version}
# Owns the icon directory; openSUSE's build fails on unowned directories.
BuildRequires:  hicolor-icon-theme
%endif

%description
Riptide is a keyboard-driven browser built on the Chromium Embedded
Framework (CEF) and controlled from Rust: modes, hints, a command
line, and configuration in TOML or Lua. Its license has an additional
permission for loading a content decryption module such as Widevine
(/opt/riptide/LICENSE-EXCEPTION); Chromium's and CEF's licenses are in
/opt/riptide/CREDITS.html.

%prep
%setup -q -T -c
tar -xzf %{_sourcedir}/riptide-%{version}-linux-%{_arch}.tar.gz --strip-components=1

%build

%install
mkdir -p %{buildroot}/opt/riptide %{buildroot}%{_bindir}
cp -a . %{buildroot}/opt/riptide/
ln -s /opt/riptide/riptide %{buildroot}%{_bindir}/riptide
install -Dm644 riptide.desktop %{buildroot}%{_datadir}/applications/riptide.desktop
install -Dm644 riptide.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/riptide.svg
chmod -R go-w %{buildroot}
# openSUSE only allows setuid files listed in its permissions packages, so
# there the sandbox relies on user namespaces, which openSUSE allows.
%if 0%{?suse_version}
chmod 0755 %{buildroot}/opt/riptide/chrome-sandbox
%else
chmod 4755 %{buildroot}/opt/riptide/chrome-sandbox
%endif

%files
%defattr(-,root,root,-)
/opt/riptide
%{_bindir}/riptide
%{_datadir}/applications/riptide.desktop
%{_datadir}/icons/hicolor/scalable/apps/riptide.svg

%changelog
