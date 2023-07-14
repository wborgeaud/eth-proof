# Prove historical Ethereum blocks using Plonky2

To prove the block with block number `B`, run

```bash
RPC_URL=YOUR_RPC_URL cargo run --release -- B
```

This requires an RPC node that supports `debug_traceTransaction`.

## TODOs

- This currently runs the whole block at once and thus uses a lot of memory for large blocks. Concretely, blocks using more than ~4M gas will make this run out of memory. To fix this, we need to implement per txn proofs.
- The traces are currently too large to actually prove the block. Currently we only run witness generation and check that the state MPT root matches the real one.
