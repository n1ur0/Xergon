/**
 * useWalletTx -- React hook for transaction signing and submission.
 *
 * Wraps the Nautilus signTx/submitTx functions with loading/error state
 * management. Provides signAndSubmit as a single async function and
 * tracks the txId of the last submitted transaction.
 *
 * Usage:
 *   const { signAndSubmit, txId, isLoading, error, reset } = useWalletTx()
 *   await signAndSubmit(unsignedTx)
 */

'use client'

import { useState, useCallback, useRef } from 'react'
import type { UnsignedTransaction } from '@/types/ergo-connector'
import { signTx, submitTx } from '@/lib/wallet/nautilus'

export interface UseWalletTxResult {
  /** Sign and submit an unsigned transaction in one step */
  signAndSubmit: (tx: UnsignedTransaction) => Promise<string>
  /** Sign an unsigned transaction (does not submit) */
  sign: (tx: UnsignedTransaction) => Promise<string>
  /** Submit a pre-signed transaction by its serialized JSON */
  submit: (signedTxJson: string) => Promise<string>
  /** Transaction ID of the last successfully submitted tx, or null */
  txId: string | null
  /** True while a transaction is being signed or submitted */
  isLoading: boolean
  /** Error message if the last operation failed, or null */
  error: string | null
  /** Clear the error state */
  clearError: () => void
  /** Reset all state (txId, error, loading) */
  reset: () => void
}

/**
 * Hook for signing and submitting Ergo transactions via Nautilus.
 *
 * Handles:
 * - Wallet-not-connected errors gracefully (clear error message)
 * - Loading state across async sign + submit
 * - Tracking the last submitted txId
 * - Concurrency guard (prevents multiple simultaneous submissions)
 */
export function useWalletTx(): UseWalletTxResult {
  const [txId, setTxId] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Ref to prevent concurrent submissions
  const submittingRef = useRef(false)

  const clearError = useCallback(() => {
    setError(null)
  }, [])

  const reset = useCallback(() => {
    setTxId(null)
    setError(null)
    setIsLoading(false)
    submittingRef.current = false
  }, [])

  /**
   * Sign and submit an unsigned transaction in one step.
   * Returns the transaction ID on success.
   */
  const signAndSubmit = useCallback(
    async (tx: UnsignedTransaction): Promise<string> => {
      // Concurrency guard
      if (submittingRef.current) {
        throw new Error('A transaction is already being submitted. Please wait.')
      }

      submittingRef.current = true
      setIsLoading(true)
      setError(null)

      try {
        const signed = await signTx(tx)
        const id = await submitTx(signed)
        setTxId(id)
        return id
      } catch (err) {
        const message = formatTxError(err)
        setError(message)
        throw new Error(message)
      } finally {
        setIsLoading(false)
        submittingRef.current = false
      }
    },
    []
  )

  /**
   * Sign only -- returns the serialized signed transaction JSON.
   * Useful when you want to inspect or store the signed tx before submitting.
   */
  const sign = useCallback(
    async (tx: UnsignedTransaction): Promise<string> => {
      setIsLoading(true)
      setError(null)

      try {
        const signed = await signTx(tx)
        return JSON.stringify(signed)
      } catch (err) {
        const message = formatTxError(err)
        setError(message)
        throw new Error(message)
      } finally {
        setIsLoading(false)
      }
    },
    []
  )

  /**
   * Submit only -- takes a pre-signed transaction JSON string.
   * Useful when you previously signed a tx and stored it.
   */
  const submit = useCallback(
    async (signedTxJson: string): Promise<string> => {
      setIsLoading(true)
      setError(null)

      try {
        const signedTx = JSON.parse(signedTxJson)
        const id = await submitTx(signedTx)
        setTxId(id)
        return id
      } catch (err) {
        const message = formatTxError(err)
        setError(message)
        throw new Error(message)
      } finally {
        setIsLoading(false)
      }
    },
    []
  )

  return {
    signAndSubmit,
    sign,
    submit,
    txId,
    isLoading,
    error,
    clearError,
    reset,
  }
}

/**
 * Normalize transaction errors into user-friendly messages.
 */
function formatTxError(err: unknown): string {
  if (err instanceof Error) {
    const msg = err.message

    // Known wallet errors
    if (msg.includes('not connected') || msg.includes('not installed')) {
      return 'Wallet is not connected. Please connect your Nautilus wallet and try again.'
    }

    if (msg.includes('rejected') || msg.includes('denied')) {
      return 'Transaction was rejected by the user.'
    }

    if (msg.includes('insufficient') || msg.includes('not enough')) {
      return 'Insufficient balance for this transaction.'
    }

    if (msg.includes('already being submitted')) {
      return msg
    }

    // Return the original error message for unknown errors
    return msg
  }

  if (typeof err === 'string') {
    return err
  }

  return 'An unexpected error occurred during the transaction.'
}
