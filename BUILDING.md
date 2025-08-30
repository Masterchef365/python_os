First we want to install the bootimage crate:
```sh
cargo install bootimage
```

Next we install some rustc dependencies
```sh
rustup component add rust-src --toolchain nightly-2024-12-26-x86_64-unknown-linux-gnu
rustup component add llvm-tools-preview --toolchain nightly-2024-12-26-x86_64-unknown-linux-gnu
```

Finally we build the project:
```sh
./build.sh
```

And virtualize it!:
```sh
./qemu.sh
```
