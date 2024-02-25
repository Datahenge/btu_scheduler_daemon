### Building the binaries with Rust tools

You'll need musl-tools, to compile 'ring'

```
musl-dev
sudo apt-get install musl-tools
```

#### Without any Dynamic Libraries

This avoids certain libc dependencies.
However, it also results in problems with OpenSSL.


```
cargo build --target x86_64-unknown-linux-musl
```
