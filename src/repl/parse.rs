#[derive(Debug)]
struct TokenIter<'a> {
    tokens: std::str::Split<'a, char>
}

impl<'a> TokenIter<'a> {
    fn new(line: &'a str) -> Self {
        Self { tokens: line.split(' ') }
    }
}

impl<'a> Iterator for TokenIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.tokens.next()
    }
}

fn tokenize(line: &str) -> TokenIter {
    TokenIter::new(line)
}

#[cfg(test)]
mod tests {
    use super::{*};

    #[test]
    fn test_parse_query() {
        let input = ".foo arg1 arg2";
        let expected = vec![".foo", "arg1", "arg2"];
        let actual  = tokenize(input);

        assert!(expected.into_iter().eq(actual));
        //assert!(actual.eq(expected));
    }
}
