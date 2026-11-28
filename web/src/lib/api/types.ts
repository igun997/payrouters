// Types mirror docs/api.md (contract v1). Field names are the wire names (snake_case).

export type Mode = 'sandbox' | 'live';
export type PaymentStatus = 'pending' | 'paid' | 'expired' | 'failed' | 'cancelled';
export type Provider = 'midtrans' | 'xendit' | 'idrx' | 'mayar' | 'paypal' | 'mock';
export type Method = 'va_bca' | 'va_bni' | 'va_bri' | 'va_mandiri' | 'va_permata' | 'qris';
export type Checkout = 'snap' | 'provider';
export type SimulateOutcome = 'paid' | 'expired' | 'failed';
export type WebhookOutcome = 'applied' | 'noop' | 'unmatched' | 'rejected' | 'error';
export type DeliveryStatus = 'pending' | 'delivered' | 'failed';
export type MethodGroup = 'virtual_account' | 'qris';
export type StatsBy = 'provider' | 'method' | 'product';

export interface Page<T> {
	data: T[];
	next_cursor: string | null;
}

export interface Customer {
	name: string | null;
	email: string | null;
	phone: string | null;
}

export interface CustomerInput {
	name?: string;
	email?: string;
	phone?: string;
}

export interface Item {
	id: string;
	name: string;
	price: number;
	quantity: number;
}

export type Instructions =
	| {
			type: 'virtual_account';
			bank: string;
			account_number: string | null;
			account_name: string | null;
			biller_code: string | null;
			bill_key: string | null;
	  }
	| { type: 'qris'; qr_string: string; qr_image_url: string | null }
	| { type: 'redirect'; url: string };

export interface Payment {
	id: string;
	order_id: string;
	status: PaymentStatus;
	mode: Mode;
	livemode: boolean;
	amount: number;
	currency: 'IDR';
	fee_amount: number | null;
	net_amount: number | null;
	/** Exact total the customer must transfer when the provider adds a fee on top; null = pay `amount`. */
	payable_amount: number | null;
	provider: Provider;
	method: Method | null;
	checkout: Checkout;
	snap_url: string | null;
	instructions: Instructions | null;
	description: string | null;
	customer: Customer | null;
	items: Item[];
	metadata: Record<string, unknown>;
	return_url: string | null;
	provider_ref: string | null;
	expires_at: string;
	paid_at: string | null;
	created_at: string;
	updated_at: string;
}

export interface AdminPayment extends Payment {
	product: { id: string; name: string };
	provider_account_id: string;
}

export interface PaymentEvent {
	id: string;
	received_at: string;
	outcome: string;
	verified: boolean;
	error: string | null;
}

export interface Delivery {
	id: string;
	payment_id: string;
	product_id: string;
	event_id: string;
	event_type: string;
	status: DeliveryStatus;
	attempts: number;
	next_attempt_at: string | null;
	last_status_code: number | null;
	last_error: string | null;
	payload: unknown;
	created_at: string;
	delivered_at: string | null;
}

export interface CashIn {
	id: string;
	payment_id: string;
	order_id: string;
	product: { id: string; name: string };
	provider: Provider;
	method: Method | null;
	mode: Mode;
	currency: 'IDR';
	gross_amount: number;
	fee_amount: number;
	net_amount: number;
	paid_at: string;
	provider_ref: string | null;
}

export interface CashInTotals {
	count: number;
	gross: number;
	fee: number;
	net: number;
}

export interface CashInPage extends Page<CashIn> {
	totals: CashInTotals;
}

export interface PaymentDetail {
	payment: AdminPayment;
	events: PaymentEvent[];
	deliveries: Delivery[];
	cash_in: CashIn | null;
}

export interface CreatePaymentBody {
	order_id: string;
	amount: number;
	provider?: Provider;
	checkout?: Checkout;
	method?: Method;
	description?: string;
	customer?: CustomerInput;
	items?: Item[];
	metadata?: Record<string, unknown>;
	expires_in?: number;
	return_url?: string;
	provider_options?: Record<string, unknown>;
}

export interface AdminCreatePaymentBody extends CreatePaymentBody {
	product_id: string;
	mode: Mode;
}

export interface WebhookEvent {
	id: string;
	provider: Provider;
	provider_account_id: string;
	payment_id: string | null;
	verified: boolean;
	outcome: WebhookOutcome;
	error: string | null;
	headers: Record<string, string>;
	/** JSON object, or the raw body as a string when it was not JSON. */
	body: unknown;
	received_at: string;
}

export interface ApiKey {
	id: string;
	mode: Mode;
	name: string;
	prefix: string;
	created_at: string;
	last_used_at: string | null;
	revoked_at: string | null;
}

export interface Product {
	id: string;
	name: string;
	slug: string;
	webhook_url: string | null;
	default_provider: Provider | null;
	/** The checklist of payment options; empty means the product uses a single provider. */
	payment_options: PaymentOption[];
	api_keys: ApiKey[];
	created_at: string;
}
/** A standard method code, or `checkout:<provider>` for that provider's hosted checkout page. */
export type PaymentOptionKey = Method | `checkout:${Provider}`;
/** One entry of a product's checklist: the option is served by exactly one provider. */
export interface PaymentOption {
	option: PaymentOptionKey;
	provider: Provider;
}
/** A provider with an enabled account, with what it can serve. */
export interface AvailableProvider {
	provider: Provider;
	label: string;
	modes: Mode[];
	methods: MethodInfo[];
	checkout: boolean;
}
/** GET and PUT shape of `/admin/products/:id/payment-options`. */
export interface PaymentOptions {
	data: PaymentOption[];
	available: AvailableProvider[];
}

export interface ProductCreated extends Product {
	webhook_secret: string;
}

export interface ApiKeyCreated extends ApiKey {
	key: string;
}

export interface FeeRule {
	fixed: number;
	percent: number;
}

export interface ProviderAccount {
	id: string;
	provider: Provider;
	mode: Mode;
	label: string;
	enabled: boolean;
	/** Secret fields masked as "••••abcd" (or ""), non-secret fields plain. */
	credentials: Record<string, string>;
	/** Per-method fee schedule; key "*" is the default. */
	fees: Record<string, FeeRule>;
	webhook_url: string;
	created_at: string;
}

export interface ProviderCreateBody {
	provider: Provider;
	mode: Mode;
	label?: string;
	credentials: Record<string, string>;
	fees?: Record<string, FeeRule>;
}

export interface ProviderPatchBody {
	label?: string;
	enabled?: boolean;
	credentials?: Record<string, string>;
	fees?: Record<string, FeeRule>;
}

export interface ProviderTestResult {
	ok: boolean;
	message: string;
}

export interface CredentialField {
	key: string;
	label: string;
	secret: boolean;
	required: boolean;
	placeholder: string | null;
}

export interface MethodInfo {
	code: Method;
	label: string;
	group: MethodGroup;
}

export interface MetaProvider {
	id: Provider;
	label: string;
	supports_sandbox: boolean;
	supports_provider_checkout: boolean;
	credential_fields: CredentialField[];
	methods: MethodInfo[];
}

export interface Meta {
	providers: MetaProvider[];
	methods: MethodInfo[];
	statuses: PaymentStatus[];
}

export interface Me {
	ok: boolean;
	version: string;
	public_base_url: string;
	timezone: string;
	/** The MCP server URL (`{public_base_url}/mcp`) that assistants connect to. */
	mcp_url: string;
}

export interface StatsRange {
	from: string;
	to: string;
	mode: Mode;
}

export interface StatsSummary {
	range: StatsRange;
	payments: {
		total: number;
		pending: number;
		paid: number;
		expired: number;
		failed: number;
		cancelled: number;
	};
	conversion_rate: number;
	cash_in: { count: number; gross: number; fee: number; net: number; average: number };
	previous: { count: number; gross: number; fee: number; net: number };
}

export interface TimeseriesPoint {
	date: string;
	created: number;
	count: number;
	gross: number;
	fee: number;
	net: number;
}

export interface BreakdownRow {
	key: string;
	label: string;
	created: number;
	paid: number;
	success_rate: number;
	gross: number;
	fee: number;
	net: number;
}

export interface SnapMethod {
	code: Method;
	label: string;
	group: MethodGroup;
}

/** What the customer is charged when the provider converts the amount (PayPal charges USD). */
export interface Charge {
	currency: string;
	/** Decimal string, e.g. "7.02". */
	value: string;
	/** Rupiah per one unit of `currency`. */
	idr_per_unit: number;
	source: 'ecb' | 'fixed';
	/** Date of the reference rate when `source` is `ecb` (YYYY-MM-DD). */
	as_of: string | null;
}

export interface SnapPayment {
	id: string;
	order_id: string;
	status: PaymentStatus;
	livemode: boolean;
	amount: number;
	payable_amount: number | null;
	currency: 'IDR';
	provider: Provider;
	method: Method | null;
	description: string | null;
	items: Item[];
	instructions: Instructions | null;
	expires_at: string;
	paid_at: string | null;
	return_url: string | null;
	/** What the customer was charged once a converting checkout (PayPal) exists. */
	charge: Charge | null;
}

export interface SnapCheckout {
	provider: Provider;
	label: string;
	/** What the customer would be charged, when the provider converts the amount (PayPal). */
	quote: Charge | null;
}
/** Body of `POST /snap/api/:token/pay`: a standard method, or a provider's hosted checkout. */
export type PayChoice = { method: Method } | { provider: Provider };
export interface SnapState {
	merchant_name: string;
	payment: SnapPayment;
	methods: SnapMethod[];
	checkouts: SnapCheckout[];
	allow_simulate: boolean;
}

export interface ApiErrorEnvelope {
	error: { code: string; message: string; details?: unknown };
}

// MCP OAuth consent and connections (docs/mcp.md, "Authorization and consent" and "Connections").

export type OAuthScopeKey = 'read' | 'write' | 'live';

export interface OAuthScope {
	key: OAuthScopeKey;
	label: string;
	description: string;
	/** Pre-checked state: what the client asked for (`read` is always granted). */
	granted: boolean;
	/** A locked scope is always granted and cannot be unticked (`read`). */
	locked: boolean;
}

export interface OAuthAuthorizePreview {
	client: { client_id: string; client_name: string };
	/** Host the user is sent back to after deciding, e.g. `127.0.0.1`. */
	redirect_host: string;
	resource: string;
	scopes: OAuthScope[];
}

export interface OAuthDecisionBody {
	decision: 'allow' | 'deny';
	/** Final space-separated scopes; must include `read`. */
	scope: string;
}

export interface OAuthRedirect {
	redirect_url: string;
}

export interface OAuthGrant {
	id: string;
	client_id: string;
	client_name: string;
	scopes: OAuthScopeKey[];
	created_at: string;
	last_used_at: string | null;
	expires_at: string;
}
