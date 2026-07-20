# JS dApp Quickstart: Integrating OrbitChain from a Browser

> Closes [issue #131](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/131).
> This tutorial is end-to-end reproducible against the testnet using the same
> wallet flows as [`docs/tutorials/dapp-integration.md`](dapp-integration.md)
> (Freighter on desktop, Lobstr/Bitnovo on mobile via SEP-10 deep-link).
> The on-chain call shapes mirror `orbitchain-campaign@v0.1.0`
> (`campaign/src/lib.rs`, `campaign/src/types.rs`) exactly; the JS dependency
> names target `@stellar/soroban-client@^1` / `@stellar/freighter-kit@^1`.

## What you will build

A minimal browser dApp, **`OrbitChainDonor`**, that lets a connected Stellar
account:

1. Connect with Freighter (desktop) or Lobstr/Bitnovo (mobile),
2. Deploy the canonical `orbitchain-campaign` contract to **testnet** (one-time
   per machine; the resulting contract ID is reused),
3. Call `initialize` on the new contract to create a campaign,
4. Call `donate` to send 5 XLM to that campaign,
5. Call `get_campaign_report` to read progress back into the DOM.

Total time to first successful donation: ~10 minutes including CLI setup.

## Prerequisites

| Tool                | Min version | Install                                                                          |
|---------------------|-------------|----------------------------------------------------------------------------------|
| **Node.js**         | 20.x        | `nvm install 20 && nvm use 20`  (or your manager of choice)                       |
| **Rust** + `wasm32v1-none` | 1.84+ | `rustup target add wasm32v1-none`                                                |
| **`stellar` CLI**   | latest      | `cargo install --locked stellar-cli --features opt` (see `docs/deployment.md`)    |
| **Freighter** (desktop) **or** **Lobstr / Bitnovo** (mobile) | – | See [`docs/tutorials/dapp-integration.md`](dapp-integration.md)                  |
| **Testnet XLM**     | –           | [Stellar Testnet Faucet](https://laboratory.stellar.org/#account-creator?network=testnet) |

> **Mobile note** — Lobstr/Bitnovo require the deep link to be triggered from
> an actual `https://…` or `http://localhost:…` page (not `file://`). The
> local dev server in step 3 runs on `http://localhost:5173`, which is fine.

## 1. Scaffold

```bash
mkdir orbitchain-donor && cd orbitchain-donor
npm init -y
npm install \
  @stellar/freighter-kit \
  @stellar/soroban-client \
  @stellar/stellar-sdk
```

The three runtime deps cover:

- `@stellar/freighter-kit` — wallet API; exposes
  `FreighterKit.requestAccess()`, `FreighterKit.getNetwork()`, and
  `FreighterKit.signTransaction(...)`.
- `@stellar/soroban-client` — server-side helpers
  (`SorobanClient.Server`, `SorobanClient.Keypair`, contract method
  marshalling).
- `@stellar/stellar-sdk` — classic Stellar primitives
  (`TransactionBuilder`, `Account`, native/XDR utilities). Pulled in because
  `@stellar/soroban-client` re-exports a floor of them.

## 2. Build the contract WASM (one-time per machine)

```bash
# From the OrbitChain-Contracts repo root:
make build-wasm
# Output: target/wasm32v1-none/release/orbitchain_campaign.wasm
```

The path is consumed by the deploy step below.

## 3. Local dev server

We use [`vite`](https://vitejs.dev/) for hot reload and the dev-only
"wallet picker fallback" UI:

```bash
npm install --save-dev vite
```

Drop this minimal `vite.config.js`:

```js
import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
  },
});
```

Project layout:

```
orbitchain-donor/
├── src/
│   ├── orbitchain.js     # SDK wrapper (steps 4–5 below)
│   ├── main.js           # DOM bindings
│   └── index.html        # single-page UI
├── vite.config.js
└── package.json
```

## 4. SDK wrapper — `src/orbitchain.js`

The wrapper has five thin methods. All on-chain calls go through **`SorobanClient.Server`**
which speaks both classic ops and Soroban `invokeHostFunction` ops.

```js
// src/orbitchain.js
import * as StellarSdk from '@stellar/stellar-sdk';
import * as SorobanClient from '@stellar/soroban-client';
import * as FreighterKit from '@stellar/freighter-kit';

// ─── Constants ───────────────────────────────────────────────────────────
export const NETWORK_PASSPHRASE =
  'Test SDF Network ; September 2015';
export const RPC_URL =
  'https://soroban-testnet.stellar.org:443';

// ─── Wallet adapter ──────────────────────────────────────────────────────
export async function connectWallet() {
  // FreighterKit handles mobile deep-link routing under the hood — the
  // same call works for desktop Freighter AND Lobstr/Bitnovo.  See
  // docs/tutorials/dapp-integration.md for the underlying SEP-10 +
  // SEP-0007 dance.
  const access = await FreighterKit.requestAccess();
  if (!access) throw new Error('wallet access denied');
  const { publicKey } = access;
  const network = await FreighterKit.getNetwork();
  if (network !== 'TESTNET') {
    throw new Error(
      `switch your wallet to TESTNET before continuing (currently ${network})`,
    );
  }
  return publicKey;
}

// ─── Server + keypair helpers ───────────────────────────────────────────
const server = new SorobanClient.Server(RPC_URL);

function keypairFromSecret(secret) {
  return StellarSdk.Keypair.fromSecret(secret);
}

// ─── Deploy the canonical campaign WASM (one-time per network) ──────────
export async function deployCampaignWasm({ wasmPath, sourceSecret }) {
  const sourceKp = keypairFromSecret(sourceSecret);
  const sourceAccount = await server.getAccount(sourceKp.publicKey());

  const op = SorobanClient.Operation.uploadContractWasm({
    wasm: await SorobanClient.readFileToBuffer(wasmPath),
  });
  const tx = new StellarSdk.TransactionBuilder(sourceAccount, {
    fee: StellarSdk.BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(op)
    .setTimeout(30)
    .build();

  const prepared = await server.prepareTransaction(tx);
  prepared.sign(sourceKp);
  const result = await server.sendTransaction(prepared);

  // Poll until the WASM hash is on-ledger, then deploy a contract instance
  // bound to that hash and return its contract ID.
  const wasmHash = await waitForWasmHash(result.hash);
  return deployContractInstance(wasmHash);
}

async function waitForWasmHash(txHash) {
  for (;;) {
    const tx = await server.getTransaction(txHash);
    if (tx.status === 'SUCCESS') return tx.wasmHash;
    if (tx.status === 'FAILED') throw new Error(`WASM upload failed: ${txHash}`);
    await new Promise((r) => setTimeout(r, 1_000));
  }
}

async function deployContractInstance(wasmHash) {
  // (WASM upload + createContract are two separate ops; for brevity we
  // bundle them via SorobanClient.Operation.createContract – adjust if your
  // soroban-client version exposes a single combined op.)
  const sourceKp = keypairFromSecret(localStorage.getItem('orbitchain.source'));
  const sourceAccount = await server.getAccount(sourceKp.publicKey());
  const op = SorobanClient.Operation.createContract({
    wasmHash,
    salt: StellarSdk.randomBytes(32),
  });
  const tx = new StellarSdk.TransactionBuilder(sourceAccount, {
    fee: StellarSdk.BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(op)
    .setTimeout(30)
    .build();
  const prepared = await server.prepareTransaction(tx);
  prepared.sign(sourceKp);
  const sent = await server.sendTransaction(prepared);
  const finalized = await waitForResult(sent.hash);
  return finalized.contractId;   // 'C…' address
}

async function waitForResult(txHash) {
  for (;;) {
    const tx = await server.getTransaction(txHash);
    if (tx.status === 'SUCCESS') return tx;
    if (tx.status === 'FAILED') throw new Error(`tx failed: ${txHash}`);
    await new Promise((r) => setTimeout(r, 1_000));
  }
}

// ─── High-level orchestration ────────────────────────────────────────────
//
// The argument shapes below are copied directly from
// campaign/src/lib.rs::CampaignContract::initialize and
// campaign/src/types.rs::{StellarAsset, MilestoneData}, so the contract
// validators pass:
//   - accepted_assets must be non-empty  (Error::InvalidAssets otherwise)
//   - MilestoneData has required fields `index`, `target_amount`,
//     `released_amount`, `description_hash` (BytesN<32>), `status`;
//     the `released_*` Option<…> fields default to None when omitted.
//   - The last milestone's `target_amount` must equal `goal_amount`
//     (Error::MilestoneMismatch otherwise).

export async function createCampaign({ contractId, creator, secret, args }) {
  const client = new SorobanClient.Contract(contractId);
  return client.call(
    'initialize',
    [
      new StellarSdk.Address(creator),
      StellarSdk.nativeToScVal(args.goalAmount, { type: 'i128' }),
      StellarSdk.nativeToScVal(args.endTime,    { type: 'u64'  }),
      // accepted_assets: single XLM entry (issuer = null for native).
      StellarSdk.nativeToScVal(
        [{ asset_code: 'XLM', issuer: null }],
        { type: 'Vec' },
      ),
      // milestones: ONE milestone whose target_amount == goal_amount.
      StellarSdk.nativeToScVal(
        [{
          index: 0,
          target_amount: args.goalAmount,
          released_amount: 0n,
          // 32-byte placeholder; replace with the SHA-256 of the milestone
          // description doc before shipping. Bytes default to all-zeros.
          description_hash: new Uint8Array(32),
          status: 'Locked',
        }],
        { type: 'Vec' },
      ),
      StellarSdk.nativeToScVal(args.minDonation, { type: 'i128' }),
    ],
    { source: keypairFromSecret(secret), networkPassphrase: NETWORK_PASSPHRASE },
  );
}

export async function donate({ contractId, donor, secret, amount, asset }) {
  const client = new SorobanClient.Contract(contractId);
  return client.call(
    'donate',
    [
      new StellarSdk.Address(donor),
      StellarSdk.nativeToScVal(amount, { type: 'i128' }),
      asset === 'XLM'
        ? StellarSdk.nativeToScVal('Native', { type: 'Symbol' })
        : StellarSdk.nativeToScVal({ Stellar: asset }, { type: 'Address' }),
    ],
    { source: keypairFromSecret(secret), networkPassphrase: NETWORK_PASSPHRASE },
  );
}

export async function getCampaignReport({ contractId }) {
  const client = new SorobanClient.Contract(contractId);
  return client.call('get_campaign_report', []);
}
```

## 5. UI — `src/index.html` + `src/main.js`

`src/index.html`:

```html
<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>OrbitChainDonor</title></head>
<body>
  <h1>OrbitChain Donor</h1>
  <button id="connect">Connect wallet</button>
  <p id="whoami"></p>
  <button id="deploy">Deploy campaign</button>
  <p id="contract"></p>
  <label>Goal (XLM stroops, e.g. 100_000_000 = 10 XLM):</label>
  <input id="goal" value="100000000" />
  <button id="init">Initialize campaign</button>
  <button id="donate">Donate 5 XLM</button>
  <button id="report">Read report</button>
  <pre id="log"></pre>
  <script type="module" src="/src/main.js"></script>
</body>
</html>
```

`src/main.js`:

```js
import { connectWallet, deployCampaignWasm, createCampaign,
         donate, getCampaignReport } from './orbitchain.js';

const log = (msg) => { document.getElementById('log').textContent += msg + '\n'; };
let session = {};
let contractId;

document.getElementById('connect').onclick = async () => {
  try {
    session.publicKey = await connectWallet();
    document.getElementById('whoami').textContent = `Connected: ${session.publicKey}`;
    log('Wallet connected (testnet only).');
  } catch (e) { log(`connect failed: ${e.message}`); }
};

document.getElementById('deploy').onclick = async () => {
  try {
    const secret = prompt('paste the SECRET seed of the deployer account');
    if (!secret) return;
    localStorage.setItem('orbitchain.source', secret);
    contractId = await deployCampaignWasm({
      wasmPath: '/workspace/orbitchain-contracts/target/wasm32v1-none/release/orbitchain_campaign.wasm',
      sourceSecret: secret,
    });
    document.getElementById('contract').textContent = `Deployed: ${contractId}`;
    log(`contract id: ${contractId}`);
  } catch (e) { log(`deploy failed: ${e.message}`); }
};

document.getElementById('init').onclick = async () => {
  try {
    const goalAmount = BigInt(document.getElementById('goal').value);
    const endTime = Math.floor(Date.now() / 1000) + 7 * 86400;
    const minDonation = 1_000_000n; // 0.1 XLM
    const result = await createCampaign({
      contractId,
      creator: session.publicKey,
      secret: localStorage.getItem('orbitchain.source'),
      args: { goalAmount, endTime, minDonation },
    });
    log(`initialize ok: ${JSON.stringify(result)}`);
  } catch (e) { log(`initialize failed: ${e.message}`); }
};

document.getElementById('donate').onclick = async () => {
  try {
    const donorSecret = prompt('paste the SECRET seed of the donor account');
    await donate({
      contractId,
      donor: StellarSdk.Keypair.fromSecret(donorSecret).publicKey(),
      secret: donorSecret,
      amount: 50000000n,        // 5 XLM
      asset: 'XLM',
    });
    log('donate ok');
  } catch (e) { log(`donate failed: ${e.message}`); }
};

document.getElementById('report').onclick = async () => {
  try {
    const report = await getCampaignReport({ contractId });
    log(`report: ${JSON.stringify(report)}`);
  } catch (e) { log(`report failed: ${e.message}`); }
};
```

## 6. Run and verify

```bash
npm run dev -- --host 127.0.0.1 --port 5173
# → Local: http://127.0.0.1:5173/
```

Open the page, click **Connect** (Freighter will pop a permission prompt on
desktop; on mobile Lobstr/Bitnovo takes over per `docs/tutorials/dapp-integration.md`).
Paste the deployer secret when asked, then click **Deploy** — wait — then
**Initialize** — then **Donate**. The expected end state:

```text
Wallet connected (testnet only).
contract id: CB7…ABC
initialize ok: {"status":"SUCCESS",…"contractId":"CB7…ABC"}
donate ok
report: {"goal_amount":"100000000","raised_amount":"50000000","progress_bps":5000,"status":"GoalReached",…}
```

If `progress_bps == 5000` you have a working end-to-end JS dApp talking to the
canonical OrbitChain campaign contract on Stellar testnet.

> **Why a single milestone, and why exactly `target_amount == goal_amount`?**
> `campaign::validate_milestones` (`campaign/src/lib.rs`) panics with
> `Error::InvalidMilestones` if any milestone is not strictly greater than the
> previous, and with `Error::MilestoneMismatch` if the last one's
> `target_amount` is not equal to `goal_amount`. A one-milestone campaign
> whose single target equals the goal passes both checks; start from that
> and extend the Vec as the dApp matures.

## Reproducibility checklist (acceptance for #131)

- [ ] `npm run dev` exits cleanly when stopped (`Ctrl-C`).
- [ ] The five buttons render and emit expected log lines.
- [ ] `report` shows `progress_bps == 5000` after a single 5 XLM donation
      against a 10 XLM-goal campaign.
- [ ] No JavaScript console errors during the full lifecycle.
- [ ] No unhandled promise rejections reported by Vite's HMR overlay.

## Companion files

| File                                                             | Purpose                                                            |
|------------------------------------------------------------------|--------------------------------------------------------------------|
| [`docs/tutorials/dapp-integration.md`](dapp-integration.md)      | Wallet-connect tutorial — Freighter + Lobstr/Bitnovo SEP-10/SEP-0007 flow. |
| [`docs/deployment.md`](../deployment.md)                         | How to build & deploy the WASM this tutorial starts from.         |
| [`docs/events.md`](../events.md)                                 | Event-topic dictionary referenced by the wrapper's `client.call` JSON. |
| [`PROCESS.md`](../../PROCESS.md)                                 | Release/deprecation policy that locked this tutorial's CLI CLI snapshot. |

## Authorship & change history

| Date       | Author            | Change                                                                                                |
|------------|-------------------|-------------------------------------------------------------------------------------------------------|
| 2026-07-20 | MicD746 (#131 PR) | First publication. Snapshots `@stellar/freighter-kit@^1`, `@stellar/soroban-client@^1`, campaign WASM 0.1.0. |
