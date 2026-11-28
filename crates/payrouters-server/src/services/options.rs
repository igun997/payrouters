//! Per-product payment options: which methods a product offers and which provider serves each one,
//! so a single product can combine providers (QRIS on Midtrans, virtual accounts on Xendit, PayPal
//! for overseas customers).
//!
//! An option is a standard method (`qris`, `va_bca`, ...) served by one provider, or a provider's own
//! hosted checkout page (`checkout:paypal`). A product without options keeps the single-provider
//! behaviour (the `provider` of the request, the product default, then the first enabled provider).
//! Options are chosen per product, not per mode: a mode uses the options whose provider has an
//! enabled account in it, and sandbox falls back to the single-provider behaviour (the mock
//! simulator) when none do, so a product stays testable before its live accounts exist.

use std::collections::HashSet;

use payrouters_types::{Checkout, Method, MethodInfo, Mode, Provider};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::accounts;
use crate::error::{AppError, Result};
use crate::providers::{self, availability};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Method(Method),
    /// The provider's hosted checkout page.
    Checkout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductOption {
    pub kind: Kind,
    pub provider: Provider,
}

impl ProductOption {
    /// The stored and wire form: a method code, or `checkout:<provider>`.
    pub fn key(&self) -> String {
        match self.kind {
            Kind::Method(m) => m.as_str().to_string(),
            Kind::Checkout => format!("checkout:{}", self.provider),
        }
    }

    /// An option as sent by the dashboard or API, checked against what the provider can do.
    pub fn parse(key: &str, provider: Provider) -> std::result::Result<ProductOption, String> {
        let kind = match key.strip_prefix("checkout:") {
            Some(p) if p == provider.as_str() => Kind::Checkout,
            Some(p) => {
                return Err(format!(
                    "option `{key}` is the checkout of {p}, not of {provider}"
                ));
            }
            None => Kind::Method(
                key.parse::<Method>()
                    .map_err(|_| format!("unknown payment option `{key}`"))?,
            ),
        };
        let info = providers::adapter(provider).info();
        match kind {
            Kind::Method(m) if !info.methods.contains(&m) => {
                Err(format!("{} does not support {m}", provider.label()))
            }
            Kind::Checkout if !info.supports_provider_checkout => Err(format!(
                "{} has no provider-hosted checkout",
                provider.label()
            )),
            _ => Ok(ProductOption { kind, provider }),
        }
    }
}

/// An option whose provider has an enabled account in the mode, and that account.
#[derive(Debug, Clone, Copy)]
pub struct Usable {
    pub option: ProductOption,
    pub account_id: Uuid,
}

fn from_row(option: &str, provider: &str) -> Option<ProductOption> {
    ProductOption::parse(option, provider.parse().ok()?).ok()
}

/// The product's options in the order the dashboard set them. Empty: not configured.
pub async fn list(state: &AppState, product_id: Uuid) -> Result<Vec<ProductOption>> {
    Ok(list_for(state, &[product_id])
        .await?
        .into_iter()
        .map(|(_, o)| o)
        .collect())
}

/// The options of several products, for the product list.
pub async fn list_for(
    state: &AppState,
    product_ids: &[Uuid],
) -> Result<Vec<(Uuid, ProductOption)>> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT product_id, option, provider FROM product_payment_options
         WHERE product_id = ANY($1) ORDER BY position, option",
    )
    .bind(product_ids)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, option, provider)| from_row(&option, &provider).map(|o| (id, o)))
        .collect())
}

/// Replace all of a product's options. An empty list clears them (single-provider behaviour).
pub async fn replace(state: &AppState, product_id: Uuid, options: &[ProductOption]) -> Result<()> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM product_payment_options WHERE product_id = $1")
        .bind(product_id)
        .execute(&mut *tx)
        .await?;
    for (position, o) in options.iter().enumerate() {
        sqlx::query(
            "INSERT INTO product_payment_options (product_id, option, provider, position)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(product_id)
        .bind(o.key())
        .bind(o.provider.as_str())
        .bind(position as i32)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The product's options that can be served in `mode`. `None`: the product has no options.
pub async fn usable(state: &AppState, product_id: Uuid, mode: Mode) -> Result<Option<Vec<Usable>>> {
    let all = list(state, product_id).await?;
    if all.is_empty() {
        return Ok(None);
    }
    let accounts = accounts::enabled_accounts(state, mode).await?;
    Ok(Some(
        all.into_iter()
            .filter_map(|option| {
                accounts
                    .iter()
                    .find(|(p, _)| *p == option.provider)
                    .map(|(_, account_id)| Usable {
                        option,
                        account_id: *account_id,
                    })
            })
            .collect(),
    ))
}

/// Where a new payment is created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Routing {
    pub provider: Provider,
    /// The hosted checkout is created now: there is nothing for the customer to choose.
    pub hosted_now: bool,
}

type RouteError = (&'static str, String);

/// Pure routing over the usable options of a configured product.
pub fn route_from(
    options: &[Usable],
    method: Option<Method>,
    checkout: Checkout,
) -> std::result::Result<Routing, RouteError> {
    match (method, checkout) {
        (Some(m), _) => options
            .iter()
            .find(|u| u.option.kind == Kind::Method(m))
            .map(|u| Routing {
                provider: u.option.provider,
                hosted_now: false,
            })
            .ok_or((
                "method_not_enabled",
                format!("this product does not offer {m}"),
            )),
        (None, Checkout::Provider) => options
            .iter()
            .find(|u| u.option.kind == Kind::Checkout)
            .map(|u| Routing {
                provider: u.option.provider,
                hosted_now: true,
            })
            .ok_or((
                "checkout_unsupported",
                "this product has no provider-hosted checkout enabled".to_string(),
            )),
        (None, Checkout::Snap) => {
            let first = &options[0];
            Ok(Routing {
                provider: first.option.provider,
                // a single hosted option leaves nothing to pick
                hosted_now: options.len() == 1 && first.option.kind == Kind::Checkout,
            })
        }
    }
}

/// `Ok(None)`: not routed by options (an explicit `provider`, or a product without options, or a
/// sandbox product none of whose providers has an account). `Err`: configured, but this payment
/// cannot be served from its options.
pub async fn route(
    state: &AppState,
    product_id: Uuid,
    mode: Mode,
    explicit_provider: Option<Provider>,
    method: Option<Method>,
    checkout: Checkout,
) -> Result<Option<Routing>> {
    if explicit_provider.is_some() {
        return Ok(None);
    }
    let Some(options) = usable(state, product_id, mode).await? else {
        return Ok(None);
    };
    if options.is_empty() {
        return if mode == Mode::Sandbox {
            Ok(None)
        } else {
            Err(AppError::unprocessable(
                "provider_unavailable",
                format!(
                    "none of this product's payment options has an enabled provider account in {mode} mode"
                ),
            ))
        };
    }
    route_from(&options, method, checkout)
        .map(Some)
        .map_err(|(code, message)| AppError::unprocessable(code, message))
}

/// What the Snap page offers: methods (each with the provider that serves it) and hosted checkouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub methods: Vec<(Method, Provider)>,
    pub checkouts: Vec<Provider>,
}

/// The picker over usable options. A method its account has been seen to refuse is left out while
/// anything else remains; when everything has been refused all of it stays, so the customer can
/// still pick and be sent to a hosted checkout.
pub fn picker_with(options: &[Usable], refused: impl Fn(Uuid, Method) -> bool) -> Picker {
    let all = |skip_refused: bool| {
        let mut p = Picker {
            methods: vec![],
            checkouts: vec![],
        };
        for u in options {
            match u.option.kind {
                Kind::Method(m) if skip_refused && refused(u.account_id, m) => {}
                Kind::Method(m) => p.methods.push((m, u.option.provider)),
                Kind::Checkout => p.checkouts.push(u.option.provider),
            }
        }
        p
    };
    let live = all(true);
    if live.methods.is_empty() && live.checkouts.is_empty() {
        all(false)
    } else {
        live
    }
}

pub fn picker(options: &[Usable]) -> Picker {
    picker_with(options, availability::is_inactive)
}

/// The providers a configured product uses in a mode, in the order of the provider list: whether the
/// product offers that provider's hosted checkout, and which standard methods it serves. This is
/// what `GET /v1/methods` reports for such a product.
pub fn catalog(options: &[Usable]) -> Vec<(Provider, bool, Vec<Method>)> {
    providers::PROVIDER_ORDER
        .into_iter()
        .filter_map(|provider| {
            let mine: Vec<&Usable> = options
                .iter()
                .filter(|u| u.option.provider == provider)
                .collect();
            if mine.is_empty() {
                return None;
            }
            let checkout = mine.iter().any(|u| u.option.kind == Kind::Checkout);
            let methods = mine
                .iter()
                .filter_map(|u| match u.option.kind {
                    Kind::Method(m) => Some(m),
                    Kind::Checkout => None,
                })
                .collect();
            Some((provider, checkout, methods))
        })
        .collect()
}

/// An option as the dashboard and the API write it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionInput {
    /// A standard method code (`qris`, `va_bca`, ...) or `checkout:<provider>`.
    pub option: String,
    pub provider: Provider,
}

impl From<&ProductOption> for OptionInput {
    fn from(o: &ProductOption) -> OptionInput {
        OptionInput {
            option: o.key(),
            provider: o.provider,
        }
    }
}

pub const MAX_OPTIONS: usize = 40;

/// Validate a submitted list: every option supported by its provider, none repeated.
pub fn parse_inputs(inputs: &[OptionInput]) -> std::result::Result<Vec<ProductOption>, String> {
    if inputs.len() > MAX_OPTIONS {
        return Err(format!("at most {MAX_OPTIONS} payment options"));
    }
    let mut seen = HashSet::new();
    inputs
        .iter()
        .map(|i| {
            let o = ProductOption::parse(&i.option, i.provider)?;
            if !seen.insert(o.key()) {
                return Err(format!(
                    "`{}` is listed twice: a method is served by one provider",
                    o.key()
                ));
            }
            Ok(o)
        })
        .collect()
}

/// A provider that has an enabled account, with what it can serve: what the dashboard offers.
#[derive(Debug, Clone, Serialize)]
pub struct AvailableProvider {
    pub provider: Provider,
    pub label: &'static str,
    /// The modes in which the provider has an enabled account.
    pub modes: Vec<Mode>,
    pub methods: Vec<MethodInfo>,
    pub checkout: bool,
}

pub async fn available(state: &AppState) -> Result<Vec<AvailableProvider>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT provider, mode FROM provider_accounts WHERE enabled ORDER BY provider, mode",
    )
    .fetch_all(&state.pool)
    .await?;
    let accounts: Vec<(Provider, Mode)> = rows
        .into_iter()
        .filter_map(|(p, m)| Some((p.parse().ok()?, m.parse().ok()?)))
        .collect();
    Ok(providers::PROVIDER_ORDER
        .into_iter()
        .filter_map(|provider| {
            let modes: Vec<Mode> = accounts
                .iter()
                .filter(|(p, _)| *p == provider)
                .map(|(_, m)| *m)
                .collect();
            if modes.is_empty() {
                return None;
            }
            let info = providers::adapter(provider).info();
            Some(AvailableProvider {
                provider,
                label: provider.label(),
                modes,
                methods: info.methods.iter().map(|m| m.info()).collect(),
                checkout: info.supports_provider_checkout,
            })
        })
        .collect())
}

/// A product's options together with what could be chosen: the body of the payment-options API.
#[derive(Debug, Clone, Serialize)]
pub struct OptionsView {
    pub data: Vec<OptionInput>,
    pub available: Vec<AvailableProvider>,
}

async fn ensure_product(state: &AppState, product_id: Uuid) -> Result<()> {
    let found: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM products WHERE id = $1 AND archived_at IS NULL")
            .bind(product_id)
            .fetch_optional(&state.pool)
            .await?;
    found
        .map(|_| ())
        .ok_or_else(|| AppError::not_found("product"))
}

pub async fn view(state: &AppState, product_id: Uuid) -> Result<OptionsView> {
    ensure_product(state, product_id).await?;
    Ok(OptionsView {
        data: list(state, product_id)
            .await?
            .iter()
            .map(OptionInput::from)
            .collect(),
        available: available(state).await?,
    })
}

/// Replace a product's options (an empty list clears them) and return the result.
pub async fn set(
    state: &AppState,
    product_id: Uuid,
    inputs: &[OptionInput],
) -> Result<OptionsView> {
    ensure_product(state, product_id).await?;
    let parsed = parse_inputs(inputs).map_err(AppError::bad_request)?;
    replace(state, product_id, &parsed).await?;
    view(state, product_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::new_id;

    fn opt(key: &str, provider: Provider) -> Usable {
        Usable {
            option: ProductOption::parse(key, provider).unwrap(),
            account_id: new_id(),
        }
    }

    fn mixed() -> Vec<Usable> {
        vec![
            opt("qris", Provider::Midtrans),
            opt("va_bca", Provider::Xendit),
            opt("checkout:paypal", Provider::Paypal),
        ]
    }

    #[test]
    fn options_are_checked_against_what_the_provider_can_do() {
        assert!(ProductOption::parse("qris", Provider::Midtrans).is_ok());
        assert!(ProductOption::parse("checkout:paypal", Provider::Paypal).is_ok());
        // PayPal has no standard methods, IDRX no QRIS, Mock no hosted page
        assert!(ProductOption::parse("qris", Provider::Paypal).is_err());
        assert!(ProductOption::parse("qris", Provider::Idrx).is_err());
        assert!(ProductOption::parse("checkout:mock", Provider::Mock).is_err());
        // a checkout belongs to its own provider, and unknown keys are refused
        assert!(ProductOption::parse("checkout:xendit", Provider::Paypal).is_err());
        assert!(ProductOption::parse("bitcoin", Provider::Xendit).is_err());
        assert!(ProductOption::parse("checkout:", Provider::Xendit).is_err());
    }

    #[test]
    fn keys_round_trip() {
        for (key, provider) in [
            ("va_bni", Provider::Midtrans),
            ("checkout:midtrans", Provider::Midtrans),
            ("checkout:paypal", Provider::Paypal),
        ] {
            assert_eq!(ProductOption::parse(key, provider).unwrap().key(), key);
        }
    }

    #[test]
    fn a_requested_method_goes_to_the_provider_the_product_chose_for_it() {
        let o = mixed();
        assert_eq!(
            route_from(&o, Some(Method::Qris), Checkout::Snap).unwrap(),
            Routing {
                provider: Provider::Midtrans,
                hosted_now: false
            }
        );
        assert_eq!(
            route_from(&o, Some(Method::VaBca), Checkout::Snap)
                .unwrap()
                .provider,
            Provider::Xendit
        );
        let (code, _) = route_from(&o, Some(Method::VaBni), Checkout::Snap).unwrap_err();
        assert_eq!(code, "method_not_enabled");
    }

    #[test]
    fn a_hosted_checkout_request_uses_an_enabled_checkout() {
        let r = route_from(&mixed(), None, Checkout::Provider).unwrap();
        assert_eq!(r.provider, Provider::Paypal);
        assert!(r.hosted_now);
        let only_methods = vec![opt("qris", Provider::Midtrans)];
        let (code, _) = route_from(&only_methods, None, Checkout::Provider).unwrap_err();
        assert_eq!(code, "checkout_unsupported");
    }

    #[test]
    fn the_customer_chooses_unless_there_is_a_single_hosted_option() {
        let r = route_from(&mixed(), None, Checkout::Snap).unwrap();
        assert!(!r.hosted_now);
        assert_eq!(r.provider, Provider::Midtrans);
        let paypal_only = vec![opt("checkout:paypal", Provider::Paypal)];
        let r = route_from(&paypal_only, None, Checkout::Snap).unwrap();
        assert!(r.hosted_now);
        assert_eq!(r.provider, Provider::Paypal);
        // one method is still a choice the customer makes on the page
        let one_method = vec![opt("qris", Provider::Midtrans)];
        assert!(
            !route_from(&one_method, None, Checkout::Snap)
                .unwrap()
                .hosted_now
        );
    }

    #[test]
    fn the_picker_lists_methods_and_checkouts_and_hides_refused_methods() {
        let o = mixed();
        let p = picker_with(&o, |_, _| false);
        assert_eq!(
            p.methods,
            vec![
                (Method::Qris, Provider::Midtrans),
                (Method::VaBca, Provider::Xendit)
            ]
        );
        assert_eq!(p.checkouts, vec![Provider::Paypal]);

        let qris_account = o[0].account_id;
        let p = picker_with(&o, |account, m| {
            account == qris_account && m == Method::Qris
        });
        assert_eq!(p.methods, vec![(Method::VaBca, Provider::Xendit)]);
        assert_eq!(p.checkouts, vec![Provider::Paypal]);
    }

    #[test]
    fn when_everything_is_refused_everything_stays_offered() {
        let methods_only = vec![
            opt("qris", Provider::Midtrans),
            opt("va_bca", Provider::Xendit),
        ];
        let p = picker_with(&methods_only, |_, _| true);
        assert_eq!(p.methods.len(), 2);
        // a hosted checkout counts as something left, so refused methods stay hidden
        let p = picker_with(&mixed(), |_, _| true);
        assert!(p.methods.is_empty());
        assert_eq!(p.checkouts, vec![Provider::Paypal]);
    }

    #[test]
    fn the_catalog_groups_a_products_options_by_provider() {
        let c = catalog(&mixed());
        assert_eq!(
            c,
            vec![
                (Provider::Midtrans, false, vec![Method::Qris]),
                (Provider::Xendit, false, vec![Method::VaBca]),
                (Provider::Paypal, true, vec![]),
            ]
        );
        let mixed_midtrans = vec![
            opt("qris", Provider::Midtrans),
            opt("checkout:midtrans", Provider::Midtrans),
        ];
        assert_eq!(
            catalog(&mixed_midtrans),
            vec![(Provider::Midtrans, true, vec![Method::Qris])]
        );
    }

    fn input(option: &str, provider: Provider) -> OptionInput {
        OptionInput {
            option: option.into(),
            provider,
        }
    }

    #[test]
    fn submitted_options_are_validated_as_a_whole() {
        let ok = parse_inputs(&[
            input("qris", Provider::Midtrans),
            input("va_bca", Provider::Xendit),
            input("checkout:paypal", Provider::Paypal),
        ])
        .unwrap();
        assert_eq!(ok.len(), 3);
        assert_eq!(ok[2].key(), "checkout:paypal");
        // a method is served by one provider
        let twice = parse_inputs(&[
            input("qris", Provider::Midtrans),
            input("qris", Provider::Xendit),
        ])
        .unwrap_err();
        assert!(twice.contains("listed twice"), "{twice}");
        assert!(parse_inputs(&[input("qris", Provider::Paypal)]).is_err());
        assert!(parse_inputs(&[]).unwrap().is_empty());
        let too_many: Vec<OptionInput> = (0..=MAX_OPTIONS)
            .map(|i| input(&format!("checkout:{i}"), Provider::Paypal))
            .collect();
        assert!(parse_inputs(&too_many).is_err());
    }
}
