{
    // if ... else 
    if(n == 2) {
        aux <== 2;
        out <== B()(aux);
    } else {
        out <== 5;
    }

    // for
    for(var i = 0; i < N-1; i++){
        comp[i] = Multiplier2();
    }

    // while
    while (n-1<a) {
        r++;
        n *= 2;
    }

    // return
    return r;

    // log
    log("hash", hash.out);

    // assert
    assert(a > 2);

    // assignment statement
    c <== a * b;
}