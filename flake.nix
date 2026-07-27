{
  # Nix flake for the `forgedb` CLI — epic #181 / Nix channel.
  #
  #   nix run   github:hoodiecollin/forgedb -- --help      # run without installing
  #   nix profile install github:hoodiecollin/forgedb      # install to your profile
  #   nix build github:hoodiecollin/forgedb                 # -> ./result/bin/forgedb
  #   nix develop                                           # dev shell (rust + tooling)
  #
  # Builds from source over the committed Cargo.lock, with the `lsp` feature on so
  # the bundled `forgedb-lsp` ships alongside `forgedb` — matching every other
  # channel. No git dependencies, so `cargoLock.lockFile` needs no output hashes.
  description = "ForgeDB — an application database generator (CLI + bundled language server)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      # Tier-1 desktop/CI targets. Mirrors the release.yml host matrix.
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # Read name/version straight from Cargo.toml so this never drifts from the crate.
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

      mkForgedb = pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # Ship the language server too (bundled feature-gated bin).
          buildFeatures = [ "lsp" ];

          # `ring` (transitive, via auth/JWT) needs a C toolchain — provided by
          # stdenv — but no system libraries, so nativeBuildInputs stays minimal.
          nativeBuildInputs = [ pkgs.pkg-config ];

          # The workspace's heavier integration tests hit the filesystem / spawn the
          # binary; unit + doctests still run. Flip to `false` to build faster.
          doCheck = true;

          meta = with pkgs.lib; {
            description = cargoToml.package.description;
            homepage = "https://forgedb.dev";
            license = with licenses; [ mit asl20 ];
            mainProgram = "forgedb";
            platforms = systems;
          };
        };
    in
    {
      packages = forAllSystems (pkgs: rec {
        forgedb = mkForgedb pkgs;
        default = forgedb;
      });

      apps = forAllSystems (pkgs: rec {
        forgedb = {
          type = "app";
          program = "${self.packages.${pkgs.system}.forgedb}/bin/forgedb";
        };
        default = forgedb;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.system}.forgedb ];
          packages = [ pkgs.rustc pkgs.cargo pkgs.clippy pkgs.rustfmt pkgs.rust-analyzer ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixpkgs-fmt);
    };
}
