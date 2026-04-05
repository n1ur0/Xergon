/**
 * Cross-chain payment bridge methods.
 */

import type { BridgeStatus, BridgeInvoice, BridgeChain } from './types';
import { XergonClientCore } from './client';

/**
 * Get bridge operational status.
 */
export async function getBridgeStatus(
  client: XergonClientCore,
): Promise<BridgeStatus> {
  return client.get<BridgeStatus>('/v1/bridge/status');
}

/**
 * List all invoices for the authenticated user.
 */
export async function getBridgeInvoices(
  client: XergonClientCore,
): Promise<BridgeInvoice[]> {
  return client.get<BridgeInvoice[]>('/v1/bridge/invoices');
}

/**
 * Get details for a specific invoice.
 */
export async function getBridgeInvoice(
  client: XergonClientCore,
  id: string,
): Promise<BridgeInvoice> {
  return client.get<BridgeInvoice>(
    `/v1/bridge/invoice/${encodeURIComponent(id)}`,
  );
}

/**
 * Create a new payment invoice.
 */
export async function createBridgeInvoice(
  client: XergonClientCore,
  amountNanoerg: string,
  chain: BridgeChain,
): Promise<BridgeInvoice> {
  return client.post<BridgeInvoice>('/v1/bridge/create-invoice', {
    amount_nanoerg: amountNanoerg,
    chain,
  });
}

/**
 * Confirm a payment for an invoice.
 */
export async function confirmBridgePayment(
  client: XergonClientCore,
  invoiceId: string,
  txHash: string,
): Promise<void> {
  await client.post('/v1/bridge/confirm', {
    invoice_id: invoiceId,
    tx_hash: txHash,
  });
}

/**
 * Request a refund for an invoice.
 */
export async function refundBridgeInvoice(
  client: XergonClientCore,
  invoiceId: string,
): Promise<void> {
  await client.post('/v1/bridge/refund', {
    invoice_id: invoiceId,
  });
}
