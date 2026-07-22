# sim-site-dalux

`sim-site-dalux` reads Dalux project items through an API-identity bearer token
and maps them into local office documents. The write surface is limited to item
note updates so hosts can keep Dalux annotations narrow and inspectable.

Live calls require explicit network and credential capabilities plus the Dalux
live gate. Deterministic tests use modeled responses and never need a Dalux
account.

## Documentation

Run the repository documentation command from the `sim-office` root:

```bash
cargo run -p xtask -- simdoc
```

The generated `docs/` tree is owned by that command.
