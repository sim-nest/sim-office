//! Deck document adapters.

use sim_kernel::Cx;
use sim_lib_doc_core::{Doc, DocId, DocKind};

use crate::model::{DECK_DOC_KIND, Deck, DeckError, deck_from_expr, deck_to_expr};

/// Converts a deck model into an office document.
pub fn deck_to_doc(cx: &mut Cx, id: DocId, deck: &Deck) -> Result<Doc, DeckError> {
    let body_expr = deck_to_expr(deck)?;
    let body = cx.factory().expr(body_expr)?;
    Ok(Doc::new(DocKind::new(DECK_DOC_KIND), id, body, Vec::new()))
}

/// Decodes an office deck document into the deck model.
pub fn doc_to_deck(cx: &mut Cx, doc: &Doc) -> Result<Deck, DeckError> {
    ensure_deck(doc)?;
    let expr = doc.body.object().as_expr(cx)?;
    deck_from_expr(&expr)
}

fn ensure_deck(doc: &Doc) -> Result<(), DeckError> {
    if doc.kind.as_str() == DECK_DOC_KIND {
        Ok(())
    } else {
        Err(DeckError::WrongDocKind(doc.kind.as_str().to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sim_kernel::{Cx, DefaultFactory, NoopEvalPolicy};
    use sim_lib_doc_core::DocId;

    use super::*;
    use crate::{Slide, SlideBlock};

    fn test_context() -> Cx {
        Cx::new(Arc::new(NoopEvalPolicy), Arc::new(DefaultFactory))
    }

    #[test]
    fn deck_docs_round_trip() {
        let mut context = test_context();
        let mut deck = Deck::new("Project Update");
        let mut slide = Slide::new("slide-1", "Status");
        slide.push_block(SlideBlock::Heading("Status".to_owned()));
        deck.push_slide(slide);

        let doc = deck_to_doc(&mut context, DocId::new("deck-1"), &deck).unwrap();
        let decoded = doc_to_deck(&mut context, &doc).unwrap();

        assert_eq!(decoded, deck);
    }
}
