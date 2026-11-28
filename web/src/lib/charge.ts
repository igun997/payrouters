// How a conversion to the provider's currency (PayPal charges US dollars) reads to the customer.
import type { Charge } from '#lib/api/types';
import { formatDay, formatIdr } from '#lib/format';

/** The amount the customer is charged: "USD 7.02". */
export function chargeAmount(charge: Charge): string {
	return `${charge.currency} ${charge.value}`;
}

/** The rate it was converted at: "1 USD = Rp 18.378". */
export function chargeRate(charge: Charge): string {
	return `1 ${charge.currency} = ${formatIdr(charge.idr_per_unit)}`;
}

/** Where the rate comes from, e.g. "European Central Bank rate of 27 Nov 2026". */
export function chargeSource(charge: Charge): string {
	if (charge.source !== 'ecb') return 'Fixed rate';
	return charge.as_of
		? `European Central Bank rate of ${formatDay(charge.as_of)}`
		: 'European Central Bank rate';
}
