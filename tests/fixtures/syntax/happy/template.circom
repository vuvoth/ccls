template MultiplierN (N, P, QQ) {
            //Declaration of signals and components.
            signal input in[N];
            signal output out;
            component comp[N-1];
             
            //Statements.
            for(var i = 0; i < N-1; i++){
                comp[i] = Multiplier2();
            }
                
}
         