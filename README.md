# Wishlist for Hypha

A wishlist for a [Hypha](https://github.com/hyphacommerce/hypha) store, as a **spore**: a
Rust crate compiled into the store's API. It's the tutorial spore: small enough to read in
one sitting, and it touches each thing a spore can do with data of its own. The walkthrough
is on the docs site, [Your first spore](https://hyphacommerce.com/docs/guides/spore-tutorial/);
Hypha's spore guide is `docs/spores/README.md` in its repository.

What it adds:

- a table of its own, `spore_wishlist_items`, one row per customer and product, gone with
  either (`src/repo.rs`);
- storefront routes for a signed-in shopper: `GET`, `PUT` and `DELETE
  /storefront/spores/wishlist/items[/{product}]`, answering the list with each product's
  handle and title, only active products shown (`src/routes.rs`);
- an operator setting, `[spores.wishlist] max_items` in `hypha.config.toml` (100 by
  default), past which saving is `409 wishlist_full`;
- a block on the customer's page in the admin listing what they saved (`src/ui.rs`).

## Using it

In a deployment: `hypha spores add wishlist --crate hypha-spore-wishlist --type
hypha_spore_wishlist::Wishlist --routes hypha_spore_wishlist::wishlist_routes --git
https://github.com/hyphacommerce/spore-wishlist`, then `hypha deploy`. The storefront calls
the routes with the publishable key and the shopper's session, as it calls `/storefront/account`.

## Developing

```
docker compose up -d        # in a Hypha checkout: the route tests' Postgres
cargo test
cargo build --lib --target wasm32-unknown-unknown
```

Hypha is a git dependency pinned by `Cargo.lock`. To build against a checkout beside this
one, patch it on the command line:

```
cargo test \
  --config 'patch."https://github.com/hyphacommerce/hypha".hypha-spore.path="../hypha/crates/hypha-spore"' \
  --config 'patch."https://github.com/hyphacommerce/hypha".hypha-host-axum.path="../hypha/crates/hypha-host-axum"'
```

## License

MIT or Apache-2.0, at your option.
