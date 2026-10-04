{
  description = "ocplay — a console emulator for OpenComputers";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      ocplayFor = pkgs: pkgs.rustPlatform.buildRustPackage {
        pname = "ocplay";
        version = "0.1.0";

        # Ship the hand-written sources and the vendored OpenOS, but not the
        # build output.
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let base = baseNameOf (toString path);
            in !(builtins.elem base [ "target" "result" ".direnv" ]);
        };

        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = [ pkgs.makeWrapper ];

        # Non-sandboxed builds get HOME=/homeless-shelter; keep cargo's home in
        # the build directory so it never writes to (or is blocked by) that path.
        preConfigure = ''
          export HOME="$TMPDIR"
        '';

        # The binary locates the vendored system Lua via $OCPLAY_SYSTEM first,
        # so install the assets and point the wrapper at them.
        postInstall = ''
          mkdir -p $out/share/ocplay
          cp -r assets/system $out/share/ocplay/system
          wrapProgram $out/bin/ocplay \
            --set OCPLAY_SYSTEM "$out/share/ocplay/system"
        '';

        # The end-to-end tests boot OpenOS and bind a loopback socket; they run
        # in `cargo test` locally/CI, not during the Nix build.
        doCheck = false;

        meta = with pkgs.lib; {
          description = "Console emulator for OpenComputers";
          homepage = "https://github.com/MagicBOTAlex/OCPlayground";
          license = licenses.mit;
          mainProgram = "ocplay";
          platforms = platforms.unix;
        };
      };
    in {
      packages = forAllSystems (pkgs:
        let ocplay = ocplayFor pkgs;
        in {
          inherit ocplay;
          default = ocplay;
        });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${ocplayFor pkgs}/bin/ocplay";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rust-analyzer clippy rustfmt ];
        };
      });
    };
}
