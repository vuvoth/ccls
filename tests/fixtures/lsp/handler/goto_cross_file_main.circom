pragma circom 2.0.0;

include "./goto_cross_file_lib.circom";

template Main() {
    signal input x;
    signal input y;
    signal output out;
    
    component mult = LibMultiplier();
    component add = LibAdder();
    
    mult.a <== x;
    mult.b <== y;
    add.a <== x;
    add.b <== y;
    out <== mult.c + add.c;
}
