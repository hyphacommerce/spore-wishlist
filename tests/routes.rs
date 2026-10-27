//! A host built with the wishlist spore, on Postgres (`HYPHA_TEST_DATABASE_URL`, Hypha's
//! compose `postgres-test`): a shopper signs in, saves and removes products, hits the
//! limit, and staff see the list on the customer's page.

use hypha_host_axum::testing::{TestApp, app_with, publishable, send, send_with};
use hypha_spore_wishlist::{Wishlist, wishlist_routes};
use serde_json::{Value, json};

hypha_host_axum::deployment! { enum WithWishlist {
    spores: [Wishlist],
    routes: [wishlist_routes],
} }

type App = TestApp<WithWishlist>;

/// An active product; `draft` leaves it unpublished.
async fn product(app: &App, handle: &str, active: bool) -> String {
    let body = json!({ "handle": handle, "title": handle, "price": { "amount": 1000, "currency": "EUR" } });
    let (status, created) = send(app, "POST", "/admin/products", Some(&body.to_string())).await;
    assert_eq!(status, 201, "{created}");
    let id = created["id"].as_str().unwrap().to_string();
    if active {
        let (status, _) = send(
            app,
            "PATCH",
            &format!("/admin/products/{id}/status"),
            Some(r#"{"status":"active"}"#),
        )
        .await;
        assert_eq!(status, 200);
    }
    id
}

/// A shopper signed in with the publishable key `pk`: (the session token, the customer's ID).
async fn shopper(app: &App, pk: &str, email: &str) -> (String, String) {
    let body = json!({ "email": email }).to_string();
    let (status, _) = send_with(
        app,
        &[("x-publishable-key", pk)],
        "POST",
        "/storefront/auth/code",
        Some(&body),
    )
    .await;
    assert_eq!(status, 202);
    // The six digits in the email the host sent (the test kit records them).
    let text = app.emails().last().unwrap().text.clone();
    let code = text
        .split(|c: char| !c.is_ascii_digit())
        .find(|w| w.len() == 6)
        .unwrap_or_else(|| panic!("no code in {text}"))
        .to_string();
    let body = json!({ "email": email, "code": code }).to_string();
    let (status, json) = send_with(
        app,
        &[("x-publishable-key", pk)],
        "POST",
        "/storefront/auth/login",
        Some(&body),
    )
    .await;
    assert_eq!(status, 200, "{json}");
    (
        json["token"].as_str().unwrap().to_string(),
        json["customer"]["id"].as_str().unwrap().to_string(),
    )
}

/// A request as the signed-in shopper.
async fn as_shopper(app: &App, pk: &str, session: &str, method: &str, uri: &str) -> (u16, Value) {
    let auth = format!("Bearer {session}");
    let (status, json) = send_with(
        app,
        &[("x-publishable-key", pk), ("authorization", &auth)],
        method,
        uri,
        None,
    )
    .await;
    (status.as_u16(), json)
}

fn handles(json: &Value) -> Vec<&str> {
    json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["handle"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn a_shopper_saves_and_removes_products_within_the_limit() {
    let app = app_with::<WithWishlist>("[spores.wishlist]\nmax_items = 2").await;
    let pk = publishable(&app).await;
    let mug = product(&app, "mug", true).await;
    let cup = product(&app, "cup", true).await;
    let bowl = product(&app, "bowl", true).await;
    let (session, _) = shopper(&app, &pk, "nina@example.com").await;
    let items = "/storefront/spores/wishlist/items";

    // Without a session the route wants an account; with one, the list starts empty.
    let (status, json) = send_with(&app, &[("x-publishable-key", &pk)], "GET", items, None).await;
    assert_eq!(
        (status.as_u16(), json["code"].as_str()),
        (401, Some("account_required"))
    );
    let (status, json) = as_shopper(&app, &pk, &session, "GET", items).await;
    assert_eq!(status, 200, "{json}");
    assert_eq!(json, json!({ "items": [], "max_items": 2 }));

    // Saving answers the list; saving again changes nothing.
    let (status, json) = as_shopper(&app, &pk, &session, "PUT", &format!("{items}/{mug}")).await;
    assert_eq!(status, 200, "{json}");
    assert_eq!(handles(&json), ["mug"]);
    assert!(
        json["items"][0]["added_at"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    let (_, json) = as_shopper(&app, &pk, &session, "PUT", &format!("{items}/{mug}")).await;
    assert_eq!(handles(&json), ["mug"]);
    let (_, json) = as_shopper(&app, &pk, &session, "PUT", &format!("{items}/{cup}")).await;
    assert_eq!(handles(&json), ["mug", "cup"]);

    // The operator's limit.
    let (status, json) = as_shopper(&app, &pk, &session, "PUT", &format!("{items}/{bowl}")).await;
    assert_eq!(status, 409, "{json}");
    assert_eq!(json["code"], "wishlist_full");
    assert_eq!(json["error"], "the wishlist holds at most 2 products");

    // Removing answers the list too; a product that isn't saved is nothing to remove.
    let (_, json) = as_shopper(&app, &pk, &session, "DELETE", &format!("{items}/{mug}")).await;
    assert_eq!(handles(&json), ["cup"]);
    let (status, json) = as_shopper(&app, &pk, &session, "DELETE", &format!("{items}/{mug}")).await;
    assert_eq!((status, handles(&json)), (200, vec!["cup"]));
}

#[tokio::test]
async fn only_active_products_can_be_saved_and_a_drafted_one_disappears() {
    let app = app_with::<WithWishlist>("").await;
    let pk = publishable(&app).await;
    let mug = product(&app, "mug", true).await;
    let draft = product(&app, "draft", false).await;
    let (session, _) = shopper(&app, &pk, "nina@example.com").await;
    let items = "/storefront/spores/wishlist/items";

    let (status, json) = as_shopper(&app, &pk, &session, "PUT", &format!("{items}/{draft}")).await;
    assert_eq!(
        (status, json["code"].as_str()),
        (404, Some("product_not_found"))
    );
    let unknown = "prod_00000000000000000000000000000001";
    let (status, _) = as_shopper(&app, &pk, &session, "PUT", &format!("{items}/{unknown}")).await;
    assert_eq!(status, 404);

    let (_, json) = as_shopper(&app, &pk, &session, "PUT", &format!("{items}/{mug}")).await;
    assert_eq!(
        (handles(&json), json["max_items"].as_u64()),
        (vec!["mug"], Some(100))
    );
    // Archived by staff: still saved, no longer shown.
    send(
        &app,
        "PATCH",
        &format!("/admin/products/{mug}/status"),
        Some(r#"{"status":"archived"}"#),
    )
    .await;
    let (_, json) = as_shopper(&app, &pk, &session, "GET", items).await;
    assert_eq!(handles(&json), Vec::<&str>::new());
}

#[tokio::test]
async fn staff_see_the_customers_wishlist_on_their_page() {
    let app = app_with::<WithWishlist>("").await;
    let pk = publishable(&app).await;
    let mug = product(&app, "mug", true).await;
    let (session, customer) = shopper(&app, &pk, "nina@example.com").await;
    as_shopper(
        &app,
        &pk,
        &session,
        "PUT",
        &format!("/storefront/spores/wishlist/items/{mug}"),
    )
    .await;

    // The block is discovered for the customer page, then rendered for one customer.
    let (status, found) = send(
        &app,
        "GET",
        "/admin/app-extensions?target=customer.details",
        None,
    )
    .await;
    assert_eq!(status, 200, "{found}");
    assert!(
        found["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["url"] == "/admin/app-extensions/blocks/spore_wishlist/items"),
        "{found}"
    );
    let path = format!("/admin/app-extensions/blocks/spore_wishlist/items?customer_id={customer}");
    let (status, block) = send(&app, "GET", &path, None).await;
    assert_eq!(status, 200, "{block}");
    let root = &block["tree"]["root"];
    assert_eq!(root["title"], "Wishlist (1)", "{block}");
    assert_eq!(root["children"][0]["items"][0]["label"], "mug");
}

#[tokio::test]
async fn a_bad_limit_stops_the_host() {
    // A limit of zero means nothing could be saved: refused when the host is built, which
    // the test kit reports as a panic naming the spore and the key.
    let built = tokio::task::spawn(app_with::<WithWishlist>("[spores.wishlist]\nmax_items = 0"));
    assert!(built.await.is_err());
}
