use nom::{
    branch::alt,
    bytes::complete::take_while1,
    character::complete::{anychar, char},
    combinator::{all_consuming, eof, map, value, verify},
    multi::{fold_many0, many_till},
    sequence::{delimited, preceded},
    IResult,
};

#[derive(Clone, Debug)]
pub(crate) enum Token {
    String(String),
    Var(String),
}

fn dollar(input: &str) -> IResult<&str, Token> {
    value(
        Token::String('$'.to_string()),
        preceded(char('$'), char('$')),
    )(input)
}

fn variable(input: &str) -> IResult<&str, Token> {
    alt((variable_unexpanded, variable_expanded))(input)
}

fn variable_unexpanded(input: &str) -> IResult<&str, Token> {
    map(
        preceded(
            char('$'),
            take_while1(|char: char| char.is_ascii_alphanumeric() || char == '_'),
        ),
        |name: &str| Token::Var(name.to_owned()),
    )(input)
}

fn variable_expanded(input: &str) -> IResult<&str, Token> {
    map(
        preceded(
            char('$'),
            delimited(
                char('{'),
                take_while1(|char: char| char.is_ascii_alphanumeric() || char == '_'),
                char('}'),
            ),
        ),
        |name: &str| Token::Var(name.to_owned()),
    )(input)
}

pub(crate) fn parse(input: &str) -> IResult<&str, Vec<Token>> {
    all_consuming(fold_many0(
        verify(
            many_till(
                anychar,
                alt((map(alt((variable, dollar)), Some), value(None, eof))),
            ),
            |(chars, output)| output.is_some() || !chars.is_empty(),
        ),
        Vec::new,
        |mut tokens, token| {
            if !token.0.is_empty() {
                if let Some(Token::String(string)) = tokens.last_mut() {
                    for char in token.0 {
                        string.push(char);
                    }
                } else {
                    let mut string = String::new();

                    for char in token.0 {
                        string.push(char);
                    }

                    tokens.push(Token::String(string));
                }
            }

            if let Some(var) = token.1 {
                if let (Some(Token::String(last)), Token::String(string)) =
                    (tokens.last_mut(), &var)
                {
                    *last = format!("{last}{string}");
                } else {
                    tokens.push(var);
                }
            }

            tokens
        },
    ))(input)
}
