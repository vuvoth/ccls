pragma circom 2.0.1;

/* This is good teamplatehello
*/
template Adder() {
    // config signal for x
    signal input x;
    x <== 100;
}


template A() {
    component a = Adder();
    a.x <== 100;
    var b = 10; 
    var c = b + 10;

    signal x; 
    x <== 10; 

    signal y; 
    y <== x + 100;

    c = 100 + 10; 
    
}


