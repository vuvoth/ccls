pragma circom 2.0.0;

template LibMultiplier() {
    signal input a;
    signal input b;
    signal output c;
    c <== a * b;
}

template LibAdder() {
    signal input a;
    signal input b;
    signal output c;
    c <== a + b;
}

function libHelper(x) {
    return x * 2;
}
