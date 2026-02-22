 pragma circom 2.0.0;

 template Exp() {
    var x[2][3] = [[2, 3], [3, 4], [5, 6]];
 } 

 template A() {
   component x = Exp()
 }