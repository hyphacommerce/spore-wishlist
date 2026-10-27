//! What the spore adds to the admin: a block on a customer's page listing what they saved.
//! Blocks are declarative trees (the app block format, version 1) the admin renders with
//! its own components; the spore answers them in-process, with the caller's ports.

use hypha_spore::core::Scope;
use hypha_spore::products::{HasProducts, ProductRepo};
use hypha_spore::ui::{SporeUi, Target, UiBlock, UiCall, UiError};
use serde_json::{Value, json};

use crate::Wishlist;
use crate::repo::{HasWishlist, WishlistRepo};

impl<P: HasProducts + HasWishlist> SporeUi<P> for Wishlist {
    fn blocks() -> Vec<UiBlock> {
        vec![UiBlock {
            id: "items",
            target: Target::CustomerDetails,
        }]
    }

    async fn block(&self, id: &str, call: UiCall<'_>, ports: &P) -> Result<Value, UiError> {
        if id != "items" {
            return Err(UiError::NotFound);
        }
        let customer = call.context.customer.ok_or(UiError::NotFound)?;
        // The spore acts as its app, with its scopes as far as the staff member holds them.
        call.ctx
            .require(Scope::ProductsRead)
            .map_err(|e| UiError::Failed(e.to_string()))?;
        let saved = ports
            .wishlist()
            .list(customer)
            .await
            .map_err(|e| UiError::Failed(e.to_string()))?;
        let ids: Vec<_> = saved.iter().map(|item| item.product).collect();
        let products = ports
            .products()
            .get_many(&ids)
            .await
            .map_err(|e| UiError::Failed(e.to_string()))?;
        let items: Vec<Value> = saved
            .iter()
            .filter_map(|item| products.iter().find(|p| p.id() == item.product))
            .map(|product| {
                json!({
                    "label": product.title(),
                    "description": format!("/{}, {}", product.handle(), product.status().as_str()),
                })
            })
            .collect();
        let children = if items.is_empty() {
            vec![json!({ "type": "text", "value": "Nothing saved yet." })]
        } else {
            vec![json!({ "type": "list", "items": items })]
        };
        Ok(json!({
            "version": 1,
            "root": {
                "type": "card",
                "title": format!("Wishlist ({})", items.len()),
                "children": children
            }
        }))
    }
}
