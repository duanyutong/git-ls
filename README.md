# git-ls

`git-ls` renders local Git branch stacks as compact coloured lanes for use as `git ls`.

![Example stack display](docs/assets/git-ls-example.svg)

## Integration Support

Supported VCS backends are:

- Git, optionally using [git-branchless](https://github.com/arxanas/git-branchless) for custom revsets and hidden-commit selection.

Git is required for both the native `gix` backend and the Git subprocess backend.

## Installation

Source installation requires Rust.

```sh
cargo install --path . --locked
```

Installation details and display examples are documented in [common commands](docs/commands.md).

## Configuration

Settings reside in the `[git-ls]` section of Git configuration.
Example `~/.gitconfig` section:

```gitconfig
[git-ls]
    backend = gix
    layout = columns
    palette = classic
    verbosity = 2
    cache = true
    cacheMaintenance = true
```

Option definitions and precedence are documented in [configuration](docs/configuration.md).
Development instructions appear in [development](docs/development.md).
The project is licensed under [AGPL-3.0-only](LICENSE).
