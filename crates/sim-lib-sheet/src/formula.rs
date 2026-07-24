//! Exact formula evaluation for local sheets.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};

use num_bigint::BigInt;
use sim_incremental_core::{IncrementalEngine, IncrementalError, QueryFrame, QueryResult};
use sim_kernel::Cx;
use sim_lib_numbers_rational::{Rational, parse_rational_parts};

use crate::model::{CellRef, CellValue, Sheet, SheetError, rational_from_parts, rational_parts};

type FormulaValue = Result<ExactNumber, FormulaFailure>;

/// Evaluates a local sheet formula.
pub fn eval_formula(cx: &mut Cx, sheet: &Sheet, formula: &str) -> Result<CellValue, SheetError> {
    let mut engine = SheetFormulaEngine::from_sheet(cx, sheet)?;
    engine.eval_formula(cx, formula)
}

/// Persistent exact formula evaluator backed by incremental queries.
pub struct SheetFormulaEngine {
    source: SharedSource,
    engine: IncrementalEngine<FormulaKey, FormulaValue>,
    registered_cells: BTreeSet<CellRef>,
    registered_formulas: BTreeSet<String>,
}

impl SheetFormulaEngine {
    /// Creates an empty formula engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            source: Arc::new(RwLock::new(BTreeMap::new())),
            engine: IncrementalEngine::new(),
            registered_cells: BTreeSet::new(),
            registered_formulas: BTreeSet::new(),
        }
    }

    /// Creates a formula engine initialized from a sheet snapshot.
    pub fn from_sheet(cx: &mut Cx, sheet: &Sheet) -> Result<Self, SheetError> {
        let mut engine = Self::new();
        engine.sync_sheet(cx, sheet)?;
        Ok(engine)
    }

    /// Replaces the engine source snapshot with the current sheet cells.
    pub fn sync_sheet(&mut self, cx: &mut Cx, sheet: &Sheet) -> Result<(), SheetError> {
        let mut next = BTreeMap::new();
        for (cell, value) in &sheet.cells {
            if let Some(value) = EngineCellValue::from_cell_value(cx, value)? {
                next.insert(cell.clone(), value);
            }
        }
        self.replace_source(next)
    }

    /// Updates one cell and invalidates query dependents when its value changes.
    pub fn set_cell(
        &mut self,
        cx: &mut Cx,
        cell: CellRef,
        value: CellValue,
    ) -> Result<(), SheetError> {
        self.ensure_cell_query(cell.clone());
        let next = EngineCellValue::from_cell_value(cx, &value)?;
        let changed = {
            let mut source = self.source.write().map_err(source_lock_error)?;
            match next {
                Some(value) => {
                    if source.get(&cell) == Some(&value) {
                        false
                    } else {
                        source.insert(cell.clone(), value);
                        true
                    }
                }
                None => source.remove(&cell).is_some(),
            }
        };
        if changed {
            self.engine.invalidate(&FormulaKey::Cell(cell));
        }
        Ok(())
    }

    /// Evaluates a formula against the current source snapshot.
    pub fn eval_formula(&mut self, cx: &mut Cx, formula: &str) -> Result<CellValue, SheetError> {
        self.ensure_formula_query(formula);
        let value = self
            .engine
            .verify(FormulaKey::Formula(formula.to_owned()))
            .map_err(incremental_error_to_sheet_error)?
            .map_err(FormulaFailure::into_sheet_error)?;
        rational_from_parts(cx, value.num, value.den).map(CellValue::Number)
    }

    #[cfg(test)]
    pub(crate) fn cell_memo_revision(
        &self,
        cell: &CellRef,
    ) -> Option<sim_incremental_core::Revision> {
        self.engine.memo_revision(&FormulaKey::Cell(cell.clone()))
    }

    fn replace_source(
        &mut self,
        next: BTreeMap<CellRef, EngineCellValue>,
    ) -> Result<(), SheetError> {
        let current = self.source.read().map_err(source_lock_error)?.clone();
        let changed = current
            .keys()
            .chain(next.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|cell| current.get(cell) != next.get(cell))
            .collect::<Vec<_>>();

        for cell in current.keys().chain(next.keys()).cloned() {
            self.ensure_cell_query(cell);
        }

        *self.source.write().map_err(source_lock_error)? = next;
        for cell in changed {
            self.engine.invalidate(&FormulaKey::Cell(cell));
        }
        Ok(())
    }

    fn ensure_cell_query(&mut self, cell: CellRef) {
        if !self.registered_cells.insert(cell.clone()) {
            return;
        }
        let source = Arc::clone(&self.source);
        self.engine
            .register_fn(FormulaKey::Cell(cell), move |key, frame| {
                eval_cell_query(&source, key, frame)
            });
    }

    fn ensure_formula_query(&mut self, formula: &str) {
        if !self.registered_formulas.insert(formula.to_owned()) {
            return;
        }
        self.engine
            .register_fn(FormulaKey::Formula(formula.to_owned()), |key, frame| {
                let FormulaKey::Formula(formula) = key else {
                    return Ok(Err(FormulaFailure::Formula(
                        "formula query received a cell key".to_owned(),
                    )));
                };
                let mut reader = IncrementalCellReader { frame };
                formula_eval_to_query_result(evaluate_formula_source(formula, &mut reader))
            });
    }
}

impl Default for SheetFormulaEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FormulaKey {
    Cell(CellRef),
    Formula(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EngineCellValue {
    Number(ExactNumber),
    Formula(String),
    Text(String),
    Bool(bool),
}

impl EngineCellValue {
    fn from_cell_value(cx: &mut Cx, value: &CellValue) -> Result<Option<Self>, SheetError> {
        match value {
            CellValue::Blank => Ok(None),
            CellValue::Number(value) => ExactNumber::from_rational(cx, value)
                .map(Self::Number)
                .map(Some),
            CellValue::Formula(value) => Ok(Some(Self::Formula(value.clone()))),
            CellValue::Text(value) => Ok(Some(Self::Text(value.clone()))),
            CellValue::Bool(value) => Ok(Some(Self::Bool(*value))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ExactNumber {
    num: BigInt,
    den: BigInt,
}

impl ExactNumber {
    fn parse(text: &str) -> Result<Self, FormulaFailure> {
        let source = text.trim();
        let literal = if source.contains('/') {
            source.to_owned()
        } else {
            format!("{source}/1")
        };
        let Some((num, den)) = parse_rational_parts(&literal) else {
            return Err(FormulaFailure::InvalidRational(source.to_owned()));
        };
        Ok(Self { num, den })
    }

    fn from_rational(cx: &mut Cx, value: &Rational) -> Result<Self, SheetError> {
        let (num, den) = rational_parts(cx, value)?;
        Self::new(num, den).map_err(FormulaFailure::into_sheet_error)
    }

    fn new(num: BigInt, den: BigInt) -> Result<Self, FormulaFailure> {
        let literal = format!("{num}/{den}");
        let Some((num, den)) = parse_rational_parts(&literal) else {
            return Err(FormulaFailure::InvalidRational(literal));
        };
        Ok(Self { num, den })
    }

    fn zero() -> Self {
        Self {
            num: BigInt::from(0_u8),
            den: BigInt::from(1_u8),
        }
    }

    fn add(&self, right: &Self) -> Result<Self, FormulaFailure> {
        Self::new(
            (&self.num * &right.den) + (&right.num * &self.den),
            &self.den * &right.den,
        )
    }

    fn sub(&self, right: &Self) -> Result<Self, FormulaFailure> {
        Self::new(
            (&self.num * &right.den) - (&right.num * &self.den),
            &self.den * &right.den,
        )
    }

    fn mul(&self, right: &Self) -> Result<Self, FormulaFailure> {
        Self::new(&self.num * &right.num, &self.den * &right.den)
    }

    fn div(&self, right: &Self) -> Result<Self, FormulaFailure> {
        if right.num == BigInt::from(0_u8) {
            return Err(FormulaFailure::DivisionByZero);
        }
        Self::new(&self.num * &right.den, &self.den * &right.num)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum FormulaFailure {
    InvalidCellRef(String),
    InvalidRational(String),
    Formula(String),
    NonNumericCell(CellRef),
    DivisionByZero,
}

impl FormulaFailure {
    fn into_sheet_error(self) -> SheetError {
        match self {
            Self::InvalidCellRef(message) => SheetError::InvalidCellRef(message),
            Self::InvalidRational(message) => SheetError::InvalidRational(message),
            Self::Formula(message) => SheetError::Formula(message),
            Self::NonNumericCell(cell) => SheetError::NonNumericCell(cell),
            Self::DivisionByZero => SheetError::DivisionByZero,
        }
    }
}

enum FormulaEvalError {
    Domain(FormulaFailure),
    Incremental(IncrementalError<FormulaKey>),
}

impl From<FormulaFailure> for FormulaEvalError {
    fn from(error: FormulaFailure) -> Self {
        Self::Domain(error)
    }
}

impl From<IncrementalError<FormulaKey>> for FormulaEvalError {
    fn from(error: IncrementalError<FormulaKey>) -> Self {
        Self::Incremental(error)
    }
}

type FormulaEvalResult<T> = Result<T, FormulaEvalError>;
type SharedSource = Arc<RwLock<BTreeMap<CellRef, EngineCellValue>>>;

fn eval_cell_query(
    source: &SharedSource,
    key: &FormulaKey,
    frame: &mut QueryFrame<'_, FormulaKey, FormulaValue>,
) -> QueryResult<FormulaKey, FormulaValue> {
    let FormulaKey::Cell(cell) = key else {
        return Ok(Err(FormulaFailure::Formula(
            "cell query received a formula key".to_owned(),
        )));
    };
    frame.observe_epoch(key.clone())?;
    let value = match source.read() {
        Ok(source) => source.get(cell).cloned(),
        Err(_) => {
            return Ok(Err(FormulaFailure::Formula(
                "sheet formula source lock poisoned".to_owned(),
            )));
        }
    };
    match value {
        Some(EngineCellValue::Number(value)) => Ok(Ok(value)),
        Some(EngineCellValue::Formula(formula)) => {
            let mut reader = IncrementalCellReader { frame };
            formula_eval_to_query_result(evaluate_formula_source(&formula, &mut reader))
        }
        Some(EngineCellValue::Text(_) | EngineCellValue::Bool(_)) | None => {
            Ok(Err(FormulaFailure::NonNumericCell(cell.clone())))
        }
    }
}

fn formula_eval_to_query_result(
    result: FormulaEvalResult<ExactNumber>,
) -> QueryResult<FormulaKey, FormulaValue> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(FormulaEvalError::Domain(error)) => Ok(Err(error)),
        Err(FormulaEvalError::Incremental(error)) => Err(error),
    }
}

fn incremental_error_to_sheet_error(error: IncrementalError<FormulaKey>) -> SheetError {
    match error {
        IncrementalError::Cycle { path } => path
            .into_iter()
            .rev()
            .find_map(|key| match key {
                FormulaKey::Cell(cell) => Some(SheetError::FormulaCycle(cell)),
                FormulaKey::Formula(_) => None,
            })
            .unwrap_or_else(|| SheetError::Formula("formula cycle".to_owned())),
        IncrementalError::UnknownQuery {
            key: FormulaKey::Cell(cell),
        } => SheetError::NonNumericCell(cell),
        error => SheetError::Formula(error.to_string()),
    }
}

fn source_lock_error<T>(_: T) -> SheetError {
    SheetError::Formula("sheet formula source lock poisoned".to_owned())
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

fn tokenize(source: &str) -> Result<Vec<Token>, FormulaFailure> {
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
                tokens.push(Token::Cell(CellRef::parse(&cell).map_err(
                    |error| match error {
                        SheetError::InvalidCellRef(message) => {
                            FormulaFailure::InvalidCellRef(message)
                        }
                        error => FormulaFailure::Formula(error.to_string()),
                    },
                )?));
            }
            ch => {
                return Err(FormulaFailure::Formula(format!(
                    "unexpected character {ch:?}"
                )));
            }
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

trait CellReader {
    fn read_cell(&mut self, cell: CellRef) -> FormulaEvalResult<ExactNumber>;
}

struct IncrementalCellReader<'a, 'frame> {
    frame: &'a mut QueryFrame<'frame, FormulaKey, FormulaValue>,
}

impl CellReader for IncrementalCellReader<'_, '_> {
    fn read_cell(&mut self, cell: CellRef) -> FormulaEvalResult<ExactNumber> {
        match self.frame.read(FormulaKey::Cell(cell.clone())) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(FormulaEvalError::Domain(error)),
            Err(IncrementalError::UnknownQuery {
                key: FormulaKey::Cell(_),
            }) => {
                self.frame.observe_missing(FormulaKey::Cell(cell.clone()))?;
                Err(FormulaEvalError::Domain(FormulaFailure::NonNumericCell(
                    cell,
                )))
            }
            Err(error) => Err(FormulaEvalError::Incremental(error)),
        }
    }
}

fn evaluate_formula_source<R: CellReader>(
    formula: &str,
    reader: &mut R,
) -> FormulaEvalResult<ExactNumber> {
    let source = formula
        .trim()
        .strip_prefix('=')
        .ok_or_else(|| FormulaFailure::Formula("formula must start with =".to_owned()))?;
    let tokens = tokenize(source)?;
    let mut parser = Parser {
        reader,
        tokens,
        pos: 0,
    };
    let value = parser.expression()?;
    parser.expect_end()?;
    Ok(value)
}

struct Parser<'a, R> {
    reader: &'a mut R,
    tokens: Vec<Token>,
    pos: usize,
}

impl<R: CellReader> Parser<'_, R> {
    fn expression(&mut self) -> FormulaEvalResult<ExactNumber> {
        let mut left = self.term()?;
        loop {
            match self.peek() {
                Token::Plus => {
                    self.bump();
                    let right = self.term()?;
                    left = left.add(&right)?;
                }
                Token::Minus => {
                    self.bump();
                    let right = self.term()?;
                    left = left.sub(&right)?;
                }
                _ => return Ok(left),
            }
        }
    }

    fn term(&mut self) -> FormulaEvalResult<ExactNumber> {
        let mut left = self.factor()?;
        loop {
            match self.peek() {
                Token::Star => {
                    self.bump();
                    let right = self.factor()?;
                    left = left.mul(&right)?;
                }
                Token::Slash => {
                    self.bump();
                    let right = self.factor()?;
                    left = left.div(&right)?;
                }
                _ => return Ok(left),
            }
        }
    }

    fn factor(&mut self) -> FormulaEvalResult<ExactNumber> {
        match self.peek().clone() {
            Token::Number(text) => {
                self.bump();
                ExactNumber::parse(&text).map_err(Into::into)
            }
            Token::Cell(cell) => {
                self.bump();
                self.reader.read_cell(cell)
            }
            Token::Minus => {
                self.bump();
                let value = self.factor()?;
                ExactNumber::zero().sub(&value).map_err(Into::into)
            }
            Token::LParen => {
                self.bump();
                let value = self.expression()?;
                match self.peek() {
                    Token::RParen => {
                        self.bump();
                        Ok(value)
                    }
                    other => {
                        Err(FormulaFailure::Formula(format!("expected ), got {other:?}")).into())
                    }
                }
            }
            other => Err(FormulaFailure::Formula(format!(
                "expected number, cell, or group, got {other:?}"
            ))
            .into()),
        }
    }

    fn expect_end(&self) -> FormulaEvalResult<()> {
        match self.peek() {
            Token::End => Ok(()),
            other => {
                Err(FormulaFailure::Formula(format!("unexpected trailing token {other:?}")).into())
            }
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) {
        self.pos += 1;
    }
}
