import { createHmac } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  constructEvent,
  getSignatureHeader,
  verifyWebhookSignature,
  WebhookSignatureError,
} from '../src/index.js';

const SECRET = 'whsec_test_0123456789abcdef';
const NOW = 1_800_000_000;
const now = (): number => NOW;

const BODY = JSON.stringify({
  id: 'evt_0192',
  type: 'payment.paid',
  livemode: false,
  created_at: '2026-11-26T10:00:00Z',
  data: { payment: { id: 'pay_1', order_id: 'INV-1', status: 'paid' } },
});

/** Independent reference implementation of the documented scheme, via node:crypto. */
function sign(body: string, t: number, secret: string = SECRET): string {
  return createHmac('sha256', secret).update(`${t}.${body}`).digest('hex');
}

function header(t: number, ...v1: string[]): string {
  return [`t=${t}`, ...v1.map((value) => `v1=${value}`)].join(',');
}

async function reasonOf(promise: Promise<unknown>): Promise<string> {
  try {
    await promise;
  } catch (error) {
    expect(error).toBeInstanceOf(WebhookSignatureError);
    return (error as WebhookSignatureError).reason;
  }
  throw new Error('expected verification to fail');
}

describe('verifyWebhookSignature', () => {
  it('accepts a valid signature over the raw body', async () => {
    await expect(
      verifyWebhookSignature({
        payload: BODY,
        header: header(NOW, sign(BODY, NOW)),
        secret: SECRET,
        now,
      }),
    ).resolves.toBeUndefined();
  });

  it('rejects a tampered body', async () => {
    const tampered = BODY.replace('"paid"', '"expired"');
    const reason = await reasonOf(
      verifyWebhookSignature({ payload: tampered, header: header(NOW, sign(BODY, NOW)), secret: SECRET, now }),
    );
    expect(reason).toBe('signature_mismatch');
  });

  it('rejects a body with only trailing whitespace added', async () => {
    const reason = await reasonOf(
      verifyWebhookSignature({
        payload: `${BODY}\n`,
        header: header(NOW, sign(BODY, NOW)),
        secret: SECRET,
        now,
      }),
    );
    expect(reason).toBe('signature_mismatch');
  });

  it('rejects a signature made with the wrong secret', async () => {
    const reason = await reasonOf(
      verifyWebhookSignature({
        payload: BODY,
        header: header(NOW, sign(BODY, NOW, 'whsec_other')),
        secret: SECRET,
        now,
      }),
    );
    expect(reason).toBe('signature_mismatch');
  });

  it('rejects a timestamp older than the tolerance', async () => {
    const stale = NOW - 301;
    const reason = await reasonOf(
      verifyWebhookSignature({ payload: BODY, header: header(stale, sign(BODY, stale)), secret: SECRET, now }),
    );
    expect(reason).toBe('timestamp_out_of_tolerance');
  });

  it('rejects a timestamp too far in the future', async () => {
    const future = NOW + 301;
    const reason = await reasonOf(
      verifyWebhookSignature({ payload: BODY, header: header(future, sign(BODY, future)), secret: SECRET, now }),
    );
    expect(reason).toBe('timestamp_out_of_tolerance');
  });

  it('accepts a timestamp exactly at the tolerance boundary', async () => {
    const edge = NOW - 300;
    await expect(
      verifyWebhookSignature({ payload: BODY, header: header(edge, sign(BODY, edge)), secret: SECRET, now }),
    ).resolves.toBeUndefined();
  });

  it('honours a custom tolerance', async () => {
    const t = NOW - 11;
    const reason = await reasonOf(
      verifyWebhookSignature({
        payload: BODY,
        header: header(t, sign(BODY, t)),
        secret: SECRET,
        toleranceSeconds: 10,
        now,
      }),
    );
    expect(reason).toBe('timestamp_out_of_tolerance');
  });

  it('accepts when any of several v1 entries matches (key rotation)', async () => {
    const stale = sign(BODY, NOW, 'whsec_retired');
    await expect(
      verifyWebhookSignature({
        payload: BODY,
        header: header(NOW, stale, sign(BODY, NOW)),
        secret: SECRET,
        now,
      }),
    ).resolves.toBeUndefined();
  });

  it('rejects when no v1 entry matches', async () => {
    const reason = await reasonOf(
      verifyWebhookSignature({
        payload: BODY,
        header: header(NOW, sign(BODY, NOW, 'whsec_a'), sign(BODY, NOW, 'whsec_b')),
        secret: SECRET,
        now,
      }),
    );
    expect(reason).toBe('signature_mismatch');
  });

  it('accepts uppercase hex digests', async () => {
    await expect(
      verifyWebhookSignature({
        payload: BODY,
        header: header(NOW, sign(BODY, NOW).toUpperCase()),
        secret: SECRET,
        now,
      }),
    ).resolves.toBeUndefined();
  });

  it('reports a missing header', async () => {
    expect(await reasonOf(verifyWebhookSignature({ payload: BODY, header: null, secret: SECRET, now }))).toBe(
      'missing_header',
    );
    expect(await reasonOf(verifyWebhookSignature({ payload: BODY, header: undefined, secret: SECRET, now }))).toBe(
      'missing_header',
    );
  });

  it('reports a header with no v1 signature', async () => {
    const reason = await reasonOf(
      verifyWebhookSignature({ payload: BODY, header: `t=${NOW}`, secret: SECRET, now }),
    );
    expect(reason).toBe('no_signature');
  });

  it('reports a header without a valid timestamp', async () => {
    const missing = await reasonOf(
      verifyWebhookSignature({ payload: BODY, header: `v1=${sign(BODY, NOW)}`, secret: SECRET, now }),
    );
    const notInteger = await reasonOf(
      verifyWebhookSignature({ payload: BODY, header: `t=12.5,v1=${sign(BODY, NOW)}`, secret: SECRET, now }),
    );
    expect(missing).toBe('malformed_header');
    expect(notInteger).toBe('malformed_header');
  });

  it('ignores unknown schemes alongside v1', async () => {
    await expect(
      verifyWebhookSignature({
        payload: BODY,
        header: `t=${NOW},v0=abcd,v1=${sign(BODY, NOW)}`,
        secret: SECRET,
        now,
      }),
    ).resolves.toBeUndefined();
  });

  it('requires a non-empty secret', async () => {
    await expect(
      verifyWebhookSignature({ payload: BODY, header: header(NOW, sign(BODY, NOW)), secret: '', now }),
    ).rejects.toBeInstanceOf(TypeError);
  });
});

describe('getSignatureHeader', () => {
  it('reads the header from a Fetch Headers object case-insensitively', () => {
    const headers = new Headers({ 'payrouters-signature': `t=${NOW},v1=abc` });
    expect(getSignatureHeader(headers)).toBe(`t=${NOW},v1=abc`);
  });

  it('reads the header from a Node-style record, joining repeated values', () => {
    expect(getSignatureHeader({ 'PAYROUTERS-SIGNATURE': ['t=1,v1=a', 'v1=b'] })).toBe('t=1,v1=a,v1=b');
    expect(getSignatureHeader({ 'content-type': 'application/json', 'PayRouters-Signature': 't=1,v1=a' })).toBe(
      't=1,v1=a',
    );
  });

  it('returns undefined when absent', () => {
    expect(getSignatureHeader({ 'content-type': 'application/json' })).toBeUndefined();
    expect(getSignatureHeader(new Headers())).toBeUndefined();
  });
});

describe('constructEvent', () => {
  it('verifies and returns the parsed event', async () => {
    const event = await constructEvent(BODY, header(NOW, sign(BODY, NOW)), SECRET, { now });
    expect(event.id).toBe('evt_0192');
    expect(event.type).toBe('payment.paid');
    expect(event.data.payment.status).toBe('paid');
  });

  it('throws without parsing when the signature is bad', async () => {
    await expect(
      constructEvent(BODY, header(NOW, sign(BODY, NOW, 'whsec_other')), SECRET, { now }),
    ).rejects.toBeInstanceOf(WebhookSignatureError);
  });

  it('honours toleranceSeconds', async () => {
    const t = NOW - 60;
    await expect(
      constructEvent(BODY, header(t, sign(BODY, t)), SECRET, { now, toleranceSeconds: 30 }),
    ).rejects.toMatchObject({ reason: 'timestamp_out_of_tolerance' });
  });
});
