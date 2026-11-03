# nix-cleaner

nix-cleaner is a GTK4 and libadwaita desktop application for NixOS that shows
what occupies the Nix store, which garbage collection roots keep each path
alive, and what removing a selected set of roots would free. It wraps the local
Nix store, `nix-store`, and `nix-collect-garbage`; the package and the module
target Linux only.

## Status

Untested. The version in `Cargo.toml` and `flake.nix` is 0.1.0. No end-to-end
run is recorded for this page. The workspace has unit tests in
`engine/tests/` and in the helper, and `checks.default` builds the package.

## Requirements

- NixOS with flakes enabled.
- A local Nix store and `nix-store` available to the daemon and to the helper.
- Polkit with `pkexec` for dry runs and cleaning.
- GTK4 and libadwaita at runtime; the package supplies both.
- A Rust toolchain, `cargo`, and `rustfmt` only when working from the devshell.

The helper is not wrapped because `pkexec` refuses shell scripts. The module
installs the package and the polkit action.

## Install

Add the flake as an input and enable the module:

```nix
{
  inputs.nix-cleaner.url = "github:ojuny-project/nix-cleaner";

  outputs = inputs@{ nixpkgs, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        inputs.nix-cleaner.nixosModules.default
        { programs.nix-cleaner.enable = true; }
      ];
    };
  };
}
```

Rebuild the system. The module installs `nix-cleaner`, the helper, and the
polkit action `dev.den.nix-cleaner.clean`. Check the result:

```console
$ command -v nix-cleaner
/run/current-system/sw/bin/nix-cleaner
```

The package can also be built alone with `nix build`. It writes
`result/bin/nix-cleaner` and a desktop entry, and `nix run` starts the
application from the checkout.

## Usage

Start `nix-cleaner` from the application menu or a terminal. Select a flake or
root group in the Roots panel, then select the roots to remove. The window
writes a plan file and calls the helper through `pkexec`; it never modifies
roots itself.

A dry run stages the selected links, asks `nix-store --gc --print-dead` for the
dead paths, reports the count and total `narSize`, then restores every link.
Expected output from the helper is JSON with `"ok": true`, `"mode": "dry-run"`,
and `dead_paths` and `dead_bytes` fields. A clean run removes the staged links,
runs `nix-collect-garbage`, and is irreversible once collection succeeds.

The helper refuses `/nix/var/nix/profiles/system`, paths below `/run/` and
`/etc/`, and links outside the profiles directory, per-user gcroots, or the
plan's home directory. Roots that protect the running system, the booted
system, the active generation, the newest three generations, or an active
profile cannot be selected.

## Limits

Sizes are `narSize` values from the Nix database, not disk blocks. The optional
Measure on disk action walks one closure and reports block usage, including
blocks shared through hardlinks. A small drift against
`nix-store --gc --print-dead` is expected because temporary roots and paths held
open by running processes are outside the scan. Derivation paths appear like any
other store path. `df` is comparable only when the store has its own filesystem.

No non-NixOS module is provided. The application does not edit
`configuration.nix`, and it does not schedule automatic cleaning.

## Documentation

- [flake.nix](flake.nix) lists the packages, app, check, devshell, and module.
- [modules/nixos.nix](modules/nixos.nix) defines the module options and polkit action.
- [engine/src/scan.rs](engine/src/scan.rs) shows how store paths and roots are read.
- [helper/src/main.rs](helper/src/main.rs) contains the plan validation and staging rules.
- [engine/src/disk.rs](engine/src/disk.rs) contains the disk measurement used by the button.

## Contributing

Open issues at <https://github.com/ojuny-project/nix-cleaner/issues>. A patch must keep
the helper's path checks intact, add or update a test for changed behavior, and
pass `cargo test` and `nix flake check` from the devshell.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE). `Cargo.toml` and the flake metadata
state the same license.
