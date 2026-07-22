# sim-site-sharepoint

`sim-site-sharepoint` reads SharePoint lists and drive folders through Microsoft
Graph. List items become sheet documents for local inspection, and drive items
become `ExternalRef` records with ETags preserved as write preconditions.

Microsoft Graph v1 is the API boundary for SharePoint sites, lists, list items,
drives, and drive items. Site provisioning is not handled by this crate; it stays
outside the Graph v1 placement contract.

## Documentation

Run the repository documentation command from the `sim-office` root:

```bash
cargo run -p xtask -- simdoc
```

The generated `docs/` tree is owned by that command.
