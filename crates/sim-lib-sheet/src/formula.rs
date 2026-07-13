//! Exact formula evaluation for local sheets.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use sim_kernel::Cx;
use sim_lib_numbers_rational::Rational;

use crate::model::{
    CellRef, CellValue, Sheet, SheetError, rational_from_parts, rational_from_str, rational_parts,
};

/// Evaluates a local sheet formula.
pub fn eval_formula(cx: &mut Cx, sheet: &Sheet, formula: &str) -> Result<CellValue, SheetError> {
    let mut visiting = BTreeSet::new();
    eval_formula_with_visiting(cx, sheet, formula, &mut visiting).map(CellValue::Number)
}

fn eval_formula_with_visiting(
    cx: &mut Cx,
    sheet: &Sheet,
    formula: &str,
    visiting: &mut BTreeSet<CellRef>,
) -> Result<Rational, SheetError> {
    let source = formula
        .trim()
        .strip_prefix('=')
        .ok_or_else(|| SheetError::Formula("formula must start with =".to_owned()))?;
    let tokens = tokenize(source)?;
    let mut parser = Parser {
        cx,
        sheet,
        visiting,
        tokens,
        pos: 0,
    };
    let value = parser.expression()?;
    parser.expect_end()?;
    Ok(value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Number(String),
    Cell(CellRef),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    End,
}

fn tokenize(source: &str) -> Result<Vec<Token>, SheetError> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < chars.len() {
        match chars[pos] {
            ch if ch.is_ascii_whitespace() => pos += 1,
            '+' => {
                tokens.push(Token::Plus);
                pos += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                pos += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                pos += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                pos += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                pos += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                pos += 1;
            }
            ch if ch.is_ascii_digit() => {
                let start = pos;
                while pos < chars.len() && chars[pos].is_ascii_digit() {
                    pos += 1;
                }
                if pos < chars.len()
                    && chars[pos] == '/'
                    && pos + 1 < chars.len()
                    && chars[pos + 1].is_ascii_digit()
                {
                    pos += 1;
                    while pos < chars.len() && chars[pos].is_ascii_digit() {
                        pos += 1;
                    }
                }
                tokens.push(Token::Number(chars[start..pos].iter().collect()));
            }
            ch if ch.is_ascii_alphabetic() => {
                let start = pos;
                while pos < chars.len() && chars[pos].is_ascii_alphabetic() {
                    pos += 1;
                }
                while pos < chars.len() && chars[pos].is_ascii_digit() {
                    pos += 1;
                }
                let cell = chars[start..pos].iter().collect::<String>();
                tokens.push(Token::Cell(CellRef::parse(&cell)?));
            }
            ch => {
                return Err(SheetError::Formula(format!("unexpected character {ch:?}")));
            }
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

struct Parser<'a, 'cx> {
    cx: &'cx mut Cx,
    sheet: &'a Sheet,
    visiting: &'a mut BTreeSet<CellRef>,
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser<'_, '_> {
    fn expression(&mut self) -> Result<Rational, SheetError> {
        let mut left = self.term()?;
        loop {
            match self.peek() {
                Token::Plus => {
                    self.bump();
                    let right = self.term()?;
                    left = self.add(&left, &right)?;
                }
                Token::Minus => {
                    self.bump();
                    let right = self.term()?;
                    left = self.sub(&left, &right)?;
                }
                _ => return Ok(left),
            }
        }
    }

    fn term(&mut self) -> Result<Rational, SheetError> {
        let mut left = self.factor()?;
        loop {
            match self.peek() {
                Token::Star => {
                    self.bump();
                    let right = self.factor()?;
                    left = self.mul(&left, &right)?;
                }
                Token::Slash => {
                    self.bump();
                    let right = self.factor()?;
                    left = self.div(&left, &right)?;
                }
                _ => return Ok(left),
            }
        }
    }

    fn factor(&mut self) -> Result<Rational, SheetError> {
        match self.peek().clone() {
            Token::Number(text) => {
                self.bump();
                rational_from_str(self.cx, &text)
            }
            Token::Cell(cell) => {
                self.bump();
                self.eval_cell(cell)
            }
            Token::Minus => {
                self.bump();
                let value = self.factor()?;
                let zero = rational_from_str(self.cx, "0")?;
                self.sub(&zero, &value)
            }
            Token::LParen => {
                self.bump();
                let value = self.expression()?;
                match self.peek() {
                    Token::RParen => {
                        self.bump();
                        Ok(value)
                    }
                    other => Err(SheetError::Formula(format!("expected ), got {other:?}"))),
                }
            }
            other => Err(SheetError::Formula(format!(
                "expected number, cell, or group, got {other:?}"
            ))),
        }
    }

    fn eval_cell(&mut self, cell: CellRef) -> Result<Rational, SheetError> {
        if self.visiting.contains(&cell) {
            return Err(SheetError::FormulaCycle(cell));
        }
        match self.sheet.cells.get(&cell) {
            Some(CellValue::Number(value)) => Ok(value.clone()),
            Some(CellValue::Formula(formula)) => {
                self.visiting.insert(cell.clone());
                let value = eval_formula_with_visiting(self.cx, self.sheet, formula, self.visiting);
                self.visiting.remove(&cell);
                value
            }
            _ => Err(SheetError::NonNumericCell(cell)),
        }
    }

    fn add(&mut self, left: &Rational, right: &Rational) -> Result<Rational, SheetError> {
        let (ln, ld) = rational_parts(self.cx, left)?;
        let (rn, rd) = rational_parts(self.cx, right)?;
        rational_from_parts(self.cx, (&ln * &rd) + (&rn * &ld), ld * rd)
    }

    fn sub(&mut self, left: &Rational, right: &Rational) -> Result<Rational, SheetError> {
        let (ln, ld) = rational_parts(self.cx, left)?;
        let (rn, rd) = rational_parts(self.cx, right)?;
        rational_from_parts(self.cx, (&ln * &rd) - (&rn * &ld), ld * rd)
    }

    fn mul(&mut self, left: &Rational, right: &Rational) -> Result<Rational, SheetError> {
        let (ln, ld) = rational_parts(self.cx, left)?;
        let (rn, rd) = rational_parts(self.cx, right)?;
        rational_from_parts(self.cx, ln * rn, ld * rd)
    }

    fn div(&mut self, left: &Rational, right: &Rational) -> Result<Rational, SheetError> {
        let (ln, ld) = rational_parts(self.cx, left)?;
        let (rn, rd) = rational_parts(self.cx, right)?;
        if rn == BigInt::from(0_u8) {
            return Err(SheetError::DivisionByZero);
        }
        rational_from_parts(self.cx, ln * rd, ld * rn)
    }

    fn expect_end(&self) -> Result<(), SheetError> {
        match self.peek() {
            Token::End => Ok(()),
            other => Err(SheetError::Formula(format!(
                "unexpected trailing token {other:?}"
            ))),
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) {
        self.pos += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};

    use crate::{CellRef, CellValue, Sheet, rational_from_str, rational_to_canonical};

    use super::*;

    fn cx() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn addition_keeps_exact_rationals() {
        let mut cx = cx();
        let mut sheet = Sheet::new("Sheet1");
        sheet.set_cell(
            CellRef::parse("A1").unwrap(),
            CellValue::Number(rational_from_str(&mut cx, "1").unwrap()),
        );
        sheet.set_cell(
            CellRef::parse("B1").unwrap(),
            CellValue::Number(rational_from_str(&mut cx, "5/2").unwrap()),
        );

        let CellValue::Number(value) = eval_formula(&mut cx, &sheet, "=A1+B1").unwrap() else {
            panic!("expected number");
        };

        assert_eq!(rational_to_canonical(&mut cx, &value).unwrap(), "7/2");
    }

    #[test]
    fn formula_cycle_fails_closed() {
        let mut cx = cx();
        let mut sheet = Sheet::new("Sheet1");
        sheet.set_cell(
            CellRef::parse("A1").unwrap(),
            CellValue::Formula("=A1".to_owned()),
        );

        let err = eval_formula(&mut cx, &sheet, "=A1").unwrap_err();

        assert!(matches!(err, SheetError::FormulaCycle(_)));
    }
}
