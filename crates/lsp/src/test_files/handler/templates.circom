pragma circom 2.0.0;

        template X() {
            signal x[100];
            signal input x = 10;
           component x = Multiplier2();
           component y = X();
           component y = Multiplier2();
           component z = Multiplier2();
              
        }
template M() {
           component h = X();
           component k = Multiplier2(); 
           test
        }
template Multiplier2 () {  
           template m = M();
           // hello world
           signal input a;  
           signal input b;  
              signal output c;  
           component y = X();
           
           mintlkrekerjke;
           component e = Y();
           component z = Y();
           component h = Y();
           signal output d;
           c <== a * b; 
        }
template Y() {
           component y = X();
           component a = X();
           
        }