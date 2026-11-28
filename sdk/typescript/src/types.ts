// Wire types for the PayRouters product API (/v1) and webhook events.
// Field names mirror docs/api.md exactly (snake_case). Amounts are integer rupiah.

export const MODES = ['sandbox', 'live'] as const;
export type Mode = (typeof MODES)[number];

export const PROVIDERS = ['midtrans', 'xendit', 'idrx', 'mayar', 'paypal', 'mock'] as const;
export type Provider = (typeof PROVIDERS)[number];

export const CHECKOUTS = ['snap', 'provider'] as const;
/** `snap`: PayRouters hosted page over the provider core API. `provider`: provider-hosted page. */
export type Checkout = (typeof CHECKOUTS)[number];

export const METHOD_CODES = [
  'va_bca',
  'va_bni',
  'va_bri',
  'va_mandiri',
  'va_permata',
  'qris',
] as const;
export type MethodCode = (typeof METHOD_CODES)[number];

export const METHOD_GROUPS = ['virtual_account', 'qris'] as const;
export type MethodGroup = (typeof METHOD_GROUPS)[number];

export const BANK_CODES = ['bca', 'bni', 'bri', 'mandiri', 'permata'] as const;
export type BankCode = (typeof BANK_CODES)[number];

export const PAYMENT_STATUSES = ['pending', 'paid', 'expired', 'failed', 'cancelled'] as const;
/** Every status except `pending` is terminal; `paid` is final. */
export type PaymentStatus = (typeof PAYMENT_STATUSES)[number];

export const PAYMENT_EVENT_TYPES = [
  'payment.paid',
  'payment.expired',
  'payment.failed',
  'payment.cancelled',
] as const;
export type PaymentEventType = (typeof PAYMENT_EVENT_TYPES)[number];

export interface Customer {
  name?: string | null;
  email?: string | null;
  phone?: string | null;
}

export interface Item {
  id: string;
  name: string;
  /** Integer rupiah per unit. */
  price: number;
  quantity: number;
}

export interface VirtualAccountInstructions {
  type: 'virtual_account';
  bank: BankCode;
  account_number: string | null;
  account_name: string | null;
  /** Mandiri bill payment only; `account_number` is then `null`. */
  biller_code: string | null;
  bill_key: string | null;
}

export interface QrisInstructions {
  type: 'qris';
  qr_string: string;
  qr_image_url: string | null;
}

export interface RedirectInstructions {
  type: 'redirect';
  url: string;
}

/** Customer-facing payment instructions, discriminated by `type`. */
export type Instructions = VirtualAccountInstructions | QrisInstructions | RedirectInstructions;

export interface Payment {
  id: string;
  order_id: string;
  status: PaymentStatus;
  mode: Mode;
  livemode: boolean;
  amount: number;
  currency: 'IDR';
  /** `null` until paid. */
  fee_amount: number | null;
  /** `null` until paid. */
  net_amount: number | null;
  /**
   * Exact total the customer must transfer when the provider adds its fee on top (IDRX VA).
   * `null` means pay `amount`. Display `payable_amount ?? amount` as "Total to pay".
   */
  payable_amount: number | null;
  provider: Provider;
  method: MethodCode | null;
  checkout: Checkout;
  snap_url: string | null;
  /** `null` until a method or checkout exists. */
  instructions: Instructions | null;
  description: string | null;
  customer: Customer | null;
  items: Item[];
  metadata: Record<string, string>;
  return_url: string | null;
  provider_ref: string | null;
  expires_at: string;
  paid_at: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * Provider-specific options. IDRX: `destination_wallet_address`, `chain_id`
 * (fall back to the provider account defaults). IDRX `amount` is the IDRX to mint (min 20000).
 * Mayar and PayPal take none: PayPal converts `amount` to its charge currency at the account rate.
 */
export interface ProviderOptions {
  destination_wallet_address?: string;
  chain_id?: number;
  [key: string]: unknown;
}

export interface CreatePaymentParams {
  /** Unique per product and mode, at most 64 characters. */
  order_id: string;
  /** Integer rupiah, at least 1. */
  amount: number;
  provider?: Provider;
  /** Defaults to `snap`. */
  checkout?: Checkout;
  method?: MethodCode;
  description?: string;
  customer?: Customer;
  /** When given, prices must sum to `amount`. */
  items?: Item[];
  metadata?: Record<string, string>;
  /** Seconds until expiry; default 86400, minimum 300. */
  expires_in?: number;
  return_url?: string;
  provider_options?: ProviderOptions;
}

export interface CreatePaymentResult {
  payment: Payment;
  /**
   * `true` when the server returned an existing pending payment for the same
   * `order_id` (HTTP 200 with `PayRouters-Idempotent-Replay: true`) instead of creating one.
   */
  replayed: boolean;
}

export interface ListPaymentsParams {
  order_id?: string;
  status?: PaymentStatus;
  /** Default 50, maximum 200. */
  limit?: number;
  cursor?: string | undefined;
}

export interface ListResult<T> {
  data: T[];
  /** Opaque cursor for the next page, `null` on the last page. */
  next_cursor: string | null;
}

export interface MethodInfo {
  code: MethodCode;
  label: string;
  group: MethodGroup;
}

export interface ProviderMethodCatalog {
  provider: Provider;
  checkout_provider: boolean;
  methods: MethodInfo[];
}

export interface MethodCatalogResponse {
  /** Only providers with an enabled account in this key's mode. */
  data: ProviderMethodCatalog[];
}

export interface PayRouterEvent {
  /** `evt_…`; dedupe on this (delivery is at-least-once). */
  id: string;
  type: PaymentEventType;
  livemode: boolean;
  created_at: string;
  data: {
    payment: Payment;
  };
}
