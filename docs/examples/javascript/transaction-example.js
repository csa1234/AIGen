import axios from "axios";
import crypto from "crypto";

// Transaction submission example.
// NOTE: The exact transaction schema + signature rules depend on the node implementation.
// This file demonstrates the JSON-RPC request shape and a typical workflow.

const RPC_URL = process.env.AIGEN_RPC_URL || "http://localhost:9944";

async function rpc(method, params = [], id = 1) {
  const payload = { jsonrpc: "2.0", id, method, params };
  const { data } = await axios.post(RPC_URL, payload, {
    headers: { "content-type": "application/json" }
  });
  if (data && data.error) throw new Error(data.error.message || "RPC error");
  return data.result;
}

async function main() {
  // TransactionResponse schema (node/src/rpc/types.rs). The node validates:
  // - sender_public_key is required and must be 32 bytes
  // - sender must match the address derived from sender_public_key
  // - tx_hash must match transaction contents
  // - signature must be an Ed25519 signature over tx_hash bytes

  const sender = "0xSENDER_ADDRESS";
  const sender_public_key = "0xSENDER_PUBLIC_KEY_32_BYTES";
  const receiver = "0xRECEIVER_ADDRESS";

  const amount = 1;
  const timestamp = Math.floor(Date.now() / 1000);
  const nonce = 1;
  const priority = false;
  const chain_id = 1;

  const fee_base = 1;
  const fee_priority = 0;
  const fee_burn = 0;

  // payload is hex in RPC, but tx_hash commits to *base64* payload bytes.
  const payloadHex = null;
  const payloadBytes = payloadHex ? Buffer.from(payloadHex.replace(/^0x/, ""), "hex") : null;
  const payloadBase64 = payloadBytes ? payloadBytes.toString("base64") : null;

  // This object matches blockchain_core::hash_transaction
  const hashPayload = {
    sender,
    receiver,
    amount,
    timestamp,
    nonce,
    priority,
    chain_id,
    fee: {
      base_fee: fee_base,
      priority_fee: fee_priority,
      burn_amount: fee_burn
    },
    payload: payloadBase64
  };

  const hashBytes = Buffer.from(JSON.stringify(hashPayload), "utf8");
  const tx_hash = "0x" + crypto.createHash("sha256").update(hashBytes).digest("hex");

  // TODO: sign tx_hash bytes with the sender's Ed25519 key.
  // signature must be 64-byte hex, e.g. 0x + 128 hex chars.
  const signature = "0xSIGNATURE_64_BYTES";

  const tx = {
    sender,
    sender_public_key,
    receiver,
    amount,
    signature,
    timestamp,
    nonce,
    priority,
    tx_hash,
    fee_base,
    fee_priority,
    fee_burn,
    chain_id,
    payload: payloadHex,
    ceo_signature: null
  };

  console.log("Submitting tx to:", RPC_URL);
  console.log("tx:", tx);

  const res = await rpc("submitTransaction", [tx]);
  console.log("submitTransaction result:", res);

  const pending = await rpc("getPendingTransactions", [50]);
  console.log("getPendingTransactions(50):", pending);
}

main().catch((e) => {
  console.error("Error:", e);
  process.exitCode = 1;
});
