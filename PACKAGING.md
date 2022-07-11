### Packaging for Debian

To create a Debian package:

```
cargo deb
```

First, you will get an error message like this:
    
    cargo-deb: This is a workspace with multiple packages, and there is no single package at the root.
    Please specify package name with -p.
    Available packages are: btu_cli, btu_daemon, btu_scheduler

So instead use this syntax:

```
cargo deb -p btu_cli
cargo deb -p btu_daemon
```

To try and install it, do this:
```
dpkg -i target/debian/btu_cli_0.3.3_amd64.deb
```
