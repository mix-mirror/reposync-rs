{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;
      forAllSystems = lib.genAttrs nixpkgs.lib.systems.flakeExposed;
      pkgsBySystem = forAllSystems (
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        }
      );
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsBySystem.${system};
          rust = pkgs.rust-bin.stable.latest.default;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rust;
            rustc = rust;
          };
        in
        {
          reposync-rs = rustPlatform.buildRustPackage (finalAttrs: {
            name = "reposync-rs";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.perl ];
            meta.mainProgram = finalAttrs.name;
          });
          default = self.packages.${system}.reposync-rs;
        }
      );
    };
}
