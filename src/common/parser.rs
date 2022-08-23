pub type ParseResult<'a, T> = Result<(ParseInput<'a>, T), ParseError>;
pub type ParseInput<'a> = &'a [u8];
pub type ParseError = &'static str;

macro_rules! any_of {
    ($lt:lifetime, $($parser:expr),+ $(,)?) => {
        |input: $crate::parser::ParseInput <$lt>| {
            $(
                if let Ok((input, value)) = $parser(input) {
                    return Ok((input, value));
                }
            )+

            Err("No parser succeeded")
        }
    };
}

/// Succeeds if input is empty, otherwise fails
pub fn empty(input: ParseInput) -> ParseResult<()> {
    if input.is_empty() {
        Ok((input, ()))
    } else {
        Err("not empty")
    }
}

/// Succeeds if input is not empty, otherwise fails
#[inline]
pub fn not_empty(input: ParseInput) -> ParseResult<()> {
    not(empty)(input)
}

/// Succeeds if parser fully consumes input, otherwise fails
pub fn fully_consumed<'a, T>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, T> {
    move |input: ParseInput| {
        let (input, value) = parser(input)?;
        let (input, _) = empty(input)?;
        Ok((input, value))
    }
}

/// Map a parser's result
pub fn map<'a, T, U>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
    f: impl Fn(T) -> U,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, U> {
    move |input: ParseInput| {
        let (input, value) = parser(input)?;
        Ok((input, f(value)))
    }
}

/// Execute three parsers in a row, failing if any fails, and returns first and third parsers' results
pub fn divided<'a, T1, T2, T3>(
    mut left: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T1>,
    mut middle: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T2>,
    mut right: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T3>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, (T1, T3)> {
    move |input: ParseInput| {
        let (input, v1) = left(input)?;
        let (input, _) = middle(input)?;
        let (input, v3) = right(input)?;
        Ok((input, (v1, v3)))
    }
}

/// Execute two parsers in a row, failing if either fails, and returns second parser's result
pub fn prefixed<'a, T1, T2>(
    mut prefix: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T1>,
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T2>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, T2> {
    move |input: ParseInput| {
        let (input, _) = prefix(input)?;
        let (input, value) = parser(input)?;
        Ok((input, value))
    }
}

/// Execute two parsers in a row, failing if either fails, and returns first parser's result
pub fn suffixed<'a, T1, T2>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T1>,
    mut suffix: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T2>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, T1> {
    move |input: ParseInput| {
        let (input, value) = parser(input)?;
        let (input, _) = suffix(input)?;
        Ok((input, value))
    }
}

/// Execute a parser, returning Some(value) if succeeds and None if fails
pub fn maybe<'a, T>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, Option<T>> {
    move |input: ParseInput| match parser(input) {
        Ok((input, value)) => Ok((input, Some(value))),
        Err(_) => Ok((input, None)),
    }
}

/// Executes a parser, failing if the parser succeeds
pub fn not<'a, T>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, ()> {
    move |input: ParseInput| match parser(input) {
        Ok(_) => Err("parser succeeded"),
        Err(_) => Ok((input, ())),
    }
}

/// Executes the parser without consuming the input
pub fn peek<'a, T>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, T> {
    move |input: ParseInput| {
        let (_, value) = parser(input)?;
        Ok((input, value))
    }
}

/// Takes while the parser returns true, returning a collection of parser results, or failing if
/// the parser did not succeed at least once
pub fn one_or_more<'a, T>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, Vec<T>> {
    move |input: ParseInput| {
        let mut results = Vec::new();
        let mut next = Some(input);
        while let Some(input) = next.take() {
            match parser(input) {
                Ok((input, value)) => {
                    next = Some(input);
                    results.push(value);
                }
                Err(_) => {
                    next = Some(input);
                    break;
                }
            }
        }

        if results.is_empty() {
            return Err("Parser failed to suceed once");
        }

        Ok((next.unwrap(), results))
    }
}

/// Same as [`one_or_more`], but won't fail if the parser never succeeds
///
/// ### Note
///
/// This will ALWAYS succeed since it will just return an empty collection on failure.
/// Be careful to not get stuck in an infinite loop here!
pub fn zero_or_more<'a, T>(
    parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, Vec<T>> {
    let mut parser = maybe(one_or_more(parser));

    move |input: ParseInput| {
        let (input, results) = parser(input)?;
        Ok((input, results.unwrap_or_default()))
    }
}

/// Takes until `parser` fails
pub fn take_while<'a, T>(
    mut parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, ParseInput> {
    move |input: ParseInput| {
        if input.is_empty() {
            return Err("Empty input");
        }

        let len = input.len();
        let mut i = 0;
        while i < len {
            // Advance by parser consumption
            match parser(&input[i..]) {
                Ok((remaining, _)) => {
                    let available_len = len - i;
                    let consumed_len = available_len - remaining.len();
                    i += consumed_len;
                }
                Err(_) => break,
            }
        }

        if i == len {
            // Consumed everything
            Ok((b"", input))
        } else if i == 0 {
            // Consumed nothing
            Ok((input, b""))
        } else {
            // Consumed something
            Ok((&input[i..], &input[..i]))
        }
    }
}

/// Same as [`take_while`], but fails if does not consume at least one byte
pub fn take_while_1<'a, T>(
    parser: impl FnMut(ParseInput<'a>) -> ParseResult<'a, T>,
) -> impl FnMut(ParseInput<'a>) -> ParseResult<'a, ParseInput> {
    let mut parser = take_while(parser);

    move |input: ParseInput| {
        let (input, value) = parser(input)?;

        if value.is_empty() {
            return Err("did not consume 1 byte");
        }

        Ok((input, value))
    }
}

/// Takes until `predicate` returns true
pub fn take_until_byte(
    mut predicate: impl FnMut(u8) -> bool,
) -> impl FnMut(ParseInput) -> ParseResult<ParseInput> {
    move |input: ParseInput| {
        let (input, value) = match input.iter().enumerate().find(|(_, b)| predicate(**b)) {
            Some((i, _)) => (&input[i..], &input[..i]),
            None => (b"".as_slice(), input),
        };

        Ok((input, value))
    }
}

/// Same as [`take_until_byte`], but fails if does not consume at least one byte
pub fn take_until_byte_1(
    predicate: impl FnMut(u8) -> bool,
) -> impl FnMut(ParseInput) -> ParseResult<ParseInput> {
    let mut parser = take_until_byte(predicate);

    move |input: ParseInput| {
        let (input, value) = parser(input)?;

        if value.is_empty() {
            return Err("did not consume 1 byte");
        }

        Ok((input, value))
    }
}

/// Takes from back until `predicate` returns true
pub fn rtake_until_byte(
    mut predicate: impl FnMut(u8) -> bool,
) -> impl FnMut(ParseInput) -> ParseResult<ParseInput> {
    move |input: ParseInput| {
        let (input, value) = match input.iter().enumerate().rev().find(|(_, b)| predicate(**b)) {
            Some((i, _)) => (&input[i..], &input[..i]),
            None => (b"".as_slice(), input),
        };

        Ok((input, value))
    }
}

/// Same as [`rtake_until_byte`], but fails if does not consume at least one byte
pub fn rtake_until_byte_1(
    predicate: impl FnMut(u8) -> bool,
) -> impl FnMut(ParseInput) -> ParseResult<ParseInput> {
    let mut parser = rtake_until_byte(predicate);

    move |input: ParseInput| {
        let (input, value) = parser(input)?;

        if value.is_empty() {
            return Err("did not consume 1 byte");
        }

        Ok((input, value))
    }
}

/// Takes `cnt` bytes, failing if not enough bytes are available
pub fn take(cnt: usize) -> impl FnMut(ParseInput) -> ParseResult<ParseInput> {
    move |input: ParseInput| {
        if cnt == 0 {
            Err("take(cnt) cannot have cnt == 0")
        } else if cnt > input.len() {
            Err("take(cnt) not enough bytes")
        } else {
            Ok((&input[cnt..], &input[..cnt]))
        }
    }
}

/// Parse multiple bytes, failing if they do not match `bytes` or there are not enough bytes
pub fn bytes<'a>(bytes: &[u8]) -> impl FnMut(ParseInput<'a>) -> ParseResult<&'a [u8]> + '_ {
    move |input: ParseInput<'a>| {
        if input.is_empty() {
            return Err("Empty input");
        } else if input.len() < bytes.len() {
            return Err("Not enough bytes");
        }

        if input.starts_with(bytes) {
            Ok((&input[bytes.len()..], &input[..bytes.len()]))
        } else {
            Err("Wrong bytes")
        }
    }
}

/// Parse a single byte, failing if it does not match `byte`
pub fn byte(byte: u8) -> impl FnMut(ParseInput) -> ParseResult<u8> {
    move |input: ParseInput| {
        if input.is_empty() {
            return Err("Empty input");
        }

        if input.starts_with(&[byte]) {
            Ok((&input[1..], byte))
        } else {
            Err("Wrong byte")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parsers {
        use super::*;

        fn parse_fail(_: ParseInput) -> ParseResult<ParseInput> {
            Err("bad parser")
        }

        fn take_all(input: ParseInput) -> ParseResult<ParseInput> {
            Ok((b"", input))
        }

        mod prefixed {
            use super::*;

            #[test]
            fn should_fail_if_prefix_parser_fails() {
                let _ = prefixed(parse_fail, take_all)(b"abc").unwrap_err();
            }

            #[test]
            fn should_fail_if_main_parser_fails() {
                let _ = prefixed(take(1), parse_fail)(b"abc").unwrap_err();
            }

            #[test]
            fn should_return_value_of_main_parser_when_succeeds() {
                let (s, value) = prefixed(take(1), take(1))(b"abc").unwrap();
                assert_eq!(s, b"c");
                assert_eq!(value, b"b");
            }
        }

        mod maybe {
            use super::*;

            #[test]
            fn should_return_some_value_if_wrapped_parser_succeeds() {
                let (s, value) = maybe(take(2))(b"abc").unwrap();
                assert_eq!(s, b"c");
                assert_eq!(value, Some(b"ab".as_slice()));
            }

            #[test]
            fn should_return_none_if_wrapped_parser_fails() {
                let (s, value) = maybe(parse_fail)(b"abc").unwrap();
                assert_eq!(s, b"abc");
                assert_eq!(value, None);
            }
        }

        mod take_util_byte {
            use super::*;

            #[test]
            fn should_consume_until_predicate_matches() {
                let (s, text) = take_until_byte(|c| c == b'b')(b"abc").unwrap();
                assert_eq!(s, b"bc");
                assert_eq!(text, b"a");
            }

            #[test]
            fn should_consume_completely_if_predicate_never_matches() {
                let (s, text) = take_until_byte(|c| c == b'z')(b"abc").unwrap();
                assert_eq!(s, b"");
                assert_eq!(text, b"abc");
            }

            #[test]
            fn should_fail_if_nothing_consumed() {
                let _ = take_until_byte(|c| c == b'a')(b"abc").unwrap_err();
            }

            #[test]
            fn should_fail_if_input_is_empty() {
                let _ = take_until_byte(|c| c == b'a')(b"").unwrap_err();
            }
        }

        mod take {
            use super::*;

            #[test]
            fn should_consume_cnt_bytes() {
                let (input, bytes) = take(2)(b"abc").unwrap();
                assert_eq!(input, b"c");
                assert_eq!(bytes, b"ab");
            }

            #[test]
            fn should_fail_if_takes_nothing() {
                take(0)(b"abc").unwrap_err();
            }

            #[test]
            fn should_fail_if_not_enough_bytes() {
                take(4)(b"abc").unwrap_err();
            }

            #[test]
            fn should_support_taking_exactly_enough_bytes_as_input() {
                let (input, bytes) = take(3)(b"abc").unwrap();
                assert_eq!(input, b"");
                assert_eq!(bytes, b"abc");
            }
        }

        mod byte {
            use super::*;

            #[test]
            fn should_succeed_if_next_byte_matches() {
                let (s, c) = byte(b'a')(b"abc").unwrap();
                assert_eq!(s, b"bc");
                assert_eq!(c, b'a');
            }

            #[test]
            fn should_fail_if_next_byte_does_not_match() {
                let _ = byte(b'b')(b"abc").unwrap_err();
            }

            #[test]
            fn should_fail_if_input_is_empty() {
                let _ = byte(b'a')(b"").unwrap_err();
            }
        }
    }
}
