/**
 * Write path: builds, simulates, signs and submits a `deprader_router` call
 * as the connected Freighter wallet. Read-only calls live in `contract.ts`.
 */
import {
  BASE_FEE,
  Contract,
  rpc,
  scValToNative,
  TransactionBuilder,
  type Transaction,
  type xdr,
} from "@stellar/stellar-sdk";
import { signTransaction } from "@stellar/freighter-api";
import { CONTRACT_ID, RPC_URL } from "../config";
import { networkPassphrase } from "../wallet/network";

const server = new rpc.Server(RPC_URL);

export interface InvokeResult {
  /** Transaction hash, for linking to the block explorer. */
  hash: string;
  /** The contract's return value, decoded to a native JS value. */
  value: unknown;
}

/**
 * Invokes `method` on the router as `signer`. Resolves once the transaction is
 * confirmed on the ledger; rejects with a readable message otherwise.
 */
export async function invokeContract(signer: string, method: string, args: xdr.ScVal[]): Promise<InvokeResult> {
  const passphrase = networkPassphrase();
  const account = await server.getAccount(signer);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: passphrase })
    .addOperation(new Contract(CONTRACT_ID).call(method, ...args))
    .setTimeout(60)
    .build();

  const simulation = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(simulation)) {
    throw new Error(`The contract rejected this transaction: ${simulation.error}`);
  }
  const prepared = rpc.assembleTransaction(tx, simulation).build();

  const signed = await signTransaction(prepared.toXDR(), { networkPassphrase: passphrase, address: signer });
  if (signed.error) throw new Error(signed.error.message);

  const signedTx = TransactionBuilder.fromXDR(signed.signedTxXdr, passphrase) as Transaction;
  const sent = await server.sendTransaction(signedTx);
  if (sent.status === "ERROR") throw new Error("The network rejected this transaction. Check your balance and try again.");

  const outcome = await server.pollTransaction(sent.hash);
  if (outcome.status !== "SUCCESS") {
    throw new Error(`The transaction failed on the network (${outcome.status}).`);
  }
  return {
    hash: sent.hash,
    value: outcome.returnValue ? scValToNative(outcome.returnValue) : undefined,
  };
}
