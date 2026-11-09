//! The token reader behind the field sweep: literals that look like code, and
//! input that ends in the middle of a literal.

use super::field_sweep_lex::{tokenize, Token};

fn tokens(source: &str) -> Vec<Token> {
    tokenize(source).into_iter().map(|s| s.token).collect()
}

fn ident(word: &str) -> Token {
    Token::Ident(word.to_owned())
}

fn text(body: &str) -> Token {
    Token::Str(body.to_owned())
}

// L5: an unterminated literal used to return an index past the end of the
// input, and the caller sliced with it.

#[test]
fn an_unterminated_string_ends_at_the_end_of_the_input() {
    assert_eq!(
        tokens("let s = \"abc"),
        [ident("let"), ident("s"), Token::Punct('='), text("abc")]
    );
}

#[test]
fn an_unterminated_raw_string_ends_at_the_end_of_the_input() {
    assert_eq!(
        tokens("let s = r#\"abc"),
        [ident("let"), ident("s"), Token::Punct('='), text("abc")]
    );
}

#[test]
fn a_string_that_ends_in_a_backslash_ends_at_the_end_of_the_input() {
    assert_eq!(tokens("\"abc\\"), [text("abc")]);
    assert_eq!(tokens("b\"abc\\"), [text("abc")]);
}

#[test]
fn an_unterminated_char_literal_ends_at_the_end_of_the_input() {
    assert_eq!(tokens("'\\"), [Token::Other]);
    assert_eq!(tokens("'\\n"), [Token::Other]);
}

#[test]
fn an_unterminated_literal_keeps_the_line_count_of_what_it_swallowed() {
    let spanned = tokenize("\"a\nb\n");

    assert_eq!(spanned.len(), 1);
    assert_eq!(spanned[0].line, 1);
}

// L8: regression tests. These shapes already worked when the tests were
// written; the tests keep them working.

#[test]
fn a_quote_inside_a_char_literal_does_not_open_a_string() {
    assert_eq!(
        tokens("a = '\"', b = 1"),
        [
            ident("a"),
            Token::Punct('='),
            Token::Other,
            Token::Punct(','),
            ident("b"),
            Token::Punct('='),
            Token::Other,
        ]
    );
}

#[test]
fn a_quote_inside_a_byte_char_literal_does_not_open_a_string() {
    assert_eq!(
        tokens("a = b'\"', c"),
        [
            ident("a"),
            Token::Punct('='),
            ident("b"),
            Token::Other,
            Token::Punct(','),
            ident("c"),
        ]
    );
}

#[test]
fn a_raw_string_with_hashes_ends_only_at_its_own_closing_hashes() {
    let found = tokens("r##\"a \"# b\"## tail");

    assert_eq!(found, [text("a \"# b"), ident("tail")]);
}

#[test]
fn a_lifetime_is_not_a_char_literal() {
    assert_eq!(
        tokens("&'a str, &'static str"),
        [
            Token::Punct('&'),
            Token::Punct('\''),
            ident("a"),
            ident("str"),
            Token::Punct(','),
            Token::Punct('&'),
            Token::Punct('\''),
            ident("static"),
            ident("str"),
        ]
    );
}

#[test]
fn a_loop_label_is_not_a_char_literal_and_swallows_nothing() {
    assert_eq!(
        tokens("'outer: loop { break 'outer; }"),
        [
            Token::Punct('\''),
            ident("outer"),
            Token::Punct(':'),
            ident("loop"),
            Token::Punct('{'),
            ident("break"),
            Token::Punct('\''),
            ident("outer"),
            Token::Punct(';'),
            Token::Punct('}'),
        ]
    );
}
