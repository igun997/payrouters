import type { PaymentStatus } from '#lib/api/types';

export const TZ = 'Asia/Jakarta';

const rupiah = new Intl.NumberFormat('id-ID', {
	style: 'currency',
	currency: 'IDR',
	maximumFractionDigits: 0
});
const integer = new Intl.NumberFormat('id-ID');
const dateTime = new Intl.DateTimeFormat('id-ID', {
	timeZone: TZ,
	day: '2-digit',
	month: 'short',
	year: 'numeric',
	hour: '2-digit',
	minute: '2-digit',
	hour12: false
});
const dateOnly = new Intl.DateTimeFormat('id-ID', {
	timeZone: TZ,
	day: '2-digit',
	month: 'short',
	year: 'numeric'
});
const isoDay = new Intl.DateTimeFormat('en-CA', {
	timeZone: TZ,
	year: 'numeric',
	month: '2-digit',
	day: '2-digit'
});

/** Integer rupiah, e.g. "Rp 150.000". */
export function formatIdr(amount: number | null | undefined): string {
	return amount == null ? '—' : rupiah.format(amount);
}

export function formatInt(value: number | null | undefined): string {
	return value == null ? '—' : integer.format(value);
}

/** Timestamp shown in Asia/Jakarta, e.g. "26 Nov 2026, 19:05 WIB". */
export function formatDateTime(iso: string | null | undefined): string {
	if (!iso) return '—';
	const date = new Date(iso);
	return Number.isNaN(date.getTime()) ? iso : `${dateTime.format(date)} WIB`;
}

/** A `YYYY-MM-DD` day (the stats granularity) shown in Asia/Jakarta. */
export function formatDay(ymd: string): string {
	const date = new Date(`${ymd}T00:00:00+07:00`);
	return Number.isNaN(date.getTime()) ? ymd : dateOnly.format(date);
}

export function formatPercent(ratio: number | null | undefined): string {
	return ratio == null ? '—' : `${(ratio * 100).toFixed(1)}%`;
}

/** Relative change against the previous period, or null when there is nothing to compare. */
export function percentChange(current: number, previous: number): number | null {
	if (previous === 0) return current === 0 ? 0 : null;
	return (current - previous) / previous;
}

export function formatDelta(change: number | null): string {
	if (change === null) return 'n/a';
	const sign = change > 0 ? '+' : '';
	return `${sign}${(change * 100).toFixed(1)}%`;
}

/** Today's date in Asia/Jakarta as `YYYY-MM-DD`. */
export function todayJakarta(): string {
	return isoDay.format(new Date());
}

export function addDays(ymd: string, days: number): string {
	const [y, m, d] = ymd.split('-').map(Number);
	return new Date(Date.UTC(y, m - 1, d + days)).toISOString().slice(0, 10);
}

/** Remaining time as "HH:MM:SS" (or "expired"). */
export function formatCountdown(targetIso: string, now: number): string {
	const remaining = new Date(targetIso).getTime() - now;
	if (remaining <= 0) return 'expired';
	const total = Math.floor(remaining / 1000);
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = total % 60;
	return [h, m, s].map((n) => String(n).padStart(2, '0')).join(':');
}

export const STATUS_LABEL: Record<PaymentStatus, string> = {
	pending: 'Pending',
	paid: 'Paid',
	expired: 'Expired',
	failed: 'Failed',
	cancelled: 'Cancelled'
};

export const STATUS_VARIANT: Record<
	PaymentStatus,
	'default' | 'secondary' | 'destructive' | 'outline'
> = {
	pending: 'secondary',
	paid: 'default',
	expired: 'outline',
	failed: 'destructive',
	cancelled: 'outline'
};

export const PROVIDER_LABEL: Record<string, string> = {
	midtrans: 'Midtrans',
	xendit: 'Xendit',
	idrx: 'IDRX',
	mock: 'Mock'
};

export type RangePreset = '7d' | '30d' | '90d' | 'custom';

/** Inclusive `from`/`to` days for a preset, ending today in Asia/Jakarta. */
export function presetRange(preset: Exclude<RangePreset, 'custom'>): { from: string; to: string } {
	const to = todayJakarta();
	const days = preset === '7d' ? 7 : preset === '30d' ? 30 : 90;
	return { from: addDays(to, -(days - 1)), to };
}
