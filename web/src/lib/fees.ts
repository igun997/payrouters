import type { FeeRule } from '#lib/api/types';

/** One editable fee-schedule row. Numbers are kept as strings while typing. */
export type FeeRow = { method: string; fixed: string; percent: string };

/** Editor rows → the wire map `{ method: { fixed, percent } }`. Rows without a method are skipped. */
export function rowsToFees(rows: FeeRow[]): Record<string, FeeRule> {
	const fees: Record<string, FeeRule> = {};
	for (const row of rows) {
		if (!row.method) continue;
		fees[row.method] = { fixed: Number(row.fixed) || 0, percent: Number(row.percent) || 0 };
	}
	return fees;
}

/** The wire map → editor rows. */
export function feesToRows(fees: Record<string, FeeRule>): FeeRow[] {
	return Object.entries(fees).map(([method, rule]) => ({
		method,
		fixed: String(rule.fixed),
		percent: String(rule.percent)
	}));
}
