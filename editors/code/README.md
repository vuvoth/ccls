# Circom lsp

Better support for circom. 

## Features 

- Go to definition
- Support circom 2 


## What makes it different?

This can process invalid circom file :D. 

For example this circom-plus can process this file.

```circom
pragma circom 2.0.1;

template Adder() {
    // config signal for x
    signal input x;
    x <== 100;
    test
}

template Another() {
    component adder = Adder(); 

}
```

## Requirements

- [Rust](https://www.rust-lang.org/) toolchain (to build the language server).
- [bun](https://bun.sh) (to build the VS Code extension).

## Install 
I recommend installing via these commands:

```bash
git clone https://github.com/vuvoth/circom-plus
cd circom-plus
cargo xtask install --server
cargo xtask install --client
```
This makes the extension install flow much smoother. 

## Bugs 

If you want to request feature or report bug, please create issue on this repo: https://github.com/vuvoth/ccls
