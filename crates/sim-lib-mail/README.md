# sim-lib-mail

Mail and calendar domain model for SIM office documents.

The crate represents mail messages and calendar events as local records with
redacted body previews and attachment references. It projects those records into
the shared office `Doc` contract and decodes them back, giving Outlook, Graph,
stores, and surfaces one privacy-aware shape to share.
