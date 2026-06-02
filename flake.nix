{
  description = "edge-relay — Cloudflare Worker (Rust/WASM) dev tooling";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        fx = fenix.packages.${system};

        # Stable Rust plus the wasm32-unknown-unknown target std that Workers need.
        rustToolchain = fx.combine [
          fx.stable.toolchain
          fx.targets.wasm32-unknown-unknown.stable.rust-std
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.wrangler # Cloudflare CLI: `wrangler dev` / `wrangler deploy`
            pkgs.worker-build # compiles the Rust worker to a deployable WASM bundle
            pkgs.binaryen # wasm-opt, invoked by worker-build
            pkgs.cargo-tarpaulin # test coverage (`cargo tarpaulin`)
            pkgs.python3 # stub server for the outbound integration test
          ];

          shellHook = ''
            echo "edge-relay devshell — $(rustc --version)"
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
