// Runtime-agnostic HMAC-SHA256 over Web Crypto (Node 18+, Bun, Deno, Workers, browsers).

const encoder = new TextEncoder();
let subtlePromise: Promise<SubtleCrypto> | undefined;

async function resolveSubtle(): Promise<SubtleCrypto> {
  const webCrypto = globalThis.crypto;
  if (webCrypto?.subtle) return webCrypto.subtle;
  // Node 18 only exposes Web Crypto on the global with --experimental-global-webcrypto.
  // Platform-specific fallback: a static `node:crypto` import would force every bundle
  // (including Workers, which always take the branch above) to resolve a Node builtin.
  // The specifier is held in a variable so bundlers leave it alone.
  const specifier = 'node:crypto';
  const nodeCrypto = (await import(specifier)) as { webcrypto: Crypto };
  return nodeCrypto.webcrypto.subtle;
}

function subtle(): Promise<SubtleCrypto> {
  subtlePromise ??= resolveSubtle();
  return subtlePromise;
}

/** Returns lowercase hex of HMAC-SHA256(key, message), both UTF-8 encoded. */
export async function hmacSha256Hex(key: string, message: string): Promise<string> {
  const cryptoKey = await (await subtle()).importKey(
    'raw',
    encoder.encode(key),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  );
  const mac = await (await subtle()).sign('HMAC', cryptoKey, encoder.encode(message));
  let hex = '';
  for (const byte of new Uint8Array(mac)) {
    hex += byte.toString(16).padStart(2, '0');
  }
  return hex;
}

/**
 * Compares two strings without short-circuiting on the first differing character.
 * Length is compared first; for fixed-length hex digests that reveals nothing secret.
 */
export function timingSafeEqualString(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}
