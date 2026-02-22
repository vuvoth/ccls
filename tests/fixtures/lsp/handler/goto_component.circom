pragma circom 2.0.0;

template Multiplier() {
    signal input a;
    signal input b;
    signal output c;
    c <== a * b;
}

template Adder() {
    signal input a;
    signal input b;
    signal output c;
    c <== a + b;
}

template ComponentTest() {
    signal input x;
    signal input y;
    signal output out;
    
    component mult = Multiplier();
    component add = Adder();
    
    mult.a <== x;
    mult.b <== y;
    add.a <== x;
    add.b <== y;
    out <== mult.c + add.c;
}
