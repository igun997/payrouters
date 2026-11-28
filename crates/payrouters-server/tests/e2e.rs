//! End-to-end tests through the real router and a real Postgres (`#[sqlx::test]` provisions a
//! throwaway database per test from `DATABASE_URL`). Providers are never contacted: payments
//! are created without a core charge and settled via signed webhooks or the mock simulator.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, Request, StatusCode};
use axum::routing::post;
use payrouters_server::config::RateConfig;
use payrouters_server::services::{accounts, outbound, worker};
use payrouters_server::util::sha512_hex;
use payrouters_server::{AppState, Config, api};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const ADMIN: &str = "adm-test-token-0123456789";

struct App {
    router: Router,
    state: AppState,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    json: Value,
    text: String,
}

impl App {
    async fn new(pool: PgPool) -> App {
        Self::with_config(pool, |_| {}).await
    }

    async fn with_config(pool: PgPool, edit: impl FnOnce(&mut Config)) -> App {
        let mut config = Config {
            database_url: String::new(),
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            public_base_url: "http://pay.test".into(),
            admin_token: ADMIN.into(),
            master_key: [1u8; 32],
            web_dir: "/nonexistent".into(),
            timezone: "Asia/Jakarta".parse().unwrap(),
            trusted_proxy_header: None,
            rate: RateConfig::default(),
        };
        edit(&mut config);
        let state = AppState::new(pool, config).unwrap();
        accounts::seed_mock(&state).await.unwrap();
        App {
            router: api::router(state.clone()),
            state,
        }
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        bearer: Option<&str>,
        extra: &[(&str, &str)],
        body: Option<String>,
    ) -> Reply {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        let req = match body {
            Some(text) => b
                .header("content-type", "application/json")
                .body(Body::from(text))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let json = serde_json::from_str(&text).unwrap_or(Value::Null);
        Reply {
            status,
            headers,
            json,
            text,
        }
    }

    async fn admin(&self, method: &str, uri: &str, body: Option<Value>) -> Reply {
        self.call(method, uri, Some(ADMIN), &[], body.map(|b| b.to_string()))
            .await
    }

    async fn product(&self, method: &str, uri: &str, key: &str, body: Option<Value>) -> Reply {
        self.call(method, uri, Some(key), &[], body.map(|b| b.to_string()))
            .await
    }

    /// Product + one key per mode. Returns (product id, webhook secret, sandbox key, live key).
    async fn product_with_keys(
        &self,
        name: &str,
        webhook_url: Option<&str>,
    ) -> (String, String, String, String) {
        let mut body = json!({ "name": name });
        if let Some(u) = webhook_url {
            body["webhook_url"] = json!(u);
        }
        let p = self.admin("POST", "/admin/products", Some(body)).await;
        assert_eq!(p.status, StatusCode::CREATED, "{}", p.text);
        let id = p.json["id"].as_str().unwrap().to_string();
        let secret = p.json["webhook_secret"].as_str().unwrap().to_string();
        let mut keys = vec![];
        for mode in ["sandbox", "live"] {
            let k = self
                .admin(
                    "POST",
                    &format!("/admin/products/{id}/api-keys"),
                    Some(json!({ "mode": mode, "name": "t" })),
                )
                .await;
            assert_eq!(k.status, StatusCode::CREATED, "{}", k.text);
            keys.push(k.json["key"].as_str().unwrap().to_string());
        }
        (id, secret, keys.remove(0), keys.remove(0))
    }

    async fn provider(&self, provider: &str, mode: &str, creds: Value) -> (String, String) {
        let r = self
            .admin(
                "POST",
                "/admin/providers",
                Some(json!({ "provider": provider, "mode": mode, "credentials": creds })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
        (
            r.json["id"].as_str().unwrap().to_string(),
            r.json["webhook_url"].as_str().unwrap().to_string(),
        )
    }
}

fn token_of(snap_url: &str) -> &str {
    snap_url.rsplit('/').next().unwrap()
}

type Hits = Arc<Mutex<Vec<(HeaderMap, String)>>>;

async fn spawn_receiver(status: Arc<AtomicU16>) -> (String, Hits) {
    let hits: Hits = Default::default();
    let store = hits.clone();
    let app = Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: String| {
            let store = store.clone();
            let status = status.clone();
            async move {
                store.lock().unwrap().push((headers, body));
                StatusCode::from_u16(status.load(Ordering::SeqCst)).unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/hook"), hits)
}

fn midtrans_notification(payment_id: &str, gross: &str, status: &str, key: &str) -> String {
    let sig = sha512_hex(format!("{payment_id}200{gross}{key}"));
    json!({
        "order_id": payment_id, "status_code": "200", "gross_amount": gross,
        "transaction_status": status, "transaction_id": "tx-1", "payment_type": "bank_transfer",
        "va_numbers": [{ "bank": "bca", "va_number": "123" }],
        "settlement_time": "2026-11-26 17:00:00", "signature_key": sig,
    })
    .to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn mock_payment_end_to_end_with_signed_outbound_webhook(pool: PgPool) {
    let app = App::new(pool).await;
    let status = Arc::new(AtomicU16::new(500));
    let (hook_url, hits) = spawn_receiver(status.clone()).await;
    let (_pid, secret, key, _live) = app.product_with_keys("Shop", Some(&hook_url)).await;

    let body = json!({ "order_id": "INV-1", "amount": 150000, "provider": "mock", "method": "va_bca", "customer": { "name": "Budi" } });
    let created = app
        .product("POST", "/v1/payments", &key, Some(body.clone()))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    assert_eq!(created.json["status"], "pending");
    assert_eq!(created.json["instructions"]["type"], "virtual_account");
    assert_eq!(created.json["livemode"], false);
    let id = created.json["id"].as_str().unwrap().to_string();
    let snap_url = created.json["snap_url"].as_str().unwrap().to_string();
    assert!(snap_url.starts_with("http://pay.test/snap/"));

    // Idempotent replay.
    let replay = app
        .product("POST", "/v1/payments", &key, Some(body.clone()))
        .await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(
        replay.headers.get("PayRouters-Idempotent-Replay").unwrap(),
        "true"
    );
    assert_eq!(replay.json["id"], created.json["id"]);
    // Same order id, different amount: conflict.
    let mut other = body.clone();
    other["amount"] = json!(1);
    let conflict = app.product("POST", "/v1/payments", &key, Some(other)).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    assert_eq!(conflict.json["error"]["code"], "order_id_conflict");

    // Snap view, then simulate payment through the public endpoint.
    let token = token_of(&snap_url);
    let view = app
        .call("GET", &format!("/snap/api/{token}"), None, &[], None)
        .await;
    assert_eq!(view.status, StatusCode::OK);
    assert_eq!(view.json["allow_simulate"], true);
    assert_eq!(view.json["merchant_name"], "Shop");
    let paid = app
        .call(
            "POST",
            &format!("/snap/api/{token}/simulate"),
            None,
            &[],
            Some(json!({ "outcome": "paid" }).to_string()),
        )
        .await;
    assert_eq!(paid.status, StatusCode::OK, "{}", paid.text);
    assert_eq!(paid.json["payment"]["status"], "paid");

    let got = app
        .product("GET", &format!("/v1/payments/{id}"), &key, None)
        .await;
    assert_eq!(got.json["status"], "paid");
    assert_eq!(got.json["net_amount"], 150000);
    assert_eq!(got.json["fee_amount"], 0);

    // Ledger row exists; a second simulate is refused instead of double counting.
    let recap = app.admin("GET", "/admin/cash-ins?mode=sandbox", None).await;
    assert_eq!(recap.json["totals"]["count"], 1);
    assert_eq!(recap.json["totals"]["gross"], 150000);
    let again = app
        .call(
            "POST",
            &format!("/snap/api/{token}/simulate"),
            None,
            &[],
            Some(json!({ "outcome": "paid" }).to_string()),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);

    // Outbound webhook: first attempt fails (500) and is rescheduled.
    assert_eq!(outbound::run_once(&app.state).await.unwrap(), 1);
    let d = app.admin("GET", "/admin/deliveries", None).await;
    assert_eq!(d.json["data"][0]["status"], "pending");
    assert_eq!(d.json["data"][0]["attempts"], 1);
    assert_eq!(d.json["data"][0]["last_status_code"], 500);
    let delivery_id = d.json["data"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(
        outbound::run_once(&app.state).await.unwrap(),
        0,
        "backoff must hold the retry back"
    );

    // Receiver recovers; manual retry delivers a correctly signed event.
    status.store(200, Ordering::SeqCst);
    let retry = app
        .admin(
            "POST",
            &format!("/admin/deliveries/{delivery_id}/retry"),
            None,
        )
        .await;
    assert_eq!(retry.status, StatusCode::OK);
    assert_eq!(outbound::run_once(&app.state).await.unwrap(), 1);
    let d = app.admin("GET", "/admin/deliveries", None).await;
    assert_eq!(d.json["data"][0]["status"], "delivered");

    let (headers, raw) = hits.lock().unwrap().last().cloned().unwrap();
    let sig = headers
        .get("PayRouters-Signature")
        .unwrap()
        .to_str()
        .unwrap();
    let event = payrouters_sdk::webhook::construct_event(raw.as_bytes(), sig, &secret, 300, None)
        .expect("server signature must verify with the SDK");
    assert_eq!(event.event_type, payrouters_types::EventType::PaymentPaid);
    assert_eq!(event.data.payment.order_id, "INV-1");
    assert!(!event.livemode);
    assert_eq!(
        headers
            .get("PayRouters-Event-Id")
            .unwrap()
            .to_str()
            .unwrap(),
        event.id
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn midtrans_webhook_settles_once_and_rejects_bad_input(pool: PgPool) {
    let app = App::new(pool).await;
    let key_s = "SB-Mid-server-test1234";
    let (account, webhook_url) = app
        .provider("midtrans", "sandbox", json!({ "server_key": key_s }))
        .await;
    assert!(webhook_url.ends_with(&format!("/hooks/{account}")));
    let (_pid, _secret, key, _) = app.product_with_keys("Shop", None).await;

    let create = |order: &str, amount: i64| json!({ "order_id": order, "amount": amount, "provider": "midtrans" });
    let p = app
        .product("POST", "/v1/payments", &key, Some(create("A", 100000)))
        .await;
    assert_eq!(p.status, StatusCode::CREATED, "{}", p.text);
    assert!(p.json["instructions"].is_null());
    let id = p.json["id"].as_str().unwrap().to_string();
    let path = format!("/hooks/{account}");

    // Forged signature: rejected, nothing changes, the attempt is recorded.
    let forged = midtrans_notification(&id, "100000.00", "settlement", "wrong-key");
    let r = app.call("POST", &path, None, &[], Some(forged)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let got = app
        .product("GET", &format!("/v1/payments/{id}"), &key, None)
        .await;
    assert_eq!(got.json["status"], "pending");
    let events = app
        .admin("GET", "/admin/webhook-events?outcome=rejected", None)
        .await;
    assert_eq!(events.json["data"].as_array().unwrap().len(), 1);
    assert_eq!(events.json["data"][0]["verified"], false);

    // Genuine settlement.
    let good = midtrans_notification(&id, "100000.00", "settlement", key_s);
    assert_eq!(
        app.call("POST", &path, None, &[], Some(good.clone()))
            .await
            .status,
        StatusCode::OK
    );
    let got = app
        .product("GET", &format!("/v1/payments/{id}"), &key, None)
        .await;
    assert_eq!(got.json["status"], "paid");
    assert_eq!(got.json["method"], "va_bca");
    assert_eq!(got.json["provider_ref"], "tx-1");
    assert_eq!(got.json["paid_at"], "2026-11-26T10:00:00Z");

    // Provider retries the same webhook: still exactly one cash-in.
    assert_eq!(
        app.call("POST", &path, None, &[], Some(good)).await.status,
        StatusCode::OK
    );
    let recap = app
        .admin(
            "GET",
            "/admin/cash-ins?mode=sandbox&from=2026-11-26&to=2026-11-26",
            None,
        )
        .await;
    assert_eq!(recap.json["totals"]["count"], 1);

    // Amount mismatch is acknowledged but never settles the payment.
    let q = app
        .product("POST", "/v1/payments", &key, Some(create("B", 100000)))
        .await;
    let qid = q.json["id"].as_str().unwrap().to_string();
    let short = midtrans_notification(&qid, "99999.00", "settlement", key_s);
    assert_eq!(
        app.call("POST", &path, None, &[], Some(short)).await.status,
        StatusCode::OK
    );
    let got = app
        .product("GET", &format!("/v1/payments/{qid}"), &key, None)
        .await;
    assert_eq!(got.json["status"], "pending");
    let events = app
        .admin("GET", "/admin/webhook-events?outcome=rejected", None)
        .await;
    assert!(events.json["data"].as_array().unwrap().iter().any(|e| {
        e["error"]
            .as_str()
            .unwrap_or("")
            .contains("amount mismatch")
    }));

    // Dashboard "test notification" with a foreign order id is authentic but unmatched.
    let foreign = {
        let sig = sha512_hex(format!("payment_notif_test_1200100000.00{key_s}"));
        json!({ "order_id": "payment_notif_test_1", "status_code": "200", "gross_amount": "100000.00",
                "transaction_status": "settlement", "signature_key": sig }).to_string()
    };
    assert_eq!(
        app.call("POST", &path, None, &[], Some(foreign))
            .await
            .status,
        StatusCode::OK
    );
    let events = app
        .admin("GET", "/admin/webhook-events?outcome=unmatched", None)
        .await;
    assert_eq!(events.json["data"].as_array().unwrap().len(), 1);

    // Unknown endpoint.
    let r = app
        .call(
            "POST",
            "/hooks/00000000-0000-0000-0000-000000000000",
            None,
            &[],
            Some("{}".into()),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

/// Since 28 Oct 2026 Midtrans signs notifications with the Signature Secret Key, a key of its own.
/// Its "Test notification URL" button sends an order that does not exist: an authentic one must
/// still get 200, or the dashboard reports the URL as failed.
#[sqlx::test(migrations = "./migrations")]
async fn midtrans_notifications_are_verified_with_the_signature_secret_key(pool: PgPool) {
    let app = App::new(pool).await;
    let server_key = "SB-Mid-server-aaaa1111";
    let signature_key = "signature-secret-bbbb2222";
    let (account, _) = app
        .provider(
            "midtrans",
            "sandbox",
            json!({ "server_key": server_key, "signature_secret_key": signature_key }),
        )
        .await;
    let path = format!("/hooks/{account}");

    // the dashboard shows the new field masked and the form lists it as optional
    let listed = app.admin("GET", "/admin/providers", None).await;
    let shown = &listed.json["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == account.as_str())
        .unwrap()["credentials"];
    assert_eq!(shown["signature_secret_key"], "••••2222");
    let meta = app.admin("GET", "/admin/meta", None).await;
    let midtrans = meta.json["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "midtrans")
        .unwrap();
    let field = midtrans["credential_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["key"] == "signature_secret_key")
        .unwrap();
    assert_eq!(field["required"], false);
    assert_eq!(field["secret"], true);

    // Midtrans' "Test notification URL": unknown order, signed with the Signature Secret Key
    let test = midtrans_notification(
        "payment_notif_test_G1234_abcd",
        "10000.00",
        "settlement",
        signature_key,
    );
    let ok = app.call("POST", &path, None, &[], Some(test)).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);

    // the server key still signs notifications for merchants who never rotated
    let legacy = midtrans_notification(
        "payment_notif_test_G1234_efgh",
        "10000.00",
        "settlement",
        server_key,
    );
    assert_eq!(
        app.call("POST", &path, None, &[], Some(legacy))
            .await
            .status,
        StatusCode::OK
    );

    // any other key is refused, and the stored event says what to check
    let forged = midtrans_notification(
        "payment_notif_test_G1234_ijkl",
        "10000.00",
        "settlement",
        "someone-elses-key",
    );
    let refused = app.call("POST", &path, None, &[], Some(forged)).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert!(
        refused.json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Signature Secret Key")
    );
    let events = app
        .admin("GET", "/admin/webhook-events?outcome=rejected", None)
        .await;
    assert!(
        events.json["data"][0]["error"]
            .as_str()
            .unwrap()
            .contains("Signature Secret Key")
    );

    // the recurring and account-linking "Test notification URL" buttons post bodies without any
    // payment field: acknowledged, nothing changes
    let linking = json!({
        "account_id": "gopay-1", "account_status": "ENABLED", "merchant_id": "G552576396",
        "payment_type": "gopay", "signature_key": "not-ours-to-verify", "status_code": "200",
        "status_message": "Midtrans account_linked notification"
    });
    let ack = app
        .call("POST", &path, None, &[], Some(linking.to_string()))
        .await;
    assert_eq!(ack.status, StatusCode::OK, "{}", ack.text);
    // a payment body without its signature is still refused
    let stripped = json!({ "order_id": "o-1", "transaction_status": "settlement" });
    let refused = app
        .call("POST", &path, None, &[], Some(stripped.to_string()))
        .await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);

    // and an account that never got a signature secret key keeps verifying with its server key
    // (covered by midtrans_webhook_settles_once_and_rejects_bad_input)
}

#[sqlx::test(migrations = "./migrations")]
async fn xendit_webhook_requires_callback_token_and_maps_channel(pool: PgPool) {
    let app = App::new(pool).await;
    let (account, _) = app
        .provider(
            "xendit",
            "sandbox",
            json!({ "secret_key": "xnd_development_x", "callback_token": "cb-token-1" }),
        )
        .await;
    let (_pid, _s, key, _) = app.product_with_keys("Shop", None).await;
    let p = app
        .product(
            "POST",
            "/v1/payments",
            &key,
            Some(json!({ "order_id": "X1", "amount": 75000, "provider": "xendit" })),
        )
        .await;
    assert_eq!(p.status, StatusCode::CREATED, "{}", p.text);
    let id = p.json["id"].as_str().unwrap().to_string();
    let body = json!({
        "event": "payment.capture", "business_id": "b",
        "data": { "reference_id": id, "status": "SUCCEEDED", "channel_code": "QRIS", "request_amount": 75000,
                  "captures": [{ "capture_timestamp": "2026-11-26T07:28:37.2Z" }] }
    })
    .to_string();
    let path = format!("/hooks/{account}");

    let bad = app
        .call(
            "POST",
            &path,
            None,
            &[("x-callback-token", "nope")],
            Some(body.clone()),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    let missing = app.call("POST", &path, None, &[], Some(body.clone())).await;
    assert_eq!(missing.status, StatusCode::UNAUTHORIZED);
    let ok = app
        .call(
            "POST",
            &path,
            None,
            &[("x-callback-token", "cb-token-1")],
            Some(body),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);

    let got = app
        .product("GET", &format!("/v1/payments/{id}"), &key, None)
        .await;
    assert_eq!(got.json["status"], "paid");
    assert_eq!(got.json["method"], "qris");
    // callback token is redacted in the stored headers
    let events = app.admin("GET", "/admin/webhook-events", None).await;
    assert!(events.text.contains("[redacted]"));
    assert!(!events.text.contains("cb-token-1"));
}

#[sqlx::test(migrations = "./migrations")]
async fn access_control_and_mode_isolation(pool: PgPool) {
    let app = App::new(pool).await;
    // admin surface
    assert_eq!(
        app.call("GET", "/admin/me", None, &[], None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.call("GET", "/admin/me", Some("wrong"), &[], None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.admin("GET", "/admin/me", None).await.status,
        StatusCode::OK
    );
    // product surface
    assert_eq!(
        app.call("GET", "/v1/payments", None, &[], None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.call("GET", "/v1/payments", Some("prk_test_deadbeef"), &[], None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    // unknown API paths answer with the JSON envelope, not the SPA
    let nf = app.call("GET", "/v1/nope", Some(ADMIN), &[], None).await;
    assert_eq!(nf.status, StatusCode::NOT_FOUND);

    let (pid, _s, sandbox, live) = app.product_with_keys("Shop", None).await;
    let (_pid2, _s2, other_sandbox, _) = app.product_with_keys("Other", None).await;

    // IDRX has no sandbox.
    let r = app
        .product(
            "POST",
            "/v1/payments",
            &sandbox,
            Some(json!({ "order_id": "I", "amount": 25000, "provider": "idrx" })),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.json["error"]["code"], "sandbox_unsupported");
    // No live account configured.
    let r = app
        .product(
            "POST",
            "/v1/payments",
            &live,
            Some(json!({ "order_id": "L", "amount": 1000 })),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.json["error"]["code"], "provider_unavailable");

    // A sandbox payment is invisible to the live key and to other products.
    let p = app
        .product(
            "POST",
            "/v1/payments",
            &sandbox,
            Some(json!({ "order_id": "S1", "amount": 1000 })),
        )
        .await;
    assert_eq!(p.status, StatusCode::CREATED, "{}", p.text);
    assert_eq!(
        p.json["provider"], "mock",
        "sandbox falls back to the simulator"
    );
    let id = p.json["id"].as_str().unwrap();
    assert_eq!(
        app.product("GET", &format!("/v1/payments/{id}"), &live, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.product("GET", &format!("/v1/payments/{id}"), &other_sandbox, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let list = app
        .product("GET", "/v1/payments", &other_sandbox, None)
        .await;
    assert_eq!(list.json["data"].as_array().unwrap().len(), 0);

    // Validation errors use the envelope.
    let bad = app
        .product(
            "POST",
            "/v1/payments",
            &sandbox,
            Some(json!({ "order_id": "", "amount": 0 })),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.json["error"]["code"], "invalid_request");
    let items = app
        .product("POST", "/v1/payments", &sandbox, Some(json!({ "order_id": "S2", "amount": 1000, "items": [{ "name": "x", "price": 1, "quantity": 1 }] })))
        .await;
    assert_eq!(items.status, StatusCode::BAD_REQUEST);

    // Revoked keys stop working immediately.
    let keys = app.admin("GET", "/admin/products", None).await;
    let key_id = keys.json["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(pid))
        .unwrap()["api_keys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        app.admin(
            "DELETE",
            &format!("/admin/products/{pid}/api-keys/{key_id}"),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.product("GET", "/v1/payments", &sandbox, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn late_payment_after_expiry_still_settles(pool: PgPool) {
    let app = App::new(pool.clone()).await;
    let key_s = "SB-Mid-server-late";
    let (account, _) = app
        .provider("midtrans", "sandbox", json!({ "server_key": key_s }))
        .await;
    let (_pid, _s, key, _) = app.product_with_keys("Shop", None).await;
    let p = app
        .product(
            "POST",
            "/v1/payments",
            &key,
            Some(json!({ "order_id": "E1", "amount": 50000, "provider": "midtrans" })),
        )
        .await;
    let id = p.json["id"].as_str().unwrap().to_string();

    sqlx::query("UPDATE payments SET expires_at = now() - interval '1 hour' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    worker::tick(&app.state).await.unwrap();
    let got = app
        .product("GET", &format!("/v1/payments/{id}"), &key, None)
        .await;
    assert_eq!(got.json["status"], "expired");

    // Customer pays at the very last minute; the provider's webhook still wins.
    let hook = midtrans_notification(&id, "50000.00", "settlement", key_s);
    assert_eq!(
        app.call("POST", &format!("/hooks/{account}"), None, &[], Some(hook))
            .await
            .status,
        StatusCode::OK
    );
    let got = app
        .product("GET", &format!("/v1/payments/{id}"), &key, None)
        .await;
    assert_eq!(got.json["status"], "paid");

    // Snap refuses to start a charge on an expired/settled payment.
    let token = token_of(got.json["snap_url"].as_str().unwrap()).to_string();
    let r = app
        .call(
            "POST",
            &format!("/snap/api/{token}/pay"),
            None,
            &[],
            Some(json!({ "method": "qris" }).to_string()),
        )
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn stats_and_recap_reflect_fees_and_filters(pool: PgPool) {
    let app = App::new(pool).await;
    let (_pid, _s, key, _) = app.product_with_keys("Shop", None).await;

    // 4 000 flat fee on BCA VA for the simulator account.
    let accounts = app.admin("GET", "/admin/providers", None).await;
    let mock_id = accounts.json["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["provider"] == "mock")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = app
        .admin(
            "PATCH",
            &format!("/admin/providers/{mock_id}"),
            Some(json!({ "fees": { "va_bca": { "fixed": 4000, "percent": 0 } } })),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);

    let mut tokens = vec![];
    for (order, amount, method) in [
        ("P1", 150000, "va_bca"),
        ("P2", 50000, "va_bca"),
        ("P3", 20000, "qris"),
    ] {
        let p = app
            .product(
                "POST",
                "/v1/payments",
                &key,
                Some(json!({ "order_id": order, "amount": amount, "method": method })),
            )
            .await;
        assert_eq!(p.status, StatusCode::CREATED, "{}", p.text);
        tokens.push(token_of(p.json["snap_url"].as_str().unwrap()).to_string());
    }
    for token in &tokens[..2] {
        let r = app
            .call(
                "POST",
                &format!("/snap/api/{token}/simulate"),
                None,
                &[],
                Some(json!({ "outcome": "paid" }).to_string()),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    }

    let s = app
        .admin("GET", "/admin/stats/summary?mode=sandbox", None)
        .await;
    assert_eq!(s.status, StatusCode::OK, "{}", s.text);
    assert_eq!(s.json["payments"]["total"], 3);
    assert_eq!(s.json["payments"]["paid"], 2);
    assert_eq!(s.json["payments"]["pending"], 1);
    assert_eq!(s.json["cash_in"]["count"], 2);
    assert_eq!(s.json["cash_in"]["gross"], 200000);
    assert_eq!(s.json["cash_in"]["fee"], 8000);
    assert_eq!(s.json["cash_in"]["net"], 192000);
    assert_eq!(s.json["cash_in"]["average"], 100000);
    assert_eq!(s.json["conversion_rate"], 1.0);
    // live mode is empty: no sandbox leakage
    let live = app
        .admin("GET", "/admin/stats/summary?mode=live", None)
        .await;
    assert_eq!(live.json["cash_in"]["count"], 0);

    let ts = app
        .admin(
            "GET",
            "/admin/stats/timeseries?mode=sandbox&from=2020-01-01&to=2020-01-03",
            None,
        )
        .await;
    assert_eq!(
        ts.json["data"].as_array().unwrap().len(),
        3,
        "zero-filled days"
    );
    let today = app
        .admin("GET", "/admin/stats/timeseries?mode=sandbox", None)
        .await;
    let rows = today.json["data"].as_array().unwrap();
    assert_eq!(rows.len(), 30);
    assert_eq!(
        rows.iter()
            .map(|r| r["count"].as_i64().unwrap())
            .sum::<i64>(),
        2
    );
    assert_eq!(
        rows.iter()
            .map(|r| r["created"].as_i64().unwrap())
            .sum::<i64>(),
        3
    );

    let by_method = app
        .admin("GET", "/admin/stats/breakdown?mode=sandbox&by=method", None)
        .await;
    let bca = by_method.json["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "va_bca")
        .unwrap();
    assert_eq!(bca["paid"], 2);
    assert_eq!(bca["gross"], 200000);
    assert_eq!(bca["label"], "BCA Virtual Account");
    let by_provider = app
        .admin(
            "GET",
            "/admin/stats/breakdown?mode=sandbox&by=provider",
            None,
        )
        .await;
    assert_eq!(by_provider.json["data"][0]["key"], "mock");
    assert_eq!(
        app.admin("GET", "/admin/stats/breakdown?by=bogus", None)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    let page1 = app
        .admin("GET", "/admin/cash-ins?mode=sandbox&limit=1", None)
        .await;
    assert_eq!(page1.json["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        page1.json["totals"]["count"], 2,
        "totals cover the whole filter"
    );
    let cursor = page1.json["next_cursor"].as_str().unwrap();
    let page2 = app
        .admin(
            "GET",
            &format!("/admin/cash-ins?mode=sandbox&limit=1&cursor={cursor}"),
            None,
        )
        .await;
    assert_eq!(page2.json["data"].as_array().unwrap().len(), 1);
    assert!(page2.json["next_cursor"].is_null());
    assert_ne!(page1.json["data"][0]["id"], page2.json["data"][0]["id"]);

    let csv = app
        .admin("GET", "/admin/cash-ins/export.csv?mode=sandbox", None)
        .await;
    assert_eq!(csv.status, StatusCode::OK);
    assert!(
        csv.headers
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/csv")
    );
    let lines: Vec<&str> = csv.text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("paid_at,order_id,payment_id"));
    assert!(lines[1].contains("va_bca") && lines[1].contains("sandbox"));

    let q = app
        .admin("GET", "/admin/payments?mode=sandbox&q=P3", None)
        .await;
    assert_eq!(q.json["data"].as_array().unwrap().len(), 1);
    assert_eq!(q.json["data"][0]["product"]["name"], "Shop");
}

/// Clients are told apart by `X-Forwarded-For`, as behind the e2e proxy.
fn trust_forwarded_for(c: &mut Config) {
    c.trusted_proxy_header = Some(HeaderName::from_static("x-forwarded-for"));
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_lockout_refuses_one_client_and_leaves_others_alone(pool: PgPool) {
    let app = App::with_config(pool, trust_forwarded_for).await;
    let attacker = [("x-forwarded-for", "198.51.100.7")];
    for attempt in 1..=10 {
        let r = app
            .call(
                "GET",
                "/admin/me",
                Some("wrong-token-0000000000"),
                &attacker,
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "attempt {attempt}");
        assert_eq!(
            r.json["error"]["message"].as_str(),
            Some("invalid credentials")
        );
    }

    // The eleventh request is refused before its token is checked.
    let locked = app
        .call(
            "GET",
            "/admin/me",
            Some("wrong-token-0000000000"),
            &attacker,
            None,
        )
        .await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = locked.headers["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=300).contains(&retry), "retry-after {retry}");
    assert_eq!(locked.json["error"]["code"].as_str(), Some("rate_limited"));
    assert_eq!(
        locked.json["error"]["message"].as_str(),
        Some(format!("Too many requests. Try again in {retry} seconds.").as_str())
    );

    // The right token from the locked client is refused too: a success must not reopen the lock.
    let valid = app
        .call("GET", "/admin/me", Some(ADMIN), &attacker, None)
        .await;
    assert_eq!(valid.status, StatusCode::TOO_MANY_REQUESTS);

    // Another client still gets 401 for a wrong token and 200 for the right one.
    let other = [("x-forwarded-for", "198.51.100.8")];
    let wrong = app
        .call(
            "GET",
            "/admin/me",
            Some("wrong-token-0000000000"),
            &other,
            None,
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    let right = app
        .call("GET", "/admin/me", Some(ADMIN), &other, None)
        .await;
    assert_eq!(right.status, StatusCode::OK);

    // Product keys are counted separately: the admin lockout does not reach /v1.
    let v1 = app.call("GET", "/v1/payments", None, &attacker, None).await;
    assert_eq!(v1.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn product_key_failures_lock_out_one_client(pool: PgPool) {
    let app = App::with_config(pool, trust_forwarded_for).await;
    let (_, _, sandbox, _) = app.product_with_keys("Shop", None).await;
    let attacker = [("x-forwarded-for", "198.51.100.30")];
    for attempt in 1..=20 {
        let r = app
            .call(
                "GET",
                "/v1/payments",
                Some("prk_test_deadbeef"),
                &attacker,
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "attempt {attempt}");
    }
    let locked = app
        .call("GET", "/v1/payments", Some(&sandbox), &attacker, None)
        .await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(locked.headers.contains_key("retry-after"));

    let other = [("x-forwarded-for", "198.51.100.31")];
    let ok = app
        .call("GET", "/v1/payments", Some(&sandbox), &other, None)
        .await;
    assert_eq!(ok.status, StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn snap_requests_are_rate_limited_per_client(pool: PgPool) {
    let app = App::with_config(pool, |c| {
        trust_forwarded_for(c);
        c.rate.snap_per_min = 3;
        c.rate.snap_pay_per_min = 1;
    })
    .await;
    let client = [("x-forwarded-for", "192.0.2.10")];
    for _ in 0..3 {
        let r = app
            .call("GET", "/snap/api/no-such-token", None, &client, None)
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND);
    }
    let limited = app
        .call("GET", "/snap/api/no-such-token", None, &client, None)
        .await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(limited.headers.contains_key("retry-after"));
    assert_eq!(limited.json["error"]["code"].as_str(), Some("rate_limited"));

    let other = [("x-forwarded-for", "192.0.2.11")];
    let ok = app
        .call("GET", "/snap/api/no-such-token", None, &other, None)
        .await;
    assert_eq!(ok.status, StatusCode::NOT_FOUND);
    // the admin API is counted under its own budget
    let admin = app
        .call("GET", "/admin/me", Some(ADMIN), &client, None)
        .await;
    assert_eq!(admin.status, StatusCode::OK);

    // pay and simulate also spend the smaller per-minute budget
    let payer = [("x-forwarded-for", "192.0.2.20")];
    let body = json!({ "method": "bank_transfer" }).to_string();
    let first = app
        .call(
            "POST",
            "/snap/api/no-such-token/pay",
            None,
            &payer,
            Some(body.clone()),
        )
        .await;
    assert_ne!(first.status, StatusCode::TOO_MANY_REQUESTS);
    let second = app
        .call(
            "POST",
            "/snap/api/no-such-token/pay",
            None,
            &payer,
            Some(body),
        )
        .await;
    assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrations = "./migrations")]
async fn logo_route_serves_only_known_codes_and_never_the_spa(pool: PgPool) {
    let app = App::new(pool).await;
    // not a code we know: nothing is fetched
    let unknown = app
        .call("GET", "/assets/logo/evil.example.com", None, &[], None)
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    // a known code is an image when the logo service answers and a plain 404 when it does not
    // (the test machine may be offline); it is never the SPA's index.html
    let known = app.call("GET", "/assets/logo/bca", None, &[], None).await;
    assert!(
        known.status == StatusCode::OK || known.status == StatusCode::NOT_FOUND,
        "{}",
        known.status
    );
    assert!(!known.text.contains("<html"), "{}", known.text);
}

#[sqlx::test(migrations = "./migrations")]
async fn mayar_and_paypal_accounts_are_catalogued_and_can_be_a_product_default(pool: PgPool) {
    let app = App::new(pool).await;

    // The client secret is required. The rupiah rate is not: without one the ECB reference rate
    // is used.
    let missing_secret = app
        .admin(
            "POST",
            "/admin/providers",
            Some(json!({
                "provider": "paypal", "mode": "sandbox",
                "credentials": { "client_id": "cid" }
            })),
        )
        .await;
    assert_eq!(missing_secret.status, StatusCode::BAD_REQUEST);
    assert!(
        missing_secret.text.contains("client_secret"),
        "{}",
        missing_secret.text
    );
    app.provider(
        "paypal",
        "live",
        json!({ "client_id": "cid", "client_secret": "secret" }),
    )
    .await;

    app.provider("mayar", "sandbox", json!({ "api_key": "mayar-key-1234" }))
        .await;
    let (paypal_id, hook) = app
        .provider(
            "paypal",
            "sandbox",
            json!({
                "client_id": "cid", "client_secret": "secret-value-1234",
                "idr_rate": "16000", "webhook_id": "WH-1"
            }),
        )
        .await;
    assert_eq!(hook, format!("http://pay.test/hooks/{paypal_id}"));

    // Secrets stay masked in the account view; the rate and ids are plain.
    let list = app.admin("GET", "/admin/providers", None).await;
    let paypal = list.json["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["provider"] == "paypal" && a["mode"] == "sandbox")
        .unwrap();
    assert_eq!(paypal["credentials"]["idr_rate"], "16000");
    assert_ne!(paypal["credentials"]["client_secret"], "secret-value-1234");

    // Both are hosted-only: they appear in the catalogue with no standard methods.
    let (pid, _, sandbox_key, _) = app.product_with_keys("Shop", None).await;
    let methods = app.product("GET", "/v1/methods", &sandbox_key, None).await;
    for provider in ["mayar", "paypal"] {
        let entry = methods.json["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["provider"] == provider)
            .unwrap_or_else(|| panic!("{provider} missing from {}", methods.text));
        assert_eq!(entry["checkout_provider"], true);
        assert_eq!(entry["methods"], json!([]));
    }

    // A method on a hosted-only provider is refused before any provider call.
    let refused = app
        .product(
            "POST",
            "/v1/payments",
            &sandbox_key,
            Some(json!({ "order_id": "M-1", "amount": 50000, "provider": "mayar", "method": "va_bca" })),
        )
        .await;
    assert_eq!(
        refused.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        refused.text
    );
    assert_eq!(refused.json["error"]["code"], "method_unsupported");

    // The provider check constraint accepts them as a product default.
    let patched = app
        .admin(
            "PATCH",
            &format!("/admin/products/{pid}"),
            Some(json!({ "default_provider": "paypal" })),
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text);
    assert_eq!(patched.json["default_provider"], "paypal");
}

#[sqlx::test(migrations = "./migrations")]
async fn mayar_and_paypal_webhooks_refuse_what_they_cannot_verify(pool: PgPool) {
    let app = App::new(pool).await;
    let (mayar, _) = app
        .provider("mayar", "sandbox", json!({ "api_key": "mayar-key" }))
        .await;
    let (paypal, _) = app
        .provider(
            "paypal",
            "sandbox",
            json!({ "client_id": "cid", "client_secret": "s", "idr_rate": "16000" }),
        )
        .await;
    let (paypal_with_id, _) = app
        .provider(
            "paypal",
            "live",
            json!({ "client_id": "cid", "client_secret": "s", "idr_rate": "16000", "webhook_id": "WH-1" }),
        )
        .await;

    // PayPal without a webhook id cannot verify anything.
    let event = json!({ "event_type": "CHECKOUT.ORDER.APPROVED", "resource": { "id": "ORD1" } });
    let no_id = app
        .call(
            "POST",
            &format!("/hooks/{paypal}"),
            None,
            &[],
            Some(event.to_string()),
        )
        .await;
    assert_eq!(no_id.status, StatusCode::UNAUTHORIZED, "{}", no_id.text);

    // With one, an event that carries no PayPal transmission headers is still refused.
    let unsigned = app
        .call(
            "POST",
            &format!("/hooks/{paypal_with_id}"),
            None,
            &[],
            Some(event.to_string()),
        )
        .await;
    assert_eq!(
        unsigned.status,
        StatusCode::UNAUTHORIZED,
        "{}",
        unsigned.text
    );
    assert!(unsigned.text.contains("paypal-"), "{}", unsigned.text);

    let garbage = app
        .call(
            "POST",
            &format!("/hooks/{paypal}"),
            None,
            &[],
            Some("nope".into()),
        )
        .await;
    assert_eq!(garbage.status, StatusCode::BAD_REQUEST);

    // Mayar events that are not a payment on a payment link are acknowledged and ignored
    // without asking Mayar anything.
    for body in [
        json!({ "event": "payment.reminder", "data": { "productId": "7c9d2e1f-3a4b-4c5d-8e6f-1a2b3c4d5e6f" } }),
        json!({ "event": "membership.memberExpired", "data": {} }),
        json!({ "event": "payment.received", "data": { "productId": "../balances" } }),
    ] {
        let r = app
            .call(
                "POST",
                &format!("/hooks/{mayar}"),
                None,
                &[],
                Some(body.to_string()),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{} for {body}", r.text);
    }
    let garbage = app
        .call(
            "POST",
            &format!("/hooks/{mayar}"),
            None,
            &[],
            Some("nope".into()),
        )
        .await;
    assert_eq!(garbage.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn snap_return_and_cancel_land_on_the_snap_page(pool: PgPool) {
    let app = App::new(pool).await;
    let (_pid, _, key, _) = app.product_with_keys("Shop", None).await;
    let created = app
        .product(
            "POST",
            "/v1/payments",
            &key,
            Some(json!({ "order_id": "R-1", "amount": 50000, "provider": "mock", "method": "va_bca" })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let snap_url = created.json["snap_url"].as_str().unwrap().to_string();
    let token = token_of(&snap_url);

    let back = app
        .call("GET", &format!("/snap/api/{token}/return"), None, &[], None)
        .await;
    assert_eq!(back.status, StatusCode::SEE_OTHER);
    assert_eq!(back.headers.get("location").unwrap(), snap_url.as_str());

    // Backing out keeps the payer on the page but stops the automatic hop to the provider.
    let cancelled = app
        .call("GET", &format!("/snap/api/{token}/cancel"), None, &[], None)
        .await;
    assert_eq!(cancelled.status, StatusCode::SEE_OTHER);
    assert_eq!(
        cancelled.headers.get("location").unwrap(),
        format!("{snap_url}?paused=1").as_str()
    );

    for tail in ["return", "cancel"] {
        let unknown = app
            .call(
                "GET",
                &format!("/snap/api/not-a-token/{tail}"),
                None,
                &[],
                None,
            )
            .await;
        assert_eq!(unknown.status, StatusCode::NOT_FOUND, "{tail}");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn mayar_webhook_token_is_required_when_set_and_never_stored(pool: PgPool) {
    let app = App::new(pool).await;
    let token = "wh-token-0123456789abcdef";
    let (mayar, _) = app
        .provider(
            "mayar",
            "sandbox",
            json!({ "api_key": "mayar-key", "webhook_token": token }),
        )
        .await;
    let url = format!("/hooks/{mayar}");
    // an authentic event that needs no call to Mayar: a payment reminder
    let reminder = json!({
        "event": "payment.reminder",
        "data": { "productId": "7c9d2e1f-3a4b-4c5d-8e6f-1a2b3c4d5e6f" }
    })
    .to_string();

    let without = app
        .call("POST", &url, None, &[], Some(reminder.clone()))
        .await;
    assert_eq!(without.status, StatusCode::UNAUTHORIZED, "{}", without.text);
    assert!(without.text.contains("content-type"), "{}", without.text);
    let wrong = app
        .call(
            "POST",
            &url,
            None,
            &[("x-mayar-token", "not-the-token")],
            Some(reminder.clone()),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED, "{}", wrong.text);

    // the token is accepted in whichever header carries it
    for header in [("x-mayar-token", token), ("some-other-name", token)] {
        let ok = app
            .call("POST", &url, None, &[header], Some(reminder.clone()))
            .await;
        assert_eq!(ok.status, StatusCode::OK, "{} {}", header.0, ok.text);
    }

    // the secret is not written to the inbox, whatever the header was called
    let events = app.admin("GET", "/admin/webhook-events", None).await;
    assert!(events.text.contains("[redacted]"), "{}", events.text);
    assert!(!events.text.contains(token), "{}", events.text);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_product_can_combine_providers_through_its_payment_options(pool: PgPool) {
    use payrouters_server::services::options;
    use payrouters_types::{Checkout, Method, Mode, Provider};

    let app = App::new(pool).await;
    app.provider(
        "midtrans",
        "sandbox",
        json!({ "server_key": "SB-Mid-server-k" }),
    )
    .await;
    app.provider(
        "xendit",
        "sandbox",
        json!({ "secret_key": "xnd_development_k", "callback_token": "cb" }),
    )
    .await;
    app.provider(
        "paypal",
        "sandbox",
        json!({ "client_id": "cid", "client_secret": "s", "idr_rate": "16000" }),
    )
    .await;
    let (pid, _, key, _) = app.product_with_keys("Shop", None).await;
    let uri = format!("/admin/products/{pid}/payment-options");

    // nothing chosen yet; the dashboard is told what it can choose from
    let none = app.admin("GET", &uri, None).await;
    assert_eq!(none.status, StatusCode::OK, "{}", none.text);
    assert_eq!(none.json["data"], json!([]));
    let providers: Vec<&str> = none.json["available"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["provider"].as_str().unwrap())
        .collect();
    assert_eq!(providers, ["midtrans", "xendit", "paypal", "mock"]);
    let paypal = &none.json["available"][2];
    assert_eq!(paypal["checkout"], true);
    assert_eq!(paypal["methods"], json!([]));
    assert_eq!(paypal["modes"], json!(["sandbox"]));

    // QRIS on Midtrans, virtual accounts on Xendit, PayPal for overseas customers
    let chosen = json!({ "options": [
        { "option": "qris", "provider": "midtrans" },
        { "option": "va_bca", "provider": "xendit" },
        { "option": "va_bni", "provider": "xendit" },
        { "option": "checkout:paypal", "provider": "paypal" }
    ] });
    let saved = app.admin("PUT", &uri, Some(chosen.clone())).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text);
    assert_eq!(saved.json["data"], chosen["options"]);
    let product = app.admin("GET", "/admin/products", None).await;
    assert_eq!(
        product.json["data"][0]["payment_options"],
        chosen["options"]
    );

    // refused: a method on a provider that cannot serve it, a method twice, an unknown product
    for (bad, why) in [
        (
            json!({ "options": [{ "option": "qris", "provider": "paypal" }] }),
            "unsupported",
        ),
        (
            json!({ "options": [
                { "option": "qris", "provider": "midtrans" },
                { "option": "qris", "provider": "xendit" }
            ] }),
            "twice",
        ),
    ] {
        let r = app.admin("PUT", &uri, Some(bad)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{why}: {}", r.text);
    }
    let gone = app
        .admin(
            "PUT",
            &format!("/admin/products/{}/payment-options", uuid::Uuid::nil()),
            Some(json!({ "options": [] })),
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    // a refused save changed nothing
    let still = app.admin("GET", &uri, None).await;
    assert_eq!(still.json["data"], chosen["options"]);

    // routing, without any provider being contacted
    let product_id: uuid::Uuid = pid.parse().unwrap();
    let route = |provider, method, checkout| {
        let state = app.state.clone();
        async move {
            options::route(
                &state,
                product_id,
                Mode::Sandbox,
                provider,
                method,
                checkout,
            )
            .await
        }
    };
    let r = route(None, Some(Method::Qris), Checkout::Snap)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((r.provider, r.hosted_now), (Provider::Midtrans, false));
    let r = route(None, Some(Method::VaBni), Checkout::Snap)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.provider, Provider::Xendit);
    assert!(
        route(None, Some(Method::VaMandiri), Checkout::Snap)
            .await
            .is_err()
    );
    let r = route(None, None, Checkout::Provider)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((r.provider, r.hosted_now), (Provider::Paypal, true));
    // an explicit provider is not routed by the options
    assert!(
        route(Some(Provider::Midtrans), None, Checkout::Snap)
            .await
            .unwrap()
            .is_none()
    );
    // the same product in live mode: none of its providers has a live account
    let live = options::route(
        &app.state,
        product_id,
        Mode::Live,
        None,
        None,
        Checkout::Snap,
    )
    .await;
    assert!(live.is_err());

    // a payment without a method waits for the customer on the Snap page
    let created = app
        .product(
            "POST",
            "/v1/payments",
            &key,
            Some(json!({ "order_id": "OPT-1", "amount": 129000 })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    assert!(created.json["instructions"].is_null());
    assert_eq!(created.json["provider"], "midtrans");
    let token = token_of(created.json["snap_url"].as_str().unwrap());
    let view = app
        .call("GET", &format!("/snap/api/{token}"), None, &[], None)
        .await;
    let methods: Vec<&str> = view.json["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["code"].as_str().unwrap())
        .collect();
    assert_eq!(methods, ["qris", "va_bca", "va_bni"]);
    assert_eq!(
        view.json["checkouts"],
        json!([{ "provider": "paypal", "label": "PayPal" }])
    );
    // what the product does not offer is refused before any provider is asked
    for (body, code) in [
        (json!({ "method": "va_mandiri" }), "method_unsupported"),
        (json!({ "provider": "mayar" }), "checkout_unsupported"),
        (json!({ "provider": "midtrans" }), "checkout_unsupported"),
    ] {
        let r = app
            .call(
                "POST",
                &format!("/snap/api/{token}/pay"),
                None,
                &[],
                Some(body.to_string()),
            )
            .await;
        assert_eq!(r.json["error"]["code"], code, "{}", r.text);
    }
    // replaying the order is still a replay, whichever provider the customer ends up on
    let replay = app
        .product(
            "POST",
            "/v1/payments",
            &key,
            Some(json!({ "order_id": "OPT-1", "amount": 129000 })),
        )
        .await;
    assert_eq!(replay.status, StatusCode::OK, "{}", replay.text);

    // `GET /v1/methods` shows what this product offers, grouped by provider
    let catalog = app.product("GET", "/v1/methods", &key, None).await;
    let grouped: Vec<(String, bool, usize)> = catalog.json["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["provider"].as_str().unwrap().to_string(),
                p["checkout_provider"].as_bool().unwrap(),
                p["methods"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(
        grouped,
        [
            ("midtrans".to_string(), false, 1),
            ("xendit".to_string(), false, 2),
            ("paypal".to_string(), true, 0)
        ]
    );

    // clearing returns to the single-provider behaviour
    let cleared = app.admin("PUT", &uri, Some(json!({ "options": [] }))).await;
    assert_eq!(cleared.json["data"], json!([]));
    let all = app.product("GET", "/v1/methods", &key, None).await;
    assert_eq!(all.json["data"].as_array().unwrap().len(), 4);
}
