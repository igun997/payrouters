//! Dashboard statistics and the cash-in recap (list, totals, CSV export).

use std::collections::BTreeMap;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use payrouters_types::{Method, Mode, Provider};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::parse_date;
use super::payments::{CashInView, cash_in_view};
use crate::api::v1::clamp_limit;
use crate::error::{ApiQuery, AppError, Result};
use crate::models::CashInRow;
use crate::state::AppState;
use crate::util::Cursor;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats/summary", get(summary))
        .route("/stats/timeseries", get(timeseries))
        .route("/stats/breakdown", get(breakdown))
        .route("/cash-ins", get(cash_ins))
        .route("/cash-ins/export.csv", get(export_csv))
}

const MAX_SPAN_DAYS: i64 = 400;
const EXPORT_CAP: i64 = 200_000;

fn local_midnight(date: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let naive = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    tz.from_local_datetime(&naive)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| naive.and_utc())
}

/// `[start, end)` in UTC covering the inclusive local-date range.
pub fn day_bounds(from: NaiveDate, to: NaiveDate, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    (
        local_midnight(from, tz),
        local_midnight(to + Duration::days(1), tz),
    )
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    mode: Option<Mode>,
    from: Option<String>,
    to: Option<String>,
    product_id: Option<Uuid>,
    provider: Option<Provider>,
    method: Option<Method>,
    by: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

#[derive(Clone)]
struct Range {
    mode: Mode,
    from: NaiveDate,
    to: NaiveDate,
    product_id: Option<Uuid>,
    provider: Option<Provider>,
    method: Option<Method>,
    tz: Tz,
}

impl Range {
    fn parse(state: &AppState, q: &StatsQuery) -> Result<Range> {
        let tz = state.config.timezone;
        let today = Utc::now().with_timezone(&tz).date_naive();
        let to = match &q.to {
            Some(t) => parse_date("to", t)?,
            None => today,
        };
        let from = match &q.from {
            Some(f) => parse_date("from", f)?,
            None => to - Duration::days(29),
        };
        if from > to {
            return Err(AppError::bad_request("from must not be after to"));
        }
        if (to - from).num_days() > MAX_SPAN_DAYS {
            return Err(AppError::bad_request(format!(
                "range is limited to {MAX_SPAN_DAYS} days"
            )));
        }
        Ok(Range {
            mode: q.mode.unwrap_or(Mode::Live),
            from,
            to,
            product_id: q.product_id,
            provider: q.provider,
            method: q.method,
            tz,
        })
    }

    fn bounds(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        day_bounds(self.from, self.to, self.tz)
    }

    fn days(&self) -> i64 {
        (self.to - self.from).num_days() + 1
    }

    fn previous(&self) -> Range {
        let prev_to = self.from - Duration::days(1);
        Range {
            from: prev_to - Duration::days(self.days() - 1),
            to: prev_to,
            ..self.clone()
        }
    }

    /// Appends `AND ...` filters. `ts` is the timestamp column, `a` the table alias.
    fn push_filters(&self, qb: &mut QueryBuilder<Postgres>, a: &str, ts: &str) {
        let (start, end) = self.bounds();
        qb.push(format!(" AND {a}.mode = "))
            .push_bind(self.mode.as_str());
        qb.push(format!(" AND {a}.{ts} >= ")).push_bind(start);
        qb.push(format!(" AND {a}.{ts} < ")).push_bind(end);
        if let Some(p) = self.product_id {
            qb.push(format!(" AND {a}.product_id = ")).push_bind(p);
        }
        if let Some(p) = self.provider {
            qb.push(format!(" AND {a}.provider = "))
                .push_bind(p.as_str());
        }
        if let Some(m) = self.method {
            qb.push(format!(" AND {a}.method = ")).push_bind(m.as_str());
        }
    }
}

#[derive(Serialize, Default, Clone, Copy)]
struct Sums {
    count: i64,
    gross: i64,
    fee: i64,
    net: i64,
}

async fn cash_sums(state: &AppState, r: &Range) -> Result<Sums> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT COUNT(*)::bigint, COALESCE(SUM(c.gross_amount),0)::bigint,
                COALESCE(SUM(c.fee_amount),0)::bigint, COALESCE(SUM(c.net_amount),0)::bigint
         FROM cash_ins c WHERE 1=1",
    );
    r.push_filters(&mut qb, "c", "paid_at");
    let (count, gross, fee, net): (i64, i64, i64, i64) =
        qb.build_query_as().fetch_one(&state.pool).await?;
    Ok(Sums {
        count,
        gross,
        fee,
        net,
    })
}

pub(crate) async fn summary(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<StatsQuery>,
) -> Result<Json<Value>> {
    let r = Range::parse(&state, &q)?;

    let mut qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT p.status, COUNT(*)::bigint FROM payments p WHERE 1=1");
    r.push_filters(&mut qb, "p", "created_at");
    qb.push(" GROUP BY p.status");
    let by_status: Vec<(String, i64)> = qb.build_query_as().fetch_all(&state.pool).await?;
    let count = |s: &str| {
        by_status
            .iter()
            .find(|(k, _)| k == s)
            .map_or(0, |(_, n)| *n)
    };
    let (pending, paid, expired, failed, cancelled) = (
        count("pending"),
        count("paid"),
        count("expired"),
        count("failed"),
        count("cancelled"),
    );
    let total = pending + paid + expired + failed + cancelled;
    let settled = paid + expired + failed + cancelled;
    let conversion = if settled == 0 {
        0.0
    } else {
        paid as f64 / settled as f64
    };

    let cash = cash_sums(&state, &r).await?;
    let prev = cash_sums(&state, &r.previous()).await?;
    let average = if cash.count == 0 {
        0
    } else {
        cash.gross / cash.count
    };

    Ok(Json(json!({
        "range": { "from": r.from.to_string(), "to": r.to.to_string(), "mode": r.mode },
        "payments": {
            "total": total, "pending": pending, "paid": paid,
            "expired": expired, "failed": failed, "cancelled": cancelled,
        },
        "conversion_rate": (conversion * 10_000.0).round() / 10_000.0,
        "cash_in": {
            "count": cash.count, "gross": cash.gross, "fee": cash.fee, "net": cash.net, "average": average,
        },
        "previous": prev,
    })))
}

async fn timeseries(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<StatsQuery>,
) -> Result<Json<Value>> {
    let r = Range::parse(&state, &q)?;
    let tz = r.tz.name().to_string();

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT (c.paid_at AT TIME ZONE ");
    qb.push_bind(tz.clone());
    qb.push(
        ")::date AS day, COUNT(*)::bigint, COALESCE(SUM(c.gross_amount),0)::bigint,
                COALESCE(SUM(c.fee_amount),0)::bigint, COALESCE(SUM(c.net_amount),0)::bigint
         FROM cash_ins c WHERE 1=1",
    );
    r.push_filters(&mut qb, "c", "paid_at");
    qb.push(" GROUP BY 1");
    let paid: Vec<(NaiveDate, i64, i64, i64, i64)> =
        qb.build_query_as().fetch_all(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new("SELECT (p.created_at AT TIME ZONE ");
    qb.push_bind(tz);
    qb.push(")::date AS day, COUNT(*)::bigint FROM payments p WHERE 1=1");
    r.push_filters(&mut qb, "p", "created_at");
    qb.push(" GROUP BY 1");
    let created: Vec<(NaiveDate, i64)> = qb.build_query_as().fetch_all(&state.pool).await?;

    let paid: BTreeMap<NaiveDate, (i64, i64, i64, i64)> = paid
        .into_iter()
        .map(|(d, c, g, f, n)| (d, (c, g, f, n)))
        .collect();
    let created: BTreeMap<NaiveDate, i64> = created.into_iter().collect();

    let data: Vec<Value> = (0..r.days())
        .map(|i| {
            let day = r.from + Duration::days(i);
            let (count, gross, fee, net) = paid.get(&day).copied().unwrap_or_default();
            json!({
                "date": day.to_string(),
                "created": created.get(&day).copied().unwrap_or(0),
                "count": count, "gross": gross, "fee": fee, "net": net,
            })
        })
        .collect();
    Ok(Json(json!({ "data": data })))
}

async fn breakdown(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<StatsQuery>,
) -> Result<Json<Value>> {
    let r = Range::parse(&state, &q)?;
    // (key expression on payments p / cash_ins c, label expression, extra join on products)
    let (p_key, c_key, label_join) = match q.by.as_deref() {
        Some("provider") => ("p.provider", "c.provider", false),
        Some("method") => (
            "COALESCE(p.method, 'other')",
            "COALESCE(c.method, 'other')",
            false,
        ),
        Some("product") => ("p.product_id::text", "c.product_id::text", true),
        _ => {
            return Err(AppError::bad_request(
                "by must be provider, method or product",
            ));
        }
    };

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "SELECT {p_key} AS key, COUNT(*)::bigint FROM payments p WHERE 1=1"
    ));
    r.push_filters(&mut qb, "p", "created_at");
    qb.push(" GROUP BY 1");
    let created: Vec<(String, i64)> = qb.build_query_as().fetch_all(&state.pool).await?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "SELECT {c_key} AS key, COUNT(*)::bigint, COALESCE(SUM(c.gross_amount),0)::bigint,
                COALESCE(SUM(c.fee_amount),0)::bigint, COALESCE(SUM(c.net_amount),0)::bigint
         FROM cash_ins c WHERE 1=1"
    ));
    r.push_filters(&mut qb, "c", "paid_at");
    qb.push(" GROUP BY 1");
    let paid: Vec<(String, i64, i64, i64, i64)> =
        qb.build_query_as().fetch_all(&state.pool).await?;

    let names: BTreeMap<String, String> = if label_join {
        let rows: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM products")
            .fetch_all(&state.pool)
            .await?;
        rows.into_iter()
            .map(|(id, n)| (id.to_string(), n))
            .collect()
    } else {
        BTreeMap::new()
    };

    let mut rows: BTreeMap<String, (i64, i64, i64, i64, i64)> = BTreeMap::new();
    for (k, n) in created {
        rows.entry(k).or_default().0 = n;
    }
    for (k, n, g, f, net) in paid {
        let e = rows.entry(k).or_default();
        (e.1, e.2, e.3, e.4) = (n, g, f, net);
    }
    let mut data: Vec<(i64, Value)> = rows
        .into_iter()
        .map(|(key, (created, paid, gross, fee, net))| {
            let label = match q.by.as_deref() {
                Some("provider") => key
                    .parse::<Provider>()
                    .map(|p| p.label().to_string())
                    .unwrap_or_else(|_| key.clone()),
                Some("method") => key
                    .parse::<Method>()
                    .map(|m| m.label().to_string())
                    .unwrap_or_else(|_| "Other".to_string()),
                _ => names.get(&key).cloned().unwrap_or_else(|| key.clone()),
            };
            let rate = if created == 0 {
                0.0
            } else {
                (paid as f64 / created as f64).min(1.0)
            };
            (
                gross,
                json!({
                    "key": key, "label": label, "created": created, "paid": paid,
                    "success_rate": (rate * 10_000.0).round() / 10_000.0,
                    "gross": gross, "fee": fee, "net": net,
                }),
            )
        })
        .collect();
    data.sort_by_key(|row| std::cmp::Reverse(row.0));
    Ok(Json(
        json!({ "data": data.into_iter().map(|(_, v)| v).collect::<Vec<_>>() }),
    ))
}

const CASH_SELECT: &str = "SELECT c.*, p.order_id, pr.name AS product_name
    FROM cash_ins c
    JOIN payments p ON p.id = c.payment_id
    JOIN products pr ON pr.id = c.product_id
    WHERE 1=1";

async fn cash_ins(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<StatsQuery>,
) -> Result<Json<Value>> {
    let r = Range::parse(&state, &q)?;
    let limit = clamp_limit(q.limit);

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(CASH_SELECT);
    r.push_filters(&mut qb, "c", "paid_at");
    if let Some(raw) = &q.cursor {
        let c = Cursor::decode(raw).ok_or_else(|| AppError::bad_request("invalid cursor"))?;
        qb.push(" AND (c.paid_at, c.id) < (")
            .push_bind(c.ts)
            .push(", ")
            .push_bind(c.id)
            .push(")");
    }
    qb.push(" ORDER BY c.paid_at DESC, c.id DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows: Vec<CashInRow> = qb.build_query_as().fetch_all(&state.pool).await?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| {
            Cursor {
                ts: r.paid_at,
                id: r.id,
            }
            .encode()
        })
    } else {
        None
    };
    let totals = cash_sums(&state, &r).await?;
    let data: Vec<CashInView> = rows.into_iter().map(cash_in_view).collect();
    Ok(Json(
        json!({ "data": data, "next_cursor": next_cursor, "totals": totals }),
    ))
}

/// Spreadsheet formula injection guard for user-controlled text.
fn csv_safe(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

async fn export_csv(
    State(state): State<AppState>,
    ApiQuery(q): ApiQuery<StatsQuery>,
) -> Result<Response> {
    let r = Range::parse(&state, &q)?;
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(CASH_SELECT);
    r.push_filters(&mut qb, "c", "paid_at");
    qb.push(" ORDER BY c.paid_at DESC, c.id DESC LIMIT ")
        .push_bind(EXPORT_CAP);
    let rows: Vec<CashInRow> = qb.build_query_as().fetch_all(&state.pool).await?;

    let mut w = csv::Writer::from_writer(Vec::new());
    let io = |e: csv::Error| AppError::Internal(anyhow::Error::from(e));
    w.write_record([
        "paid_at",
        "order_id",
        "payment_id",
        "product",
        "provider",
        "method",
        "mode",
        "gross",
        "fee",
        "net",
        "provider_ref",
    ])
    .map_err(io)?;
    for c in rows {
        w.write_record([
            c.paid_at.with_timezone(&r.tz).to_rfc3339(),
            csv_safe(c.order_id.as_deref().unwrap_or_default()),
            c.payment_id.to_string(),
            csv_safe(c.product_name.as_deref().unwrap_or_default()),
            c.provider,
            c.method.unwrap_or_default(),
            c.mode,
            c.gross_amount.to_string(),
            c.fee_amount.to_string(),
            c.net_amount.to_string(),
            csv_safe(c.provider_ref.as_deref().unwrap_or_default()),
        ])
        .map_err(io)?;
    }
    let bytes = w
        .into_inner()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
    let filename = format!("cash-ins_{}_{}_{}.csv", r.mode, r.from, r.to);
    let mut resp = bytes.into_response();
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?,
    );
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_bounds_follow_the_local_calendar() {
        let tz: Tz = "Asia/Jakarta".parse().unwrap();
        let d = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let (start, end) = day_bounds(d, d, tz);
        assert_eq!(start.to_rfc3339(), "2026-11-25T17:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-11-26T17:00:00+00:00");
    }

    #[test]
    fn csv_injection_is_neutralised() {
        assert_eq!(csv_safe("=SUM(A1)"), "'=SUM(A1)");
        assert_eq!(csv_safe("INV-1"), "INV-1");
        assert_eq!(csv_safe("-1+1"), "'-1+1");
    }
}
