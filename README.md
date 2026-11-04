# nix-cleaner

nix-cleaner is a small desktop application for nixOs that helps you keep your nix store clean. It presents GC roots, estimated store size and how much you stand to gain from freeing that space, it also gives you graphical control over cleaning the nix store with safety functionality to prevent you from deleting your last couple system and HM generations. 

## Requirements

- NixOS with flakes enabled.
- A local Nix store and `nix-store` available to the daemon and to the helper.
- Polkit with `pkexec` for dry runs and cleaning.


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
application from the checkout. Note that running with `nix run` will not register polkit, effectively putting the application in "read-only" mode unless run as root. use `nix run github:ojuny-project/nix-cleaner` to use the application for exploration of your nix store *without* any risk of modification. 

## Usage

Start `nix-cleaner` from the application menu or a terminal. It will show empty. Press "scan" in the top left to run a scan of the nix store, which will fill the 3 panels.

the GC roots panel is your primary cleanup method. here you can see every Garbage Collection root. a GC root causes its runtime dependencies to be kept during garbage collection. removing the root allows its contents and dependencies to be freed. 

the store panel allows you to look through all of your store items, alongside how many duplicates each item has in the store. folding out an entry allows you to see each path and the GC roots it is linked to. if an item is labeled "unused" here, that means it will be collected by the next garbage collection. 

the Dependency Tree page allows you to walk through any of your system or HM GC roots and show its dependency tree, showing the total size of each entry and its dependencies, as well as standalone size, this allows you to see what in your configuration is taking up space. 

To clean up your system, select any number of GC roots you wish to remove in the GC roots page, you can calculate the actual freeable space using the "calculate actual" button, then clean the selected GC roots with the "clean selected" button. 

the GC roots page will refuse to let you clean if you selected any of your last 3 HM/nixOS generations or the booted/current system/home manager generation. This is to prevent you from making your system unbootable. 

## Limits

Sizes are `narSize` values from the Nix database, not disk blocks. The optional
Measure on disk action walks one closure and reports block usage, including
blocks shared through hardlinks. A small drift against
`nix-store --gc --print-dead` is expected because temporary roots and paths held
open by running processes are outside the scan. Derivation paths appear like any
other store path. `df` is comparable only when the store has its own filesystem.

No non-NixOS module is provided. The application does not edit
`configuration.nix`, and it does not schedule automatic cleaning. Automatic cleaning can be scheduled directly in the nix daemon settings via `nix.conf` or in your `configuration.nix`

## Documentation
- [OPTIONS.md](OPTIONS.md) - generated options reference.

## Contributing

Open issues at <https://github.com/ojuny-project/nix-cleaner/issues>. A patch must keep
the helper's path checks intact, add or update a test for changed behavior, and
pass `cargo test` and `nix flake check` from the devshell.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE). 
