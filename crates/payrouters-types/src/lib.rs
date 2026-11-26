//! Standardized PayRouters wire types, shared by the server and the Rust SDK.
//! Field names and enums mirror `docs/api.md`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $s)] $variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub const fn as_str(&self) -> &'static str {
                match self { $($name::$variant => $s),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok($name::$variant),)+
                    other => Err(format!("unknown {} `{}`", stringify!($name), other)),
                }
            }
        }
    };
}

string_enum!(
    /// Sandbox vs live data plane.
    Mode { Sandbox => "sandbox", Live => "live" }
);

string_enum!(
    Provider {
        Midtrans => "midtrans",
        Xendit => "xendit",
        Idrx => "idrx",
        Mock => "mock",
    }
);

string_enum!(
    PaymentStatus {
        Pending => "pending",
        Paid => "paid",
        Expired => "expired",
        Failed => "failed",
        Cancelled => "cancelled",
    }
);

string_enum!(
    /// Standard payment method codes, identical across providers.
    Method {
        VaBca => "va_bca",
        VaBni => "va_bni",
        VaBri => "va_bri",
        VaMandiri => "va_mandiri",
        VaPermata => "va_permata",
        Qris => "qris",
    }
);

string_enum!(
    Checkout { Snap => "snap", Provider => "provider" }
);

string_enum!(
    /// Result forced by the sandbox simulator.
    Outcome { Paid => "paid", Expired => "expired", Failed => "failed" }
);

string_enum!(
    EventType {
        PaymentPaid => "payment.paid",
        PaymentExpired => "payment.expired",
        PaymentFailed => "payment.failed",
        PaymentCancelled => "payment.cancelled",
    }
);

impl Mode {
    pub const fn livemode(&self) -> bool {
        matches!(self, Mode::Live)
    }
}

impl Provider {
    pub const fn label(&self) -> &'static str {
        match self {
            Provider::Midtrans => "Midtrans",
            Provider::Xendit => "Xendit",
            Provider::Idrx => "IDRX",
            Provider::Mock => "Mock (simulator)",
        }
    }
}

impl PaymentStatus {
    pub const fn is_terminal(&self) -> bool {
        !matches!(self, PaymentStatus::Pending)
    }

    pub const fn event_type(&self) -> Option<EventType> {
        match self {
            PaymentStatus::Pending => None,
            PaymentStatus::Paid => Some(EventType::PaymentPaid),
            PaymentStatus::Expired => Some(EventType::PaymentExpired),
            PaymentStatus::Failed => Some(EventType::PaymentFailed),
            PaymentStatus::Cancelled => Some(EventType::PaymentCancelled),
        }
    }
}

impl From<Outcome> for PaymentStatus {
    fn from(o: Outcome) -> Self {
        match o {
            Outcome::Paid => PaymentStatus::Paid,
            Outcome::Expired => PaymentStatus::Expired,
            Outcome::Failed => PaymentStatus::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodGroup {
    VirtualAccount,
    Qris,
}

impl Method {
    pub const fn label(&self) -> &'static str {
        match self {
            Method::VaBca => "BCA Virtual Account",
            Method::VaBni => "BNI Virtual Account",
            Method::VaBri => "BRI Virtual Account",
            Method::VaMandiri => "Mandiri Virtual Account",
            Method::VaPermata => "Permata Virtual Account",
            Method::Qris => "QRIS",
        }
    }

    pub const fn group(&self) -> MethodGroup {
        match self {
            Method::Qris => MethodGroup::Qris,
            _ => MethodGroup::VirtualAccount,
        }
    }

    /// Lower-case bank slug for virtual-account methods.
    pub const fn bank(&self) -> Option<&'static str> {
        match self {
            Method::VaBca => Some("bca"),
            Method::VaBni => Some("bni"),
            Method::VaBri => Some("bri"),
            Method::VaMandiri => Some("mandiri"),
            Method::VaPermata => Some("permata"),
            Method::Qris => None,
        }
    }

    pub fn from_bank(bank: &str) -> Option<Method> {
        match bank.to_ascii_lowercase().as_str() {
            "bca" => Some(Method::VaBca),
            "bni" => Some(Method::VaBni),
            "bri" => Some(Method::VaBri),
            "mandiri" => Some(Method::VaMandiri),
            "permata" => Some(Method::VaPermata),
            _ => None,
        }
    }

    pub fn info(&self) -> MethodInfo {
        MethodInfo {
            code: *self,
            label: self.label().to_string(),
            group: self.group(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodInfo {
    pub code: Method,
    pub label: String,
    pub group: MethodGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMethods {
    pub provider: Provider,
    pub checkout_provider: bool,
    pub methods: Vec<MethodInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodCatalog {
    pub data: Vec<ProviderMethods>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Customer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub price: i64,
    pub quantity: i64,
}

/// What the customer has to do to pay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Instructions {
    VirtualAccount {
        bank: String,
        account_number: Option<String>,
        account_name: Option<String>,
        biller_code: Option<String>,
        bill_key: Option<String>,
    },
    Qris {
        qr_string: String,
        qr_image_url: Option<String>,
    },
    Redirect {
        url: String,
    },
}

/// The standardized payment object returned by every PayRouters surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub order_id: String,
    pub status: PaymentStatus,
    pub mode: Mode,
    pub livemode: bool,
    pub amount: i64,
    pub currency: String,
    pub fee_amount: Option<i64>,
    pub net_amount: Option<i64>,
    pub payable_amount: Option<i64>,
    pub provider: Provider,
    pub method: Option<Method>,
    pub checkout: Checkout,
    pub snap_url: String,
    pub instructions: Option<Instructions>,
    pub description: Option<String>,
    pub customer: Option<Customer>,
    pub items: Vec<Item>,
    pub metadata: serde_json::Value,
    pub return_url: Option<String>,
    pub provider_ref: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CreatePaymentRequest {
    pub order_id: String,
    pub amount: i64,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub checkout: Option<Checkout>,
    #[serde(default)]
    pub method: Option<Method>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub customer: Option<Customer>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub return_url: Option<String>,
    #[serde(default)]
    pub provider_options: Option<serde_json::Value>,
}

// The enum comes from `string_enum!`, which cannot carry a `#[default]` variant.
#[allow(clippy::derivable_impls)]
impl Default for Checkout {
    fn default() -> Self {
        Checkout::Snap
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventData {
    pub payment: Payment,
}

/// Outbound webhook body (PayRouters → your product).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayRouterEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub livemode: bool,
    pub created_at: DateTime<Utc>,
    pub data: EventData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

/// Per-method fee rule: `fixed` rupiah + `percent` of the amount (e.g. `0.7` = 0.7 %).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FeeRule {
    #[serde(default)]
    pub fixed: i64,
    #[serde(default)]
    pub percent: f64,
}

pub type FeeSchedule = BTreeMap<String, FeeRule>;

impl FeeRule {
    /// Fee in whole rupiah, rounded half up.
    pub fn apply(&self, amount: i64) -> i64 {
        let pct = (amount as f64 * self.percent / 100.0).round() as i64;
        (self.fixed + pct).max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_round_trip() {
        for m in Method::ALL {
            assert_eq!(m.as_str().parse::<Method>().unwrap(), *m);
            let json = serde_json::to_string(m).unwrap();
            assert_eq!(json, format!("\"{}\"", m.as_str()));
        }
        assert_eq!(
            serde_json::to_string(&EventType::PaymentPaid).unwrap(),
            "\"payment.paid\""
        );
    }

    #[test]
    fn instructions_are_tagged() {
        let i = Instructions::Qris {
            qr_string: "000201".into(),
            qr_image_url: None,
        };
        let v = serde_json::to_value(&i).unwrap();
        assert_eq!(v["type"], "qris");
        let back: Instructions = serde_json::from_value(v).unwrap();
        assert_eq!(back, i);
    }

    #[test]
    fn fee_rule_rounds() {
        let r = FeeRule {
            fixed: 0,
            percent: 0.7,
        };
        assert_eq!(r.apply(100_000), 700);
        let r = FeeRule {
            fixed: 4000,
            percent: 0.0,
        };
        assert_eq!(r.apply(10), 4000);
    }
}
