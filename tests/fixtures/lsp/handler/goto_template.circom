pragma circom 2.0.0;

template Multiplier2() {
    signal input a;
    signal input b;
    signal output c;
    c <== a * b;
}

template Main() {
    signal input x;
    signal input y;
    signal output out;
    
    component mult = Multiplier2();
    mult.a <== x;
    mult.b <== y;
    out <== mult.c;
}
