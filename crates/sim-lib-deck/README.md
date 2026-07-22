# sim-lib-deck

Presentation deck domain model for SIM office documents.

The crate represents decks as ordered slides with headings, bullet lists,
tables, and external image references. It projects decks into the shared office
`Doc` contract as data-position expressions and decodes them back to deck
records, giving file codecs and hosted presentation placements one portable
record to share.
