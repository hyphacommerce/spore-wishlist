//! A wishlist built into Hypha's API: the tutorial spore. Signed-in shoppers save products
//! through storefront routes of the spore's own, and staff see a customer's list on their
//! page in the admin. Read it beside Hypha's spore guide (`docs/spores/README.md`) and the
//! tutorial that walks through it.
//!
//! What's in it, and what each part shows:
//!
//! - **`lib.rs`**: the `Spore` impl (its name, scopes, migrations, configuration) and the
//!   routes it declares.
//! - **`repo.rs`**: its own table and the repository trait over it, implemented for Hypha's
//!   store; core data (products, customers) is only ever read through core's ports.
//! - **`routes.rs`**: storefront routes for a signed-in shopper (`CustomerCaller`), the
//!   spore's own error codes, and the OpenAPI annotations the host documents them with.
//! - **`ui.rs`**: an admin block on the customer's page.
//! - **`tests/routes.rs`**: a host built with the spore, on Postgres, driven over HTTP.

pub mod repo;
pub mod routes;
pub mod ui;

use hypha_spore::core::{ErrorCode, ProductId, Scope};
use hypha_spore::migrations::{Migration, MigrationSet};
use hypha_spore::{Spore, SporeEnv, SporeError};
use serde::Deserialize;

/// The spore.
pub struct Wishlist {
    pub env: SporeEnv,
    /// How many products a shopper may save.
    pub max_items: u32,
}

/// Its section of `hypha.config.toml`, `[spores.wishlist]`: the operator's settings. A key
/// the spore doesn't know stops the API at startup, naming it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    /// Defaults to 100.
    #[serde(default)]
    max_items: Option<u32>,
}

impl Wishlist {
    pub const DEFAULT_MAX_ITEMS: u32 = 100;
}

/// What the routes refuse. Every variant has a stable code clients match on.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WishlistError {
    #[error("the wishlist holds at most {max} products")]
    Full { max: u32 },
    #[error("product {0} doesn't exist or isn't active")]
    ProductNotFound(ProductId),
}

impl ErrorCode for WishlistError {
    fn code(&self) -> &'static str {
        match self {
            WishlistError::Full { .. } => "wishlist_full",
            WishlistError::ProductNotFound(_) => "product_not_found",
        }
    }
}

impl Spore for Wishlist {
    /// Its key is `spore_wishlist`, its tables `spore_wishlist_*`. Never rename it.
    const NAME: &'static str = "wishlist";
    /// What it acts with when it reads core (products, the customer the block is on).
    const SCOPES: &'static [Scope] = &[Scope::ProductsRead, Scope::CustomersRead];

    fn title() -> &'static str {
        "Wishlist"
    }

    /// Append-only: a change is a new migration, never an edit. Rows go with their
    /// customer or product (`ON DELETE CASCADE`), which is the only link to core allowed.
    fn migrations() -> MigrationSet {
        MigrationSet {
            module: "spore_wishlist",
            migrations: &[Migration {
                version: 1,
                name: "items",
                sql: "CREATE TABLE spore_wishlist_items (
                    customer_id uuid   NOT NULL REFERENCES customers (id) ON DELETE CASCADE,
                    product_id  uuid   NOT NULL REFERENCES products (id) ON DELETE CASCADE,
                    added_at    bigint NOT NULL,
                    PRIMARY KEY (customer_id, product_id)
                );
                CREATE INDEX spore_wishlist_items_product ON spore_wishlist_items (product_id);",
            }],
        }
    }

    /// Built once at startup; an error here stops the API, naming the spore.
    fn new(env: SporeEnv) -> Result<Self, SporeError> {
        let config: Config = env.config_as()?;
        let max_items = config.max_items.unwrap_or(Self::DEFAULT_MAX_ITEMS);
        if max_items == 0 {
            return Err(SporeError::Config {
                key: Some("max_items".to_string()),
                message: "must be at least 1".to_string(),
            });
        }
        Ok(Wishlist { env, max_items })
    }

    // No payment provider, no event handler: a wishlist needs neither.
    type PaymentProviders = ();
    type Handlers = ();
    fn payment_providers(&self) {}
    fn handlers(&self) {}
}

// The routes, at the crate root as `spore_routes!` asks: a host built with the spore
// expands `wishlist_routes!` with its storage. Paths live under the spore's name.
hypha_spore::http::spore_routes! { crate::wishlist_routes for Wishlist:
    [storefront, GET, "/storefront/spores/wishlist/items", routes::list],
    [storefront, PUT, "/storefront/spores/wishlist/items/{product}", routes::add],
    [storefront, DELETE, "/storefront/spores/wishlist/items/{product}", routes::remove],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_have_stable_codes() {
        assert_eq!(WishlistError::Full { max: 3 }.code(), "wishlist_full");
        assert_eq!(
            WishlistError::Full { max: 3 }.to_string(),
            "the wishlist holds at most 3 products"
        );
    }
}
