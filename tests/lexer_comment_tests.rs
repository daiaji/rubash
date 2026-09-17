use rubash::lexer::{tokenize, TokenKind};

#[test]
fn test_comment_only() {
    let input = "# this is a comment";
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 0);
}

#[test]
fn test_command_before_comment() {
    let input = "ls # this lists";
    let tokens = tokenize(input);
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].value, "ls");
}

#[test]
fn test_quote_in_comment_does_not_continue_command() {
    let tokens: Vec<_> = tokenize("# let's not open a quote\nshopt -z")
        .into_iter()
        .filter(|token| token.kind != TokenKind::Semicolon)
        .collect();
    assert_eq!(tokens[0].value, "shopt");
    assert_eq!(tokens[0].position, 2);
    assert_eq!(tokens[1].value, "-z");
    assert_eq!(tokens[1].position, 2);
}

#[test]
fn test_quote_in_inline_comment_does_not_continue_command() {
    let tokens: Vec<_> = tokenize("shopt -p # list 'em all\nshopt -u")
        .into_iter()
        .filter(|token| token.kind != TokenKind::Semicolon)
        .collect();
    let words: Vec<_> = tokens.iter().map(|token| token.value.as_str()).collect();
    assert_eq!(words, vec!["shopt", "-p", "shopt", "-u"]);
    assert_eq!(tokens[2].position, 2);
}

#[test]
fn test_backtick_in_comment_does_not_continue_command_substitution() {
    let tokens: Vec<_> = tokenize("# mentions `eval' in prose\nexport -n var\necho ok")
        .into_iter()
        .filter(|token| token.kind != TokenKind::Semicolon)
        .collect();
    let words: Vec<_> = tokens.iter().map(|token| token.value.as_str()).collect();
    assert_eq!(words, vec!["export", "-n", "var", "echo", "ok"]);
    assert_eq!(tokens[3].position, 3);
}

// ---------------------------------------------------------------------------
// niubash #120 follow-up: a `#' comment sharing a line with a brace group
// must not be swallowed by the group token.
// ---------------------------------------------------------------------------

#[test]
fn test_brace_group_token_stops_before_trailing_comment() {
    let tokens: Vec<_> = tokenize("f() { echo x; } # note")
        .into_iter()
        .filter(|token| token.kind != TokenKind::Semicolon)
        .collect();
    let words: Vec<_> = tokens.iter().map(|token| token.value.as_str()).collect();
    assert_eq!(words, vec!["f", "(", ")", "{ echo x; }"]);
}

#[test]
fn test_unterminated_function_brace_is_a_bare_reserved_word() {
    // `f() { # note' ends its logical line inside the group, so the scanner
    // sees an unterminated brace: it must emit a bare `{' (the parser then
    // assembles the body from the following lines) instead of `{ # note'.
    let tokens: Vec<_> = tokenize("f() { # note\n  echo x\n}")
        .into_iter()
        .filter(|token| token.kind != TokenKind::Semicolon)
        .collect();
    let words: Vec<_> = tokens.iter().map(|token| token.value.as_str()).collect();
    assert_eq!(words, vec!["f", "(", ")", "{", "echo", "x", "}"]);
}

#[test]
fn test_brace_word_that_does_not_stand_alone_is_not_reserved() {
    // `{' is reserved only when it stands alone as a word, so an unterminated
    // word like `{a' keeps its text (GNU `echo {a' prints `{a').
    let tokens: Vec<_> = tokenize("echo {a")
        .into_iter()
        .filter(|token| token.kind != TokenKind::Semicolon)
        .collect();
    let words: Vec<_> = tokens.iter().map(|token| token.value.as_str()).collect();
    assert_eq!(words, vec!["echo", "{a"]);
}
