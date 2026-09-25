use chrono::Utc;
use rust_decimal::Decimal;

use super::BudgetPolicy;
use crate::http::budget::ResolvedBudgetScope;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatusResponse {
	pub observed_at: i64,
	pub budgets: Vec<BudgetStatus>,
}

/// User-facing snapshot of one budget's definition, current usage, and fixed window.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatus {
	pub id: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub api_key_id: Option<String>,
	pub name: String,
	pub limit: BudgetStatusLimit,
	pub usage: BudgetStatusUsage,
	pub window: BudgetStatusWindow,
	pub on_budget_exceeded: String,
	pub updated_at: i64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatusLimit {
	pub unit: String,
	pub amount: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatusUsage {
	pub used: String,
	pub remaining: String,
	pub exceeded: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatusWindow {
	pub start: i64,
	pub end: i64,
	pub duration_ms: i64,
	pub expired: bool,
}

impl BudgetPolicy {
	/// Returns a point-in-time status snapshot, optionally filtered by API key display name.
	/// Expired counters are reported with zero usage even if no request has advanced their window.
	pub fn status(&self, api_key_id: Option<&str>) -> anyhow::Result<BudgetStatusResponse> {
		let observed_at = Utc::now();
		let mut budgets = self
			.counters
			.iter()
			.filter_map(|counter| {
				let definition = counter.definition.as_ref()?;
				let scope_key = match &definition.scope {
					ResolvedBudgetScope::Key { api_key_id } => Some(api_key_id.as_str()),
					_ => None,
				};
				if api_key_id.is_some_and(|filter| scope_key.is_some_and(|key| key != filter)) {
					return None;
				}
				let limit = definition.budget.limit.amount.decimal();
				let expired = observed_at >= counter.window_end;
				let used = if expired {
					Decimal::ZERO
				} else {
					counter.amount
				};
				let remaining = (limit - used).max(Decimal::ZERO);
				Some(BudgetStatus {
					id: definition.id.clone(),
					api_key_id: scope_key.map(str::to_owned),
					name: definition.budget.name.clone(),
					limit: BudgetStatusLimit {
						unit: definition.budget.limit.unit.as_str().to_owned(),
						amount: limit.normalize().to_string(),
					},
					usage: BudgetStatusUsage {
						used: used.normalize().to_string(),
						remaining: remaining.normalize().to_string(),
						exceeded: !expired && used >= limit,
					},
					window: BudgetStatusWindow {
						start: counter.window_start.timestamp_millis(),
						end: counter.window_end.timestamp_millis(),
						duration_ms: i64::try_from(counter.rolling.as_millis())
							.expect("budget duration was validated"),
						expired,
					},
					on_budget_exceeded: definition.budget.on_budget_exceeded.as_str().to_owned(),
					updated_at: counter.updated_at.timestamp_millis(),
				})
			})
			.collect::<Vec<_>>();
		budgets.sort_by(|a, b| (&a.id, &a.name).cmp(&(&b.id, &b.name)));
		Ok(BudgetStatusResponse {
			observed_at: observed_at.timestamp_millis(),
			budgets,
		})
	}
}
