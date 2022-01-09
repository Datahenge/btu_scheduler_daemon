## Building BTU Scheduler

### Steps
Update to latest version of Rust.
```
rustup update stable
```

### Regarding version of glibc
The GNU C library (glibc) is the GNU implementation of the standard C library. Rust depends greatly on this library.
Depending on what OS you build a Rust program, your binary will be required to link to a particular version of glibc.

#### A real world example:
On January 9th 2022, I built `btu` and `btu-daemon` on Debian 11 (Bullseye).  Then I deployed to a Debian 10 (Buster) environment.  I immediately encountered problems with `glibc`.

```
./btu-daemon: /lib/x86_64-linux-gnu/libm.so.6: version `GLIBC_2.29' not found (required by ./btu-daemon)
```

* Target Machine (Debian 10 Buster):  The version of **glibc** was 2.28
* Build Machine (Debian 11 Bullseye):  The version of **glibc** was 2.31

#### What version do I have?
To find the version of `glibc` installed on a particular machine:
```bash
ldd --version
```
### What now?
To handle this **glibc** problem, we need to build *different* binaries for Debian 11, Debian 10, Ubuntu 18.04, etc.  One binary per target.

However:

* I don't have a room full of different computers, with different operating systems.
* I *really* don't want to manage a suite of different VMs or cloud VPCs, with different operating systems.
* I don't have the time & energy to construct my own suite of Docker images for building.

So...I decided to use **GitHub Actions**.  Which is kind of like doing your own Docker images and Bash scripting.

Except the scripting and "glue" work has already been done for me.  So it's (*theoretically*) less work.

## GitHub Actions

### Useful Articles and Links
* https://kobzol.github.io/rust/ci/2021/05/07/building-rust-binaries-in-ci-that-work-with-older-glibc.html
* https://github.com/marketplace/actions/rust-action
