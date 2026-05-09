{
  description = "RavenFabric — Security-first distributed execution engine";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        version = "0.1.4";
      in
      {
        packages = {
          rf-agent = pkgs.rustPlatform.buildRustPackage {
            pname = "rf-agent";
            inherit version;
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "-p" "rf-agent" ];
            meta = with pkgs.lib; {
              description = "RavenFabric agent — secure remote execution daemon";
              homepage = "https://ravenfabric.io";
              license = licenses.agpl3Plus;
              mainProgram = "rf-agent";
            };
          };

          rf-relay = pkgs.rustPlatform.buildRustPackage {
            pname = "rf-relay";
            inherit version;
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "-p" "rf-relay" ];
            meta = with pkgs.lib; {
              description = "RavenFabric relay — stateless encrypted broker";
              homepage = "https://ravenfabric.io";
              license = licenses.agpl3Plus;
              mainProgram = "rf-relay";
            };
          };

          rf-cli = pkgs.rustPlatform.buildRustPackage {
            pname = "rf-cli";
            inherit version;
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "-p" "rf-cli" ];

            postInstall = ''
              installShellCompletion --cmd rf \
                --bash <($out/bin/rf completions bash) \
                --zsh  <($out/bin/rf completions zsh) \
                --fish <($out/bin/rf completions fish)
            '';

            nativeBuildInputs = [ pkgs.installShellCompletion ];

            meta = with pkgs.lib; {
              description = "RavenFabric CLI — remote execution client";
              homepage = "https://ravenfabric.io";
              license = licenses.agpl3Plus;
              mainProgram = "rf";
            };
          };

          default = self.packages.${system}.rf-cli;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer
          ];
        };
      }
    );
}
