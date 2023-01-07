# `filewalker`

[![Crates.io](https://img.shields.io/crates/v/ssstar.svg)](https://crates.io/crates/ssstar)
[![Docs.rs](https://docs.rs/ssstar/badge.svg)](https://docs.rs/ssstar)
![CI](https://github.com/elastio/ssstar/workflows/CI/badge.svg)](https://github.com/elastio/ssstar/actions)

Taken and modified from [mrfutils-rs](https://github.com/lukerhoads/mrfutils-rs) which used it to walk lined `.txt` files from a specified location in the file.

## Quick Start

An example:
```rust
let mut forward = vec![];
for line in open_file("file.txt", None, None, None).unwrap() {
    println!(line);
}
```

Another way is to use the builder pattern:
```rust
let mut forward = vec![];
let opener = OpenerBuilder::default()
    .path("file.txt".to_string())
    .position("end")
    .direction("backward")
    .build()
    .unwrap()
for line in opener.open() {
    println!(line);
}
```