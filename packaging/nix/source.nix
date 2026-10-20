# Riptide built from source, against nixpkgs' cef-binary at the CEF build the
# cef crate in Cargo.lock pins. The flake builds its own checkout with it
# (`nix build github:joshzcold/riptide#riptide-source`).
{
  lib,
  stdenv,
  rustPlatform,
  cef-binary,
  runCommand,
  cmake,
  git,
  ninja,
  makeWrapper,
  gtk3,
  libGL,
  vulkan-loader,
  src ? lib.cleanSource ../..,
  # The commit --version shows; the source has no .git to read it from.
  commit ? "unknown",
}:

let
  lock = builtins.fromTOML (builtins.readFile (src + "/Cargo.lock"));
  cefCrate = lib.findFirst (p: p.name == "cef") null lock.package;

  # The CEF build for the pinned cef crate (154.4.0+154.0.33 is CEF 154.0.33).
  # The weekly cef-update workflow updates these with Cargo.lock.
  cefVersion = "154.0.33";
  cefGitRevision = "a03e714";
  cefChromiumVersion = "154.0.8037.94";
  cefSha1 = "9794ecf85ccd4dfcca42bfaac7a7666004f051e8";
  cef =
    (cef-binary.override {
      version = cefVersion;
      gitRevision = cefGitRevision;
      chromiumVersion = cefChromiumVersion;
      srcHashes.x86_64-linux = "sha256-zYnjZgVdRBKUL6JTNKn8mKW2n/Wwq/YgnJTi2eOABaI=";
    }).overrideAttrs
      (old: {
        # CEF 154 no longer ships libEGL.so and libGLESv2.so, which nixpkgs'
        # cef-binary (151) patches.
        installPhase = lib.concatStringsSep "\n" (
          lib.filter (line: !(lib.hasInfix "libEGL.so" line || lib.hasInfix "libGLESv2.so" line)) (
            lib.splitString "\n" old.installPhase
          )
        );
      });

  # The layout the cef-dll-sys build script expects in CEF_PATH.
  cefPath = runCommand "riptide-cef-path" { } ''
    mkdir $out
    ln -s ${cef}/Release/* ${cef}/Resources/* $out/
    ln -s ${cef}/{CMakeLists.txt,cmake,include,libcef_dll,CREDITS.html} $out/
    echo '{"type":"minimal","name":"cef_binary_${cefVersion}+g${cefGitRevision}+chromium-${cefChromiumVersion}_linux64_minimal.tar.bz2","sha1":"${cefSha1}"}' > $out/archive.json
  '';

  # Chromium's runtime files, which the build copies next to the binary.
  runtimeFiles = [
    "libcef.so"
    "libvk_swiftshader.so"
    "libvulkan.so.1"
    "vk_swiftshader_icd.json"
    "icudtl.dat"
    "v8_context_snapshot.bin"
    "resources.pak"
    "chrome_100_percent.pak"
    "chrome_200_percent.pak"
    "CREDITS.html"
    "locales"
  ];
in
assert lib.assertMsg
  (cefCrate != null && lib.last (lib.splitString "+" cefCrate.version) == cefVersion)
  "packaging/nix/source.nix pins CEF ${cefVersion}, but Cargo.lock's cef crate is ${cefCrate.version or "missing"}";

rustPlatform.buildRustPackage {
  pname = "riptide";
  version = (builtins.fromTOML (builtins.readFile (src + "/Cargo.toml"))).workspace.package.version;
  inherit src;

  cargoLock.lockFile = src + "/Cargo.lock";
  cargoBuildFlags = [
    "--bin"
    "riptide"
  ];

  nativeBuildInputs = [
    cmake
    ninja
    makeWrapper
  ];
  # cmake and ninja build CEF's C++ wrapper from the cef-dll-sys build script, not here.
  dontUseCmakeConfigure = true;
  dontUseNinjaBuild = true;
  dontUseNinjaInstall = true;
  dontUseNinjaCheck = true;

  env = {
    CEF_PATH = cefPath;
    RT_GIT_COMMIT = commit;
  };

  # The plugin tests install from local git repositories.
  nativeCheckInputs = [ git ];
  # The unit tests of the CEF-free crates; the rest need a display.
  cargoTestFlags = [
    "-p"
    "rt-core"
    "-p"
    "rt-config"
    "-p"
    "rt-storage"
  ];

  # Chromium opens these at run time rather than linking them.
  runtimeLibraries = lib.makeLibraryPath [
    gtk3
    libGL
    vulkan-loader
  ];

  installPhase = ''
    runHook preInstall
    release=target/${stdenv.hostPlatform.rust.rustcTarget}/release
    mkdir -p $out/lib/riptide $out/bin
    cp "$release/riptide" $out/lib/riptide/
    for f in ${lib.escapeShellArgs runtimeFiles}; do
      cp -rL "$release/$f" $out/lib/riptide/
    done
    chmod -R u+w $out/lib/riptide
    # The Nix store can't hold a setuid sandbox helper, so there is none; the
    # sandbox uses user namespaces instead, which NixOS allows by default.
    makeWrapper $out/lib/riptide/riptide $out/bin/riptide \
      --prefix LD_LIBRARY_PATH : "$runtimeLibraries"
    install -Dm644 packaging/riptide.desktop $out/share/applications/riptide.desktop
    install -Dm644 packaging/riptide.svg $out/share/icons/hicolor/scalable/apps/riptide.svg
    runHook postInstall
  '';

  meta = {
    description = "Keyboard-driven web browser with vim-like bindings, built on CEF";
    homepage = "https://joshzcold.github.io/riptide/";
    license = lib.licenses.gpl3Plus;
    mainProgram = "riptide";
    platforms = [ "x86_64-linux" ];
  };
}
