//! The storefront routes: a signed-in shopper reads, saves and removes products. Each
//! handler takes a `CustomerCaller` (the publishable key and the customer's session; without
//! the session, `401 account_required`), authenticates it for the request's ports, and
//! answers the whole list, so a storefront never needs a second read.

use hypha_spore::core::{Clock, ProductId, Scope};
use hypha_spore::http::axum::http::StatusCode;
use hypha_spore::http::axum::{Json, extract::State};
use hypha_spore::http::{ApiError, CustomerCaller, PathId, SharedState, Storage, rfc3339, utoipa};
use hypha_spore::products::{HasProducts, ProductRepo, ProductStatus};
use serde::Serialize;

use crate::repo::{HasWishlist, WishlistRepo};
use crate::{Wishlist, WishlistError};

/// A saved product, as a storefront shows it.
#[derive(Serialize, utoipa::ToSchema)]
pub struct WishlistItem {
    pub product: ProductId,
    pub handle: String,
    pub title: String,
    /// RFC 3339.
    pub added_at: String,
}

/// The shopper's wishlist, oldest first. Products no longer active are left out.
#[derive(Serialize, utoipa::ToSchema)]
pub struct WishlistResponse {
    pub items: Vec<WishlistItem>,
    /// How many may be saved (`[spores.wishlist] max_items`).
    pub max_items: u32,
}

/// The list as the storefront sees it: the saved products that are still active.
async fn respond<P: HasWishlist + HasProducts>(
    spore: &Wishlist,
    ports: &P,
    customer: hypha_spore::core::CustomerId,
) -> Result<Json<WishlistResponse>, ApiError> {
    let saved = ports.wishlist().list(customer).await?;
    let ids: Vec<ProductId> = saved.iter().map(|item| item.product).collect();
    let products = ports.products().get_many(&ids).await?;
    let mut items = Vec::with_capacity(saved.len());
    for item in saved {
        let Some(product) = products.iter().find(|p| p.id() == item.product) else {
            continue;
        };
        if product.status() != ProductStatus::Active {
            continue;
        }
        items.push(WishlistItem {
            product: item.product,
            handle: product.handle().to_string(),
            title: product.title().to_string(),
            added_at: rfc3339(item.added_at)?,
        });
    }
    Ok(Json(WishlistResponse {
        items,
        max_items: spore.max_items,
    }))
}

/// What the signed-in shopper saved.
#[utoipa::path(
    get,
    path = "/storefront/spores/wishlist/items",
    responses((status = 200, description = "OK", body = WishlistResponse)),
    extensions(("x-hypha-scopes" = json!([])), ("x-hypha-customer-session" = json!(true))),
)]
pub async fn list<S: Storage>(
    State(state): State<SharedState<S>>,
    caller: CustomerCaller,
) -> Result<Json<WishlistResponse>, ApiError>
where
    for<'a> S::Ports<'a>: HasWishlist,
{
    let (ports, me) = caller.authenticate(&state).await?;
    let spore = hypha_spore::http::spore::<Wishlist, S>(&state)?;
    respond(spore, &ports, me.customer.id()).await
}

/// Saves a product. Saving one that's saved changes nothing; a product that doesn't exist
/// or isn't active is `404 product_not_found`; a full list is `409 wishlist_full`.
#[utoipa::path(
    put,
    path = "/storefront/spores/wishlist/items/{product}",
    params(("product" = ProductId, Path, description = "The product to save")),
    responses((status = 200, description = "OK", body = WishlistResponse)),
    extensions(("x-hypha-scopes" = json!([])), ("x-hypha-customer-session" = json!(true))),
)]
pub async fn add<S: Storage>(
    State(state): State<SharedState<S>>,
    caller: CustomerCaller,
    PathId(product): PathId<ProductId>,
) -> Result<Json<WishlistResponse>, ApiError>
where
    for<'a> S::Ports<'a>: HasWishlist,
{
    let (ports, me) = caller.authenticate(&state).await?;
    // The shopper's own context: what the storefront may read, as them.
    me.ctx.require(Scope::ProductsRead)?;
    let spore = hypha_spore::http::spore::<Wishlist, S>(&state)?;
    let customer = me.customer.id();
    let found = ports.products().get(product).await?;
    if !found.is_some_and(|p| p.status() == ProductStatus::Active) {
        return Err(ApiError::of(
            StatusCode::NOT_FOUND,
            &WishlistError::ProductNotFound(product),
        ));
    }
    let saved = ports.wishlist().list(customer).await?;
    let already = saved.iter().any(|item| item.product == product);
    if !already && saved.len() >= spore.max_items as usize {
        return Err(ApiError::of(
            StatusCode::CONFLICT,
            &WishlistError::Full {
                max: spore.max_items,
            },
        ));
    }
    ports
        .wishlist()
        .add(customer, product, spore.env.now())
        .await?;
    respond(spore, &ports, customer).await
}

/// Removes a product; removing one that isn't saved changes nothing.
#[utoipa::path(
    delete,
    path = "/storefront/spores/wishlist/items/{product}",
    params(("product" = ProductId, Path, description = "The product to remove")),
    responses((status = 200, description = "OK", body = WishlistResponse)),
    extensions(("x-hypha-scopes" = json!([])), ("x-hypha-customer-session" = json!(true))),
)]
pub async fn remove<S: Storage>(
    State(state): State<SharedState<S>>,
    caller: CustomerCaller,
    PathId(product): PathId<ProductId>,
) -> Result<Json<WishlistResponse>, ApiError>
where
    for<'a> S::Ports<'a>: HasWishlist,
{
    let (ports, me) = caller.authenticate(&state).await?;
    let spore = hypha_spore::http::spore::<Wishlist, S>(&state)?;
    let customer = me.customer.id();
    ports.wishlist().remove(customer, product).await?;
    respond(spore, &ports, customer).await
}
