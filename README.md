# RustPython OS
![demo of pythonos](./demo.png)

This is a proof-of-concept which demonstrates (a fork of) Rustpython running in a bare-metal x86 environment using `#![no_std]` Rust.

It makes use of the PS/2 and VGA subsystems. It is single threaded, single process only.

## Building
Install `cargo bootimage` and run it.
