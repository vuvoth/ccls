pragma circom 2.0.0;

include "lib.circom";

template Surface(N, M) {
    signal input in[N][M];
    signal output out;

    signal (a, b) <== in[0][0];
    var (x, y) = 0x1F;

    var h = 0xDEADBEEF \ 16;
    var bits = ~x | y ^ x << 2;
    var neg = !a;
    var cmp = (x != y) && (x >= 0) || (y <= 1);

    x += h;
    x -= 1;
    x *= 2;
    x /= 2;
    x %= 3;
    x **= 2;
    x \= 4;
    x &= bits;
    x |= h;
    x ^= y;
    x <<= 1;
    x >>= 2;

    out <== x + y;
}

component main {public [out]} = Surface(4, 2);
