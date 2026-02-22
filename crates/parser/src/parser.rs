use crate::lexer::{tokenize, Diagnostic, Token};

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

impl<'a> ParserCallbacks<'a> for Parser<'a> {
    type Diagnostic = Diagnostic;
    type Context = ();

    fn create_tokens(
        _context: &mut Self::Context,
        source: &'a str,
        diags: &mut Vec<Self::Diagnostic>,
    ) -> (Vec<Token>, Vec<Span>) {
        tokenize(source, diags)
    }

    fn create_diagnostic(&self, span: Span, message: String) -> Self::Diagnostic {
        Diagnostic { message, span }
    }

    fn predicate_signal_init_1(&self) -> bool {
        matches!(self.peek(1), Token::ConstrainL | Token::ArrowL)
    }

    fn predicate_primary_expr_3(&self) -> bool {
        self.peek(1) != Token::RPar
    }
}
