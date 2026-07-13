# sim-lib-office-pack

`sim-lib-office-pack` plans annual accounts review packs from closed ledger
statements. It turns the statement projection into local `.xlsx` and `.pptx`
bytes, then returns preview-only office edits for spreadsheet export, deck
export, Outlook draft preparation, and SharePoint archive preparation.

The crate does not upload files, send mail, or call a live service. Hosts inspect
the returned edits and commit them through their chosen sites after an operator
approves the pack.
