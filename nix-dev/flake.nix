{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-25.05";
    systems.url = "github:nix-systems/default";
    devenv.url = "github:cachix/devenv";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs = {
    self,
    nixpkgs,
    devenv,
    systems,
    ...
  } @ inputs: let
    forEachSystem = nixpkgs.lib.genAttrs (import systems);
  in rec {
    packages = forEachSystem (system: let
      pkgs = import nixpkgs {inherit system;};
    in {
      devenv-up = self.devShells.${system}.default.config.procfileScript;
      ais-uploader = pkgs.callPackage package-defs.ais-uploader {};
    });

    devShells =
      forEachSystem
      (system: let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        default = devenv.lib.mkShell {
          inherit inputs pkgs;
          modules = [
            {
              languages.rust = {
                enable = true;
                channel = "stable";
                components = ["rustc" "cargo" "clippy" "rust-analyzer" "rust-src" "rustfmt"];
              };
              # https://devenv.sh/reference/options/
              packages = [pkgs.mold pkgs.cargo-expand pkgs.cargo-nextest pkgs.openssl];

              # enterShell = ''
              #   hello
              # '';
            }
          ];
        };
      });

    package-defs = {
      ais-uploader = ./nix/ais-uploader.nix;
    };
  };
}
