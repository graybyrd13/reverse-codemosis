//! The fact language.
//!
//! Rules state the facts that must hold before a correction is safe, as small
//! expressions over the captures of a shape:
//!
//! ```text
//! !py.bound(text(@name)) && in_table('stdlib_modules', text(@name))
//! kind(@right) in ['integer', 'string']
//! ```
//!
//! Grammar (lowest precedence first):
//!
//! ```text
//! expr    := and ( ('||' | 'or') and )*
//! and     := unary ( ('&&' | 'and') unary )*
//! unary   := ('!' | 'not') unary | compare
//! compare := primary ( op primary )?      op := == != < <= > >= in 'not in'
//! primary := call | '@'name | string | number | true | false | list | '(' expr ')'
//! call    := name '(' ( expr ( ',' expr )* )? ')'
//! list    := '[' ( expr ( ',' expr )* )? ']'
//! ```
//!
//! The language has no assignment, no loops and no side effects. Every
//! function it can call is listed in the fact vocabulary (`codemosis facts`).

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Bool(bool),
    Num(i64),
    Str(String),
    Capture(String),
    List(Vec<Expr>),
    Call(String, Vec<Expr>),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cmp(Box<Expr>, CmpOp, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at character {})", self.message, self.offset + 1)
    }
}

impl std::error::Error for ExprError {}

impl Expr {
    /// Every capture name the expression refers to.
    pub fn captures(&self, out: &mut Vec<String>) {
        match self {
            Expr::Capture(c) => out.push(c.clone()),
            Expr::List(items) | Expr::Call(_, items) => {
                items.iter().for_each(|e| e.captures(out));
            }
            Expr::Not(e) => e.captures(out),
            Expr::And(a, b) | Expr::Or(a, b) | Expr::Cmp(a, _, b) => {
                a.captures(out);
                b.captures(out);
            }
            Expr::Bool(_) | Expr::Num(_) | Expr::Str(_) => {}
        }
    }

    /// Every function name the expression calls.
    pub fn calls(&self, out: &mut Vec<String>) {
        match self {
            Expr::Call(name, args) => {
                out.push(name.clone());
                args.iter().for_each(|e| e.calls(out));
            }
            Expr::List(items) => items.iter().for_each(|e| e.calls(out)),
            Expr::Not(e) => e.calls(out),
            Expr::And(a, b) | Expr::Or(a, b) | Expr::Cmp(a, _, b) => {
                a.calls(out);
                b.calls(out);
            }
            Expr::Bool(_) | Expr::Num(_) | Expr::Str(_) | Expr::Capture(_) => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Capture(String),
    Str(String),
    Num(i64),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Bang,
    AndAnd,
    OrOr,
    Op(CmpOp),
}

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut i = 0;
    let mut out = Vec::new();
    let err = |m: &str, at: usize| ExprError {
        message: m.to_string(),
        offset: at,
    };
    while i < chars.len() {
        let (at, c) = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '(' => {
                out.push((Tok::LParen, at));
                i += 1
            }
            ')' => {
                out.push((Tok::RParen, at));
                i += 1
            }
            '[' => {
                out.push((Tok::LBracket, at));
                i += 1
            }
            ']' => {
                out.push((Tok::RBracket, at));
                i += 1
            }
            ',' => {
                out.push((Tok::Comma, at));
                i += 1
            }
            '!' => {
                if chars.get(i + 1).map(|x| x.1) == Some('=') {
                    out.push((Tok::Op(CmpOp::Ne), at));
                    i += 2;
                } else {
                    out.push((Tok::Bang, at));
                    i += 1;
                }
            }
            '=' => {
                if chars.get(i + 1).map(|x| x.1) == Some('=') {
                    out.push((Tok::Op(CmpOp::Eq), at));
                    i += 2;
                } else {
                    return Err(err("expected '==' (the fact language has no assignment)", at));
                }
            }
            '<' | '>' => {
                let eq = chars.get(i + 1).map(|x| x.1) == Some('=');
                let op = match (c, eq) {
                    ('<', false) => CmpOp::Lt,
                    ('<', true) => CmpOp::Le,
                    ('>', false) => CmpOp::Gt,
                    _ => CmpOp::Ge,
                };
                out.push((Tok::Op(op), at));
                i += if eq { 2 } else { 1 };
            }
            '&' => {
                if chars.get(i + 1).map(|x| x.1) == Some('&') {
                    out.push((Tok::AndAnd, at));
                    i += 2;
                } else {
                    return Err(err("expected '&&'", at));
                }
            }
            '|' => {
                if chars.get(i + 1).map(|x| x.1) == Some('|') {
                    out.push((Tok::OrOr, at));
                    i += 2;
                } else {
                    return Err(err("expected '||'", at));
                }
            }
            '\'' | '"' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(err("unterminated string", at)),
                        Some(&(_, ch)) if ch == quote => {
                            i += 1;
                            break;
                        }
                        Some(&(_, '\\')) => {
                            match chars.get(i + 1) {
                                Some(&(_, 'n')) => s.push('\n'),
                                Some(&(_, 't')) => s.push('\t'),
                                Some(&(_, e)) => s.push(e),
                                None => return Err(err("unterminated string", at)),
                            }
                            i += 2;
                        }
                        Some(&(_, ch)) => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                out.push((Tok::Str(s), at));
            }
            '@' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && is_ident_char(chars[j].1) {
                    j += 1;
                }
                if j == start {
                    return Err(err("expected a capture name after '@'", at));
                }
                let name: String = chars[start..j].iter().map(|x| x.1).collect();
                out.push((Tok::Capture(name), at));
                i = j;
            }
            c if c.is_ascii_digit() || (c == '-' && next_is_digit(&chars, i)) => {
                let mut j = i + 1;
                while j < chars.len() && chars[j].1.is_ascii_digit() {
                    j += 1;
                }
                let text: String = chars[i..j].iter().map(|x| x.1).collect();
                let n = text.parse::<i64>().map_err(|_| err("number out of range", at))?;
                out.push((Tok::Num(n), at));
                i = j;
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut j = i;
                while j < chars.len() && (is_ident_char(chars[j].1) || chars[j].1 == '.') {
                    j += 1;
                }
                let word: String = chars[i..j].iter().map(|x| x.1).collect();
                match word.as_str() {
                    "and" => out.push((Tok::AndAnd, at)),
                    "or" => out.push((Tok::OrOr, at)),
                    "in" => out.push((Tok::Op(CmpOp::In), at)),
                    "not" => {
                        // `not in` is a comparison; a lone `not` is negation.
                        let mut k = j;
                        while k < chars.len() && chars[k].1.is_whitespace() {
                            k += 1;
                        }
                        let next: String = chars[k..chars.len().min(k + 2)].iter().map(|x| x.1).collect();
                        let after = chars.get(k + 2).map(|x| x.1);
                        if next == "in" && !after.is_some_and(is_ident_char) {
                            out.push((Tok::Op(CmpOp::NotIn), at));
                            j = k + 2;
                        } else {
                            out.push((Tok::Bang, at));
                        }
                    }
                    _ => out.push((Tok::Ident(word), at)),
                }
                i = j;
            }
            other => return Err(err(&format!("unexpected character '{other}'"), at)),
        }
    }
    Ok(out)
}

fn next_is_digit(chars: &[(usize, char)], i: usize) -> bool {
    chars.get(i + 1).is_some_and(|x| x.1.is_ascii_digit())
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    len: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn offset(&self) -> usize {
        self.toks.get(self.pos).map(|t| t.1).unwrap_or(self.len)
    }

    fn err<T>(&self, m: &str) -> Result<T, ExprError> {
        Err(ExprError {
            message: m.to_string(),
            offset: self.offset(),
        })
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.and()?;
        while self.eat(&Tok::OrOr) {
            let rhs = self.and()?;
            lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn and(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.unary()?;
        while self.eat(&Tok::AndAnd) {
            let rhs = self.unary()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        if self.eat(&Tok::Bang) {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.compare()
    }

    fn compare(&mut self) -> Result<Expr, ExprError> {
        let lhs = self.primary()?;
        if let Some(Tok::Op(op)) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.primary()?;
            return Ok(Expr::Cmp(Box::new(lhs), op, Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn list_items(&mut self, close: &Tok) -> Result<Vec<Expr>, ExprError> {
        let mut items = Vec::new();
        if self.eat(close) {
            return Ok(items);
        }
        loop {
            items.push(self.expr()?);
            if self.eat(close) {
                return Ok(items);
            }
            if !self.eat(&Tok::Comma) {
                return self.err("expected ',' or a closing bracket");
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, ExprError> {
        let Some(tok) = self.peek().cloned() else {
            return self.err("unexpected end of expression");
        };
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Capture(c) => Ok(Expr::Capture(c)),
            Tok::LParen => {
                let e = self.expr()?;
                if !self.eat(&Tok::RParen) {
                    return self.err("expected ')'");
                }
                Ok(e)
            }
            Tok::LBracket => Ok(Expr::List(self.list_items(&Tok::RBracket)?)),
            Tok::Ident(name) => match name.as_str() {
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                _ => {
                    if !self.eat(&Tok::LParen) {
                        self.pos -= 1;
                        return self.err(&format!(
                            "'{name}' is not a value; strings need quotes and functions need ()"
                        ));
                    }
                    Ok(Expr::Call(name, self.list_items(&Tok::RParen)?))
                }
            },
            _ => {
                self.pos -= 1;
                self.err("expected a value")
            }
        }
    }
}

/// Parse a fact expression.
pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let toks = tokenize(src)?;
    let mut p = Parser {
        toks,
        pos: 0,
        len: src.len(),
    };
    let e = p.expr()?;
    if p.pos != p.toks.len() {
        return p.err("unexpected input after the end of the expression");
    }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_calls_and_operators() {
        let e = parse("!py.bound(text(@name)) && kind(@x) in ['a', \"b\"]").unwrap();
        match e {
            Expr::And(a, b) => {
                assert!(matches!(*a, Expr::Not(_)));
                assert!(matches!(*b, Expr::Cmp(_, CmpOp::In, _)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn not_in_and_words() {
        let e = parse("text(@op) not in ['is'] and not exists(@x) or false").unwrap();
        assert!(matches!(e, Expr::Or(_, _)));
        let e = parse("not inside(@x, 'lambda')").unwrap();
        assert!(matches!(e, Expr::Not(_)));
    }

    #[test]
    fn collects_captures_and_calls() {
        let e = parse("count(@a) == field_count(@b, 'operators')").unwrap();
        let mut caps = vec![];
        e.captures(&mut caps);
        assert_eq!(caps, vec!["a", "b"]);
        let mut calls = vec![];
        e.calls(&mut calls);
        assert_eq!(calls, vec!["count", "field_count"]);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse("text(@a) = 'x'").is_err());
        assert!(parse("bare_word").is_err());
        assert!(parse("f(@a").is_err());
        assert!(parse("1 2").is_err());
        assert!(parse("-3 < 2").is_ok());
    }
}
