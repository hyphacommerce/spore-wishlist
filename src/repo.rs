//! The spore's own table, `spore_wishlist_items`: one row per (customer, product), read and
//! written through a repository trait of the spore's own, implemented for Hypha's store.
//! That's how a spore keeps data: its tables, keyed by core's IDs, which go when the core
//! row goes (`ON DELETE CASCADE`); never a write to core's tables.

use hypha_spore::core::{CustomerId, ProductId, RepoError, Timestamp};
use hypha_spore::postgres::tokio_postgres::types::Type;
use hypha_spore::postgres::{PgConnection, PgStore, Queries, spore_repo_error};
use uuid::Uuid;

/// A saved product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub product: ProductId,
    pub added_at: Timestamp,
}

/// A customer's wishlist, oldest first.
#[allow(async_fn_in_trait)]
pub trait WishlistRepo {
    async fn list(&self, customer: CustomerId) -> Result<Vec<Item>, RepoError>;
    /// Saves the product; `false` when it was saved already (nothing changes).
    async fn add(
        &self,
        customer: CustomerId,
        product: ProductId,
        now: Timestamp,
    ) -> Result<bool, RepoError>;
    /// Removes the product; `false` when it wasn't saved.
    async fn remove(&self, customer: CustomerId, product: ProductId) -> Result<bool, RepoError>;
}

/// The accessor the spore's handlers bound their ports on, like a domain crate's `HasXxx`.
pub trait HasWishlist {
    type Wishlist: WishlistRepo;
    fn wishlist(&self) -> &Self::Wishlist;
}

// Allowed by the orphan rule: the trait is ours, so it may be implemented for Hypha's
// store. Queries go through the connection's `db()` with the driver's typed parameters.
impl<C: PgConnection> WishlistRepo for PgStore<C> {
    async fn list(&self, customer: CustomerId) -> Result<Vec<Item>, RepoError> {
        let mut conn = self.conn().await;
        let rows = conn
            .db()
            .query_typed(
                "SELECT product_id, added_at FROM spore_wishlist_items
                 WHERE customer_id = $1 ORDER BY added_at, product_id",
                &[(&customer.as_uuid(), Type::UUID)],
            )
            .await
            .map_err(|e| spore_repo_error(e, &[]))?;
        rows.iter()
            .map(|row| {
                let product: Uuid = row
                    .try_get("product_id")
                    .map_err(|e| spore_repo_error(e, &[]))?;
                let added_at: i64 = row
                    .try_get("added_at")
                    .map_err(|e| spore_repo_error(e, &[]))?;
                Ok(Item {
                    product: ProductId::from_uuid(product),
                    added_at: Timestamp::from_unix_micros(added_at),
                })
            })
            .collect()
    }

    async fn add(
        &self,
        customer: CustomerId,
        product: ProductId,
        now: Timestamp,
    ) -> Result<bool, RepoError> {
        let mut conn = self.conn().await;
        let written = conn
            .db()
            .execute_typed(
                "INSERT INTO spore_wishlist_items (customer_id, product_id, added_at)
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                &[
                    (&customer.as_uuid(), Type::UUID),
                    (&product.as_uuid(), Type::UUID),
                    (&now.unix_micros(), Type::INT8),
                ],
            )
            .await
            .map_err(|e| spore_repo_error(e, &[]))?;
        Ok(written == 1)
    }

    async fn remove(&self, customer: CustomerId, product: ProductId) -> Result<bool, RepoError> {
        let mut conn = self.conn().await;
        let removed = conn
            .db()
            .execute_typed(
                "DELETE FROM spore_wishlist_items WHERE customer_id = $1 AND product_id = $2",
                &[
                    (&customer.as_uuid(), Type::UUID),
                    (&product.as_uuid(), Type::UUID),
                ],
            )
            .await
            .map_err(|e| spore_repo_error(e, &[]))?;
        Ok(removed == 1)
    }
}

impl<C: PgConnection> HasWishlist for PgStore<C> {
    type Wishlist = Self;
    fn wishlist(&self) -> &Self {
        self
    }
}
