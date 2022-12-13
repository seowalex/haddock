use anyhow::{anyhow, Result};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{anychar, char},
    combinator::{all_consuming, cut, eof, map, map_parser, value, verify},
    multi::{fold_many0, many_till},
    sequence::{delimited, preceded, tuple},
    Finish, IResult,
};
use parse_hyperlinks::take_until_unbalanced;

#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum Token {
    Str(String),
    Var(String, Option<Var>),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum Var {
    Default(State, Vec<Token>),
    Err(State, Vec<Token>),
    Replace(State, Vec<Token>),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum State {
    Set,
    SetAndNonEmpty,
}

fn dollar_or_variable(input: &str) -> IResult<&str, Token> {
    preceded(char('$'), cut(alt((dollar, variable, variable_expanded))))(input)
}

fn dollar(input: &str) -> IResult<&str, Token> {
    value(Token::Str('$'.to_string()), char('$'))(input)
}

fn name(input: &str) -> IResult<&str, &str> {
    take_while1(|char: char| char.is_ascii_alphanumeric() || char == '_')(input)
}

fn variable(input: &str) -> IResult<&str, Token> {
    map(name, |name| Token::Var(name.to_string(), None))(input)
}

fn variable_expanded(input: &str) -> IResult<&str, Token> {
    map_parser(
        delimited(char('{'), take_until_unbalanced('{', '}'), char('}')),
        cut(alt((parameter, parameter_expanded))),
    )(input)
}

fn parameter(input: &str) -> IResult<&str, Token> {
    all_consuming(variable)(input)
}

fn parameter_expanded(input: &str) -> IResult<&str, Token> {
    map(
        all_consuming(tuple((
            name,
            alt((
                tag(":-"),
                tag("-"),
                tag(":?"),
                tag("?"),
                tag(":+"),
                tag("+"),
            )),
            string,
        ))),
        |(name, separator, tokens)| {
            Token::Var(
                name.to_string(),
                match separator {
                    ":-" => Some(Var::Default(State::SetAndNonEmpty, tokens)),
                    "-" => Some(Var::Default(State::Set, tokens)),
                    ":?" => Some(Var::Err(State::SetAndNonEmpty, tokens)),
                    "?" => Some(Var::Err(State::Set, tokens)),
                    ":+" => Some(Var::Replace(State::SetAndNonEmpty, tokens)),
                    "+" => Some(Var::Replace(State::Set, tokens)),
                    _ => unreachable!(),
                },
            )
        },
    )(input)
}

fn string(input: &str) -> IResult<&str, Vec<Token>> {
    fold_many0(
        verify(
            many_till(
                anychar,
                alt((map(dollar_or_variable, Some), value(None, eof))),
            ),
            |(chars, token)| token.is_some() || !chars.is_empty(),
        ),
        Vec::new,
        |mut tokens, token| {
            if !token.0.is_empty() {
                if let Some(Token::Str(string)) = tokens.last_mut() {
                    for char in token.0 {
                        string.push(char);
                    }
                } else {
                    let mut string = String::new();

                    for char in token.0 {
                        string.push(char);
                    }

                    tokens.push(Token::Str(string));
                }
            }

            if let Some(var) = token.1 {
                if let (Some(Token::Str(last)), Token::Str(string)) = (tokens.last_mut(), &var) {
                    last.push_str(string);
                } else {
                    tokens.push(var);
                }
            }

            tokens
        },
    )(input)
}

pub(crate) fn parse(input: &str) -> Result<Vec<Token>> {
    all_consuming(string)(input)
        .finish()
        .map(|(_, tokens)| tokens)
        .map_err(|_| anyhow!("invalid interpolation format for \"{input}\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string() {
        assert_eq!(
            parse("foo").ok(),
            Some(vec![Token::Str(String::from("foo"))])
        );
    }

    #[test]
    fn variable() {
        assert_eq!(
            parse("$foo").ok(),
            Some(vec![Token::Var(String::from("foo"), None)])
        );
    }

    #[test]
    fn variable_with_leading_string() {
        assert_eq!(
            parse(" $foo").ok(),
            Some(vec![
                Token::Str(String::from(" ")),
                Token::Var(String::from("foo"), None)
            ])
        );
    }

    #[test]
    fn variable_with_trailing_string() {
        assert_eq!(
            parse("$foo ").ok(),
            Some(vec![
                Token::Var(String::from("foo"), None),
                Token::Str(String::from(" "))
            ])
        );
    }

    #[test]
    fn variables() {
        assert_eq!(
            parse("$foo$bar").ok(),
            Some(vec![
                Token::Var(String::from("foo"), None),
                Token::Var(String::from("bar"), None)
            ])
        );
    }

    #[test]
    fn variables_with_separating_string() {
        assert_eq!(
            parse("$foo $bar").ok(),
            Some(vec![
                Token::Var(String::from("foo"), None),
                Token::Str(String::from(" ")),
                Token::Var(String::from("bar"), None)
            ])
        );
    }

    #[test]
    fn empty_string() {
        assert_eq!(parse("").ok(), Some(vec![]));
    }

    #[test]
    fn escaped_dollar_sign() {
        assert_eq!(
            parse("$$foo").ok(),
            Some(vec![Token::Str(String::from("$foo"))])
        );
    }

    #[test]
    fn single_dollar_sign() {
        assert_eq!(
            parse("$").err().map(|err| err.to_string()),
            Some(String::from("invalid interpolation format for \"$\""))
        );
    }

    #[test]
    fn expanded_variable() {
        assert_eq!(
            parse("${foo}").ok(),
            Some(vec![Token::Var(String::from("foo"), None)])
        );
    }

    #[test]
    fn expanded_variable_with_leading_string() {
        assert_eq!(
            parse(" ${foo}").ok(),
            Some(vec![
                Token::Str(String::from(" ")),
                Token::Var(String::from("foo"), None)
            ])
        );
    }

    #[test]
    fn expanded_variable_with_trailing_string() {
        assert_eq!(
            parse("${foo} ").ok(),
            Some(vec![
                Token::Var(String::from("foo"), None),
                Token::Str(String::from(" "))
            ])
        );
    }

    #[test]
    fn expanded_variables() {
        assert_eq!(
            parse("${foo}${bar}").ok(),
            Some(vec![
                Token::Var(String::from("foo"), None),
                Token::Var(String::from("bar"), None)
            ])
        );
    }

    #[test]
    fn expanded_variables_with_separating_string() {
        assert_eq!(
            parse("${foo} ${bar}").ok(),
            Some(vec![
                Token::Var(String::from("foo"), None),
                Token::Str(String::from(" ")),
                Token::Var(String::from("bar"), None)
            ])
        );
    }

    #[test]
    fn empty_expanded_variable() {
        assert_eq!(
            parse("${}").err().map(|err| err.to_string()),
            Some(String::from("invalid interpolation format for \"${}\""))
        );
    }

    #[test]
    fn expanded_variable_with_default_if_unset_or_empty() {
        assert_eq!(
            parse("${foo:-bar}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Default(
                    State::SetAndNonEmpty,
                    vec![Token::Str(String::from("bar"))]
                ))
            )])
        );
    }

    #[test]
    fn expanded_variable_with_default_if_unset() {
        assert_eq!(
            parse("${foo-bar}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Default(
                    State::Set,
                    vec![Token::Str(String::from("bar"))]
                ))
            )])
        );
    }

    #[test]
    fn expanded_variable_with_error_if_unset_or_empty() {
        assert_eq!(
            parse("${foo:?bar}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Err(
                    State::SetAndNonEmpty,
                    vec![Token::Str(String::from("bar"))]
                ))
            )])
        );
    }

    #[test]
    fn expanded_variable_with_error_if_unset() {
        assert_eq!(
            parse("${foo?bar}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Err(State::Set, vec![Token::Str(String::from("bar"))]))
            )])
        );
    }

    #[test]
    fn expanded_variable_with_replacement_if_set_and_non_empty() {
        assert_eq!(
            parse("${foo:+bar}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Replace(
                    State::SetAndNonEmpty,
                    vec![Token::Str(String::from("bar"))]
                ))
            )])
        );
    }

    #[test]
    fn expanded_variable_with_replacement_if_set() {
        assert_eq!(
            parse("${foo+bar}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Replace(
                    State::Set,
                    vec![Token::Str(String::from("bar"))]
                ))
            )])
        );
    }

    #[test]
    fn nested_expanded_variable() {
        assert_eq!(
            parse("${foo:-${bar}}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Default(
                    State::SetAndNonEmpty,
                    vec![Token::Var(String::from("bar"), None)]
                ))
            )])
        );
    }

    #[test]
    fn double_nested_expanded_variable() {
        assert_eq!(
            parse("${foo:-${bar:-${hello}}}").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Default(
                    State::SetAndNonEmpty,
                    vec![Token::Var(
                        String::from("bar"),
                        Some(Var::Default(
                            State::SetAndNonEmpty,
                            vec![Token::Var(String::from("hello"), None)]
                        ))
                    )]
                ))
            )])
        );
    }

    #[test]
    fn nested_expanded_variable_with_leading_and_trailing_strings() {
        assert_eq!(
            parse("${foo:- ${bar} }").ok(),
            Some(vec![Token::Var(
                String::from("foo"),
                Some(Var::Default(
                    State::SetAndNonEmpty,
                    vec![
                        Token::Str(String::from(" ")),
                        Token::Var(String::from("bar"), None),
                        Token::Str(String::from(" "))
                    ]
                ))
            )])
        );
    }

    #[test]
    fn expanded_variable_with_illegal_name() {
        assert_eq!(
            parse("${foo$}").err().map(|err| err.to_string()),
            Some(String::from("invalid interpolation format for \"${foo$}\""))
        );
    }
}
