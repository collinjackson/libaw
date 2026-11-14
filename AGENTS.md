# AGENTS.md

Building a bot on libaw? Read [docs/bots.md](docs/bots.md) first: what to ask the engine instead of
reimplementing the rules, what a seat may know, and where the rules are written down. The same page
is on the docs site: https://collinjackson.github.io/libaw/bots.html

Changing the engine itself? See [CONTRIBUTING.md](CONTRIBUTING.md).

Docs live in `docs/` and are published as a book (`book.toml`, table of contents in
`docs/SUMMARY.md`). `docs/rules/tables.md` is generated: run
`UPDATE_DOCS=1 cargo test -p libaw --test rules_tables` after changing `data/`.
