pragma circom 2.0.0;

template SignalTest() {
    signal input a;
    signal input b;
    signal output c;
    signal intermediate;
    
    intermediate <== a + b;
    c <== intermediate * 2;
}

template ParamTest(N) {
    signal input in[N];
    signal output out;
    var sum = 0;
    
    for (var i = 0; i < N; i++) {
        sum += in[i];
    }
    out <== sum;
}
