//! Deck records and expression projection.

use std::fmt;

use sim_kernel::{Expr, Symbol};
use sim_lib_doc_core::ExternalRef;

/// Office document kind used for presentation deck documents.
pub const DECK_DOC_KIND: &str = "deck";

const FIELD_BACKEND: &str = "backend";
const FIELD_BLOCKS: &str = "blocks";
const FIELD_COLUMNS: &str = "columns";
const FIELD_EXTERNAL_ID: &str = "external-id";
const FIELD_ID: &str = "id";
const FIELD_ITEMS: &str = "items";
const FIELD_KIND: &str = "kind";
const FIELD_ROWS: &str = "rows";
const FIELD_SLIDES: &str = "slides";
const FIELD_TITLE: &str = "title";
const FIELD_VALUE: &str = "value";
const FIELD_VERSION: &str = "version";
const FIELD_WEB_URL: &str = "web-url";

const BLOCK_BULLET_LIST: &str = "deck-bullet-list";
const BLOCK_HEADING: &str = "deck-heading";
const BLOCK_IMAGE_REF: &str = "deck-image-ref";
const BLOCK_TABLE: &str = "deck-table";
const KIND_SLIDE: &str = "deck-slide";

/// In-memory presentation deck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deck {
    /// Deck title.
    pub title: String,
    /// Ordered slides.
    pub slides: Vec<Slide>,
}

impl Deck {
    /// Builds an empty deck.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            slides: Vec::new(),
        }
    }

    /// Appends one slide.
    pub fn push_slide(&mut self, slide: Slide) {
        self.slides.push(slide);
    }
}

/// One presentation slide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    /// Stable slide id.
    pub id: String,
    /// Slide title.
    pub title: String,
    /// Ordered content blocks.
    pub blocks: Vec<SlideBlock>,
}

impl Slide {
    /// Builds an empty slide.
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            blocks: Vec::new(),
        }
    }

    /// Appends one content block.
    pub fn push_block(&mut self, block: SlideBlock) {
        self.blocks.push(block);
    }
}

/// Portable deck content block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlideBlock {
    /// Large slide heading.
    Heading(String),
    /// Bulleted text list.
    BulletList(Vec<String>),
    /// Simple table with column labels and row values.
    Table {
        /// Column labels.
        columns: Vec<String>,
        /// Row values.
        rows: Vec<Vec<String>>,
    },
    /// Reference to an external image asset.
    ImageRef(ExternalRef),
}

/// Presentation-domain failure.
#[derive(Debug)]
pub enum DeckError {
    /// A kernel operation failed.
    Kernel(String),
    /// The document kind was not `deck`.
    WrongDocKind(String),
    /// The document body did not have the deck expression shape.
    WrongDocBody(String),
    /// A deck record was invalid.
    InvalidDeck(String),
}

impl fmt::Display for DeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel(message) => write!(f, "kernel error: {message}"),
            Self::WrongDocKind(kind) => write!(f, "expected deck document, got {kind}"),
            Self::WrongDocBody(message) => write!(f, "invalid deck document body: {message}"),
            Self::InvalidDeck(message) => write!(f, "invalid deck: {message}"),
        }
    }
}

impl std::error::Error for DeckError {}

impl From<sim_kernel::Error> for DeckError {
    fn from(error: sim_kernel::Error) -> Self {
        Self::Kernel(error.to_string())
    }
}

pub(crate) fn deck_to_expr(deck: &Deck) -> Result<Expr, DeckError> {
    if deck.title.trim().is_empty() {
        return Err(DeckError::InvalidDeck("deck title is empty".to_owned()));
    }
    let slides = deck
        .slides
        .iter()
        .map(slide_to_expr)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(map(vec![
        entry(FIELD_KIND, Expr::Symbol(office_symbol(DECK_DOC_KIND))),
        entry(FIELD_TITLE, Expr::String(deck.title.clone())),
        entry(FIELD_SLIDES, Expr::List(slides)),
    ]))
}

pub(crate) fn deck_from_expr(expr: &Expr) -> Result<Deck, DeckError> {
    let entries = expect_map(expr)?;
    expect_kind(entries, DECK_DOC_KIND)?;
    let title = expect_string(value_for(entries, FIELD_TITLE)?, FIELD_TITLE)?.to_owned();
    let slide_exprs = expect_list(value_for(entries, FIELD_SLIDES)?, FIELD_SLIDES)?;
    let slides = slide_exprs
        .iter()
        .map(slide_from_expr)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Deck { title, slides })
}

fn slide_to_expr(slide: &Slide) -> Result<Expr, DeckError> {
    if slide.id.trim().is_empty() {
        return Err(DeckError::InvalidDeck("slide id is empty".to_owned()));
    }
    let blocks = slide
        .blocks
        .iter()
        .map(block_to_expr)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(map(vec![
        entry(FIELD_KIND, Expr::Symbol(office_symbol(KIND_SLIDE))),
        entry(FIELD_ID, Expr::String(slide.id.clone())),
        entry(FIELD_TITLE, Expr::String(slide.title.clone())),
        entry(FIELD_BLOCKS, Expr::List(blocks)),
    ]))
}

fn slide_from_expr(expr: &Expr) -> Result<Slide, DeckError> {
    let entries = expect_map(expr)?;
    expect_kind(entries, KIND_SLIDE)?;
    let id = expect_string(value_for(entries, FIELD_ID)?, FIELD_ID)?.to_owned();
    let title = expect_string(value_for(entries, FIELD_TITLE)?, FIELD_TITLE)?.to_owned();
    let block_exprs = expect_list(value_for(entries, FIELD_BLOCKS)?, FIELD_BLOCKS)?;
    let blocks = block_exprs
        .iter()
        .map(block_from_expr)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Slide { id, title, blocks })
}

fn block_to_expr(block: &SlideBlock) -> Result<Expr, DeckError> {
    Ok(match block {
        SlideBlock::Heading(value) => map(vec![
            entry(FIELD_KIND, Expr::Symbol(office_symbol(BLOCK_HEADING))),
            entry(FIELD_VALUE, Expr::String(value.clone())),
        ]),
        SlideBlock::BulletList(items) => map(vec![
            entry(FIELD_KIND, Expr::Symbol(office_symbol(BLOCK_BULLET_LIST))),
            entry(
                FIELD_ITEMS,
                Expr::List(items.iter().cloned().map(Expr::String).collect()),
            ),
        ]),
        SlideBlock::Table { columns, rows } => map(vec![
            entry(FIELD_KIND, Expr::Symbol(office_symbol(BLOCK_TABLE))),
            entry(
                FIELD_COLUMNS,
                Expr::List(columns.iter().cloned().map(Expr::String).collect()),
            ),
            entry(
                FIELD_ROWS,
                Expr::List(
                    rows.iter()
                        .map(|row| Expr::List(row.iter().cloned().map(Expr::String).collect()))
                        .collect(),
                ),
            ),
        ]),
        SlideBlock::ImageRef(reference) => map(vec![
            entry(FIELD_KIND, Expr::Symbol(office_symbol(BLOCK_IMAGE_REF))),
            entry(FIELD_BACKEND, Expr::String(reference.backend.clone())),
            entry(
                FIELD_EXTERNAL_ID,
                Expr::String(reference.external_id.clone()),
            ),
            entry(FIELD_VERSION, option_string(&reference.version)),
            entry(FIELD_WEB_URL, option_string(&reference.web_url)),
        ]),
    })
}

fn block_from_expr(expr: &Expr) -> Result<SlideBlock, DeckError> {
    let entries = expect_map(expr)?;
    let kind = expect_symbol(value_for(entries, FIELD_KIND)?, FIELD_KIND)?;
    match kind.as_qualified_str().as_str() {
        "office/deck-heading" => Ok(SlideBlock::Heading(
            expect_string(value_for(entries, FIELD_VALUE)?, FIELD_VALUE)?.to_owned(),
        )),
        "office/deck-bullet-list" => Ok(SlideBlock::BulletList(strings_from_expr(
            value_for(entries, FIELD_ITEMS)?,
            FIELD_ITEMS,
        )?)),
        "office/deck-table" => Ok(SlideBlock::Table {
            columns: strings_from_expr(value_for(entries, FIELD_COLUMNS)?, FIELD_COLUMNS)?,
            rows: rows_from_expr(value_for(entries, FIELD_ROWS)?)?,
        }),
        "office/deck-image-ref" => Ok(SlideBlock::ImageRef(ExternalRef::new(
            expect_string(value_for(entries, FIELD_BACKEND)?, FIELD_BACKEND)?,
            expect_string(value_for(entries, FIELD_EXTERNAL_ID)?, FIELD_EXTERNAL_ID)?,
            optional_string(value_for(entries, FIELD_VERSION)?)?,
            optional_string(value_for(entries, FIELD_WEB_URL)?)?,
        ))),
        other => Err(DeckError::WrongDocBody(format!(
            "unsupported slide block kind {other}"
        ))),
    }
}

fn option_string(value: &Option<String>) -> Expr {
    match value {
        Some(value) => Expr::String(value.clone()),
        None => Expr::Nil,
    }
}

fn optional_string(expr: &Expr) -> Result<Option<String>, DeckError> {
    match expr {
        Expr::Nil => Ok(None),
        Expr::String(value) => Ok(Some(value.clone())),
        _ => Err(DeckError::WrongDocBody(
            "optional reference field must be nil or string".to_owned(),
        )),
    }
}

fn rows_from_expr(expr: &Expr) -> Result<Vec<Vec<String>>, DeckError> {
    expect_list(expr, FIELD_ROWS)?
        .iter()
        .map(|row| strings_from_expr(row, FIELD_ROWS))
        .collect()
}

fn strings_from_expr(expr: &Expr, label: &'static str) -> Result<Vec<String>, DeckError> {
    expect_list(expr, label)?
        .iter()
        .map(|item| expect_string(item, label).map(str::to_owned))
        .collect()
}

fn map(entries: Vec<(Expr, Expr)>) -> Expr {
    Expr::Map(entries)
}

fn entry(name: &'static str, value: Expr) -> (Expr, Expr) {
    (Expr::Symbol(Symbol::new(name)), value)
}

fn office_symbol(name: &'static str) -> Symbol {
    Symbol::qualified("office", name)
}

fn value_for<'a>(entries: &'a [(Expr, Expr)], name: &'static str) -> Result<&'a Expr, DeckError> {
    entries
        .iter()
        .find_map(|(key, value)| match key {
            Expr::Symbol(symbol) if symbol == &Symbol::new(name) => Some(value),
            _ => None,
        })
        .ok_or_else(|| DeckError::WrongDocBody(format!("missing field {name}")))
}

fn expect_map(expr: &Expr) -> Result<&[(Expr, Expr)], DeckError> {
    match expr {
        Expr::Map(entries) => Ok(entries),
        _ => Err(DeckError::WrongDocBody("expected map".to_owned())),
    }
}

fn expect_list<'a>(expr: &'a Expr, label: &'static str) -> Result<&'a [Expr], DeckError> {
    match expr {
        Expr::List(items) => Ok(items),
        _ => Err(DeckError::WrongDocBody(format!(
            "field {label} must be a list"
        ))),
    }
}

fn expect_string<'a>(expr: &'a Expr, label: &'static str) -> Result<&'a str, DeckError> {
    match expr {
        Expr::String(value) => Ok(value),
        _ => Err(DeckError::WrongDocBody(format!(
            "field {label} must be a string"
        ))),
    }
}

fn expect_symbol<'a>(expr: &'a Expr, label: &'static str) -> Result<&'a Symbol, DeckError> {
    match expr {
        Expr::Symbol(value) => Ok(value),
        _ => Err(DeckError::WrongDocBody(format!(
            "field {label} must be a symbol"
        ))),
    }
}

fn expect_kind(entries: &[(Expr, Expr)], expected: &'static str) -> Result<(), DeckError> {
    let kind = expect_symbol(value_for(entries, FIELD_KIND)?, FIELD_KIND)?;
    let expected = office_symbol(expected);
    if kind == &expected {
        Ok(())
    } else {
        Err(DeckError::WrongDocBody(format!(
            "expected kind {}, got {}",
            expected,
            kind.as_qualified_str()
        )))
    }
}

#[cfg(test)]
mod tests {
    use sim_lib_doc_core::ExternalRef;

    use super::*;

    #[test]
    fn deck_expression_round_trips() {
        let mut deck = Deck::new("Quarter Review");
        let mut slide = Slide::new("slide-1", "Results");
        slide.push_block(SlideBlock::Heading("Results".to_owned()));
        slide.push_block(SlideBlock::BulletList(vec![
            "Revenue up".to_owned(),
            "Margin steady".to_owned(),
        ]));
        slide.push_block(SlideBlock::ImageRef(ExternalRef::new(
            "site/msgraph",
            "drive-item-1",
            Some("etag-1".to_owned()),
            None,
        )));
        deck.push_slide(slide);

        let expr = deck_to_expr(&deck).unwrap();
        let decoded = deck_from_expr(&expr).unwrap();

        assert_eq!(decoded, deck);
    }
}
