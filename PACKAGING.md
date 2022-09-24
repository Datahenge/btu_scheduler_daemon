### Packaging for Debian
Using a lovely Rust crate "cargo-deb", available on crates.io

### Configuration
Without a few extra tricks, you cannot build a Debian Package against a Workspace crate.

1. Choose an arbitrary crate in your workspace to "own" the Debian Package configuration.  I choose "btu_scheduler"

2. Add this to the crate's Cargo.toml

```

[package.metadata.deb]
name = "BTU Scheduler"
maintainer = "Brian Pond <brian@datahenge.com>"
copyright = "2022, Brian Pond <brian@datahenge.com>"
license-file = ["../LICENSE.txt", "4"]
extended-description = """\
A simple subcommand for the Cargo package manager for \
building Debian packages from Rust projects."""
# depends = "$auto"
# section = "utility"
# priority = "optional"
assets = [
  ["target/release/btu", "usr/bin/", "755"],
  ["target/release/btu-daemon", "usr/bin/", "755"],
]
```

### Building the Debian Package
From the package root, execute 2 shell commands:
```
cargo build --release
cargo deb -p btu_scheduler
```

### Installation
To try and install it, do this:

```
dpkg -i target/debian/btu_cli_0.3.3_amd64.deb
```
