# Circom lsp

Better support for circom. 

## Features 

- Go to definition
- Support circom 2 


## What make it different? 

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

## Install 
I remcoment install via those commands:

```bash
git clone https://github.com/vuvoth/ccls
cd ccls 
cargo xtask install --server 
cargo xtask install --client
```
This make install extension flow work more smooth. 

## Bugs 

If you want to request feature or report bug, please create issue on this repo: https://github.com/vuvoth/ccls
