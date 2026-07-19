#!/usr/bin/env node
// scripts/sep10-sign-challenge.mjs — sign a SEP-10 challenge transaction.
//
// Stands in for the "wallet" half of the handshake (Lobstr / Bitnovo / any
// SEP-0007 wallet) so scripts/smoke-test-sep10.sh can reproduce the full
// challenge -> sign -> verify loop over plain curl + this one signing step,
// without a real mobile device.
//
// Usage: node scripts/sep10-sign-challenge.mjs <base64-xdr> <client-secret> <network-passphrase>

import { Keypair, Networks, Transaction } from '@stellar/stellar-sdk';

const [xdr, secret, networkPassphraseArg] = process.argv.slice(2);

if (!xdr || !secret) {
  console.error('usage: sep10-sign-challenge.mjs <base64-xdr> <client-secret> [network-passphrase]');
  process.exit(1);
}

const networkPassphrase = networkPassphraseArg || Networks.TESTNET;
const keypair = Keypair.fromSecret(secret);
const transaction = new Transaction(xdr, networkPassphrase);
transaction.sign(keypair);

process.stdout.write(transaction.toXDR());
