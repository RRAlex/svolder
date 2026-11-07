# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `nix build` builds svolder from this checkout with nixpkgs' Rust, linking nixpkgs' libdbus;
# `nix run` starts the CLI. The user units land in lib/systemd/user, which a NixOS
# configuration picks up through `systemd.packages`.
{
  description = "Control Schiit Forkbeard devices over Bluetooth LE: a CLI, a TUI and a daemon";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
    in
    {
      packages = forAll (pkgs: rec {
        svolder = pkgs.rustPlatform.buildRustPackage {
          pname = "svolder";
          inherit version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
          buildInputs = [ pkgs.dbus ];
          # The suites spawn the binaries with private homes and a fake transport; `make check`
          # runs them, and the sandbox has no session for the ones that ask the system bus.
          doCheck = false;
          postInstall = ''
            mkdir -p $out/lib/systemd/user $out/share/doc/svolder
            for unit in svolderd.service svolderd.socket; do
              sed -e "s|__BINDIR__|$out/bin|g" -e 's|__SOCKET__|%t/svolder.sock|g' \
                systemd/$unit > $out/lib/systemd/user/$unit
            done
            cp README.md LEGAL.md AUTHORS LICENSE $out/share/doc/svolder/
            mkdir -p $out/share/doc/svolder/usage
            cp docs/usage/*.md $out/share/doc/svolder/usage/
            # The computer's volume and EQ spawn the PipeWire tools; a user unit's PATH on NixOS
            # holds none of them, so both binaries carry theirs after whatever PATH they get.
            for bin in svolderd svolder; do
              wrapProgram $out/bin/$bin \
                --suffix PATH : ${pkgs.lib.makeBinPath [ pkgs.pipewire pkgs.wireplumber ]}
            done
          '';
          meta = {
            description = "Control Schiit Forkbeard devices over Bluetooth LE";
            homepage = "https://github.com/RRAlex/svolder";
            license = pkgs.lib.licenses.gpl3Plus;
            mainProgram = "svolder";
            platforms = pkgs.lib.platforms.linux;
          };
        };
        default = svolder;
      });
    };
}
