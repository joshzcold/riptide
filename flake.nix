{
  description = "Riptide: a keyboard-driven web browser with vim-like bindings, built on CEF";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  # The latest release's Linux tarball, patched for NixOS (packaging/nix/package.nix).
  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system} = rec {
        riptide = pkgs.callPackage ./packaging/nix/package.nix { };
        # Built from this checkout (packaging/nix/source.nix).
        riptide-source = pkgs.callPackage ./packaging/nix/source.nix {
          src = self;
          commit = builtins.substring 0 10 (self.rev or self.dirtyRev or "unknown");
        };
        default = riptide;
      };
      apps.${system}.default = {
        type = "app";
        program = "${self.packages.${system}.riptide}/bin/riptide";
      };
    };
}
