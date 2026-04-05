/**
 * Browser-safe HMAC-SHA256 using Web Crypto API.
 *
 * Re-exports from the main auth module for wallet-specific use cases.
 * This module exists so consumers who only need wallet helpers
 * don't need to import the full SDK.
 */

export { hmacSign, hmacVerify, buildHmacPayload } from '../auth';
