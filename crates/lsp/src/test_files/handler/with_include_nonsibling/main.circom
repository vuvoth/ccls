pragma circom 2.0.0;

include "lib.circom";

template Main() {
    signal input a;
    signal output c;
    component m = Lib();
    c <== m.out;
}
