# Riptide from its Linux release tarball (the binary and the CEF runtime),
# patched to run on NixOS. Build a local tarball with:
#   ./task package
#   nix-build -E 'with import <nixpkgs> {}; callPackage ./packaging/nix/package.nix {
#     src = ./dist/riptide-0.1.0-linux-x86_64.tar.gz; version = "0.1.0"; }'
# Without `src`, it fetches the release for `version` from GitHub.
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  alsa-lib,
  at-spi2-atk,
  at-spi2-core,
  atk,
  cairo,
  cups,
  dbus,
  expat,
  glib,
  gtk3,
  libdrm,
  libGL,
  libxkbcommon,
  mesa,
  nspr,
  nss,
  pango,
  systemd,
  vulkan-loader,
  xorg,
  version ? "0.1.0",
  src ? fetchurl {
    url = "https://github.com/joshzcold/riptide/releases/download/v${version}/riptide-${version}-linux-x86_64.tar.gz";
    # Filled in at each release from the tarball's sha256 (see docs: Releasing).
    hash = lib.fakeHash;
  },
}:

stdenv.mkDerivation {
  pname = "riptide";
  inherit version src;

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
  ];

  # What libcef.so links against.
  buildInputs = [
    alsa-lib
    at-spi2-atk
    at-spi2-core
    atk
    cairo
    cups
    dbus
    expat
    glib
    libdrm
    libxkbcommon
    mesa
    nspr
    nss
    pango
    systemd
    xorg.libX11
    xorg.libXcomposite
    xorg.libXdamage
    xorg.libXext
    xorg.libXfixes
    xorg.libXrandr
    xorg.libxcb
  ];

  # Chromium opens these at run time rather than linking them.
  runtimeLibraries = lib.makeLibraryPath [
    gtk3
    libGL
    vulkan-loader
  ];

  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/lib/riptide $out/bin
    cp -r . $out/lib/riptide
    # The Nix store can't hold a setuid helper; the sandbox uses user
    # namespaces instead, which NixOS allows by default.
    rm -f $out/lib/riptide/chrome-sandbox
    makeWrapper $out/lib/riptide/riptide $out/bin/riptide \
      --prefix LD_LIBRARY_PATH : "$runtimeLibraries"
    if [ -f riptide.desktop ]; then
      install -Dm644 riptide.desktop $out/share/applications/riptide.desktop
      install -Dm644 riptide.svg $out/share/icons/hicolor/scalable/apps/riptide.svg
    fi
    runHook postInstall
  '';

  meta = {
    description = "Keyboard-driven web browser with vim-like bindings, built on CEF";
    homepage = "https://joshzcold.github.io/riptide/";
    license = lib.licenses.gpl3Plus;
    mainProgram = "riptide";
    platforms = [ "x86_64-linux" ];
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
}
