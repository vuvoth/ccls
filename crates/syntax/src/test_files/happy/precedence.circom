{
    // Associativity (must be LEFT-associative): a - b - c == (a - b) - c
    var left_sub = a - b - c;
    var left_div = a / b / c;
    var mixed_add_sub = a - b + c;
    // Precedence: bitwise binds tighter than comparisons (a & b == c == (a & b) == c)
    var bitwise_vs_cmp = a & b == c;
    var cmp_vs_bitwise = a == b & c;
    // * before + (a * b + c)
    var mul_before_add = a * b + c;
    var add_after_mul = a + b * c;
    // ** is left-associative in circom: a ** b ** c == (a ** b) ** c
    var power_left = a ** b ** c;
    // prefix - is TIGHTER than ** (grammar Expression2 < Expression3): -a ** b == (-a) ** b
    var prefix_vs_power = -a ** b;
    // || looser than && : a || b && c == a || (b && c)
    var bool_precedence = a || b && c;
    // ternary (branches at || level)
    var ternary = a ? b : c;
}
