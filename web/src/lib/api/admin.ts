import { downloadFile, request, requestVoid } from '#lib/api/client';
import type {
	AdminCreatePaymentBody,
	AdminPayment,
	ApiKeyCreated,
	BreakdownRow,
	CashInPage,
	Delivery,
	Me,
	Meta,
	Page,
	PaymentDetail,
	Product,
	ProductCreated,
	ProviderAccount,
	ProviderCreateBody,
	ProviderPatchBody,
	ProviderTestResult,
	SimulateOutcome,
	StatsBy,
	StatsSummary,
	TimeseriesPoint,
	WebhookEvent
} from '#lib/api/types';

// Query shapes mirror the contract's filter parameters. Empty values are dropped by the client.
export type PaymentsQuery = {
	mode?: string;
	status?: string;
	provider?: string;
	product_id?: string;
	q?: string;
	from?: string;
	to?: string;
	limit?: number;
	cursor?: string;
};

export type StatsQuery = {
	mode: string;
	from?: string;
	to?: string;
	product_id?: string;
	provider?: string;
};

export type CashInsQuery = StatsQuery & { method?: string; limit?: number; cursor?: string };

export type WebhookEventsQuery = {
	provider_account_id?: string;
	outcome?: string;
	limit?: number;
	cursor?: string;
};

export type DeliveriesQuery = {
	status?: string;
	product_id?: string;
	payment_id?: string;
	limit?: number;
	cursor?: string;
};

export const adminApi = {
	/** Validates a token: pass `token` explicitly (login) or omit it to use the stored one. */
	me: (opts: { token?: string | null } = {}) =>
		request<Me>('/admin/me', { token: opts.token, redirectOn401: false }),
	meta: () => request<Meta>('/admin/meta'),

	// Products
	listProducts: () => request<{ data: Product[] }>('/admin/products'),
	createProduct: (body: { name: string; webhook_url?: string; default_provider?: string }) =>
		request<ProductCreated>('/admin/products', { method: 'POST', body }),
	updateProduct: (
		id: string,
		body: { name?: string; webhook_url?: string | null; default_provider?: string | null }
	) => request<Product>(`/admin/products/${id}`, { method: 'PATCH', body }),
	archiveProduct: (id: string) => requestVoid(`/admin/products/${id}`, { method: 'DELETE' }),
	rotateWebhookSecret: (id: string) =>
		request<{ webhook_secret: string }>(`/admin/products/${id}/webhook-secret/rotate`, {
			method: 'POST'
		}),
	createApiKey: (id: string, body: { mode: string; name: string }) =>
		request<ApiKeyCreated>(`/admin/products/${id}/api-keys`, { method: 'POST', body }),
	revokeApiKey: (productId: string, keyId: string) =>
		requestVoid(`/admin/products/${productId}/api-keys/${keyId}`, { method: 'DELETE' }),

	// Provider accounts
	listProviders: () => request<{ data: ProviderAccount[] }>('/admin/providers'),
	createProvider: (body: ProviderCreateBody) =>
		request<ProviderAccount>('/admin/providers', { method: 'POST', body }),
	updateProvider: (id: string, body: ProviderPatchBody) =>
		request<ProviderAccount>(`/admin/providers/${id}`, { method: 'PATCH', body }),
	deleteProvider: (id: string) => requestVoid(`/admin/providers/${id}`, { method: 'DELETE' }),
	testProvider: (id: string) =>
		request<ProviderTestResult>(`/admin/providers/${id}/test`, { method: 'POST' }),

	// Payments
	listPayments: (query: PaymentsQuery) => request<Page<AdminPayment>>('/admin/payments', { query }),
	getPayment: (id: string) => request<PaymentDetail>(`/admin/payments/${id}`),
	createPayment: (body: AdminCreatePaymentBody) =>
		request<AdminPayment>('/admin/payments', { method: 'POST', body }),
	syncPayment: (id: string) =>
		request<AdminPayment>(`/admin/payments/${id}/sync`, { method: 'POST' }),
	simulatePayment: (id: string, outcome: SimulateOutcome) =>
		request<AdminPayment>(`/admin/payments/${id}/simulate`, {
			method: 'POST',
			body: { outcome }
		}),

	// Webhook inbox & deliveries
	listWebhookEvents: (query: WebhookEventsQuery) =>
		request<Page<WebhookEvent>>('/admin/webhook-events', { query }),
	listDeliveries: (query: DeliveriesQuery) =>
		request<Page<Delivery>>('/admin/deliveries', { query }),
	retryDelivery: (id: string) =>
		request<Delivery>(`/admin/deliveries/${id}/retry`, { method: 'POST' }),

	// Stats & cash-in recap
	statsSummary: (query: StatsQuery) => request<StatsSummary>('/admin/stats/summary', { query }),
	statsTimeseries: (query: StatsQuery) =>
		request<{ data: TimeseriesPoint[] }>('/admin/stats/timeseries', { query }),
	statsBreakdown: (by: StatsBy, query: StatsQuery) =>
		request<{ data: BreakdownRow[] }>('/admin/stats/breakdown', { query: { by, ...query } }),
	listCashIns: (query: CashInsQuery) => request<CashInPage>('/admin/cash-ins', { query }),
	downloadCashInsCsv: (query: CashInsQuery, filename: string) =>
		downloadFile('/admin/cash-ins/export.csv', query, filename)
};
