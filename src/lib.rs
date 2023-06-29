mod partial_tries;
pub mod utils;

use std::collections::HashMap;

use crate::partial_tries::insert_proof;
use anyhow::{anyhow, Result};
use eth_trie_utils::partial_trie::{HashedPartialTrie, Node, PartialTrie};
use ethers::prelude::*;
use ethers::types::GethDebugTracerType;
use ethers::utils::keccak256;
use plonky2::field::goldilocks_field::GoldilocksField;
use plonky2::plonk::config::KeccakGoldilocksConfig;
use plonky2::util::timing::TimingTree;
use plonky2_evm::all_stark::AllStark;
use plonky2_evm::config::StarkConfig;
use plonky2_evm::generation::{GenerationInputs, TrieInputs};
use plonky2_evm::proof::BlockMetadata;
use plonky2_evm::prover::dont_prove_with_outputs;

pub async fn get_proof(
    address: Address,
    locations: Vec<H256>,
    block_number: U64,
    provider: &Provider<Http>,
) -> Result<(Vec<Bytes>, Vec<StorageProof>, H256)> {
    let proof = provider.get_proof(address, locations, Some(block_number.into()));
    let proof = proof.await?;
    dbg!(&proof);
    Ok((proof.account_proof, proof.storage_proof, proof.storage_hash))
}

fn tracing_options() -> GethDebugTracingOptions {
    GethDebugTracingOptions {
        tracer: Some(GethDebugTracerType::BuiltInTracer(
            GethDebugBuiltInTracerType::PreStateTracer,
        )),

        ..GethDebugTracingOptions::default()
    }
}

fn contract_codes() -> HashMap<H256, Vec<u8>> {
    vec![(keccak256([]).into(), vec![])].into_iter().collect()
}

pub async fn prove_txn(hash: H256, provider: &Provider<Http>) -> Result<()> {
    let txn = provider.get_transaction(hash);
    let txn = txn
        .await?
        .ok_or_else(|| anyhow!("Transaction not found."))?;
    let trace = provider
        .debug_trace_transaction(hash, tracing_options())
        .await?;
    let accounts =
        if let GethTrace::Known(GethTraceFrame::PreStateTracer(PreStateFrame::Default(accounts))) =
            trace
        {
            accounts.0
        } else {
            panic!("wtf?");
        };

    let block_number = txn
        .block_number
        .ok_or_else(|| anyhow!("No block number?"))?;
    let mut contract_codes = contract_codes();
    let mut storage_tries = vec![];
    let mut trie = HashedPartialTrie::new(Node::Empty);
    for (address, account) in accounts {
        dbg!(address, &account);
        let AccountState { code, storage, .. } = account;
        let empty_storage = storage.is_none();
        let storage_keys = storage
            .unwrap_or_default()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let (proof, storage_proof, storage_hash) =
            get_proof(address, storage_keys, block_number - 1, provider).await?;
        let key = keccak256(address.0);
        insert_proof(&mut trie, key, proof)?;
        if !empty_storage {
            let mut storage_trie = HashedPartialTrie::new(Node::Empty);
            for sp in storage_proof {
                dbg!(sp.key, sp.value);
                insert_proof(&mut storage_trie, keccak256(sp.key.0), sp.proof)?;
            }
            assert_eq!(storage_hash, storage_trie.hash());
            storage_tries.push((key.into(), storage_trie));
        }
        if let Some(code) = code {
            let code = hex::decode(&code[2..])?;
            let codehash = keccak256(&code);
            contract_codes.insert(codehash.into(), code);
        }
    }

    let block_metadata = get_block_metadata(block_number, &txn, provider).await?;
    dbg!(&trie);
    let txn_rlp = txn.rlp().to_vec();
    prove(txn_rlp, block_metadata, trie, contract_codes, storage_tries);

    Ok(())
}

pub async fn get_block_metadata(
    block_number: U64,
    txn: &Transaction,
    provider: &Provider<Http>,
) -> Result<BlockMetadata> {
    let block = provider
        .get_block(block_number)
        .await?
        .ok_or_else(|| anyhow!("Block not found. Block number: {}", block_number))?;
    Ok(BlockMetadata {
        block_beneficiary: block.author.unwrap(),
        block_timestamp: block.timestamp,
        block_number: U256([block_number.0[0], 0, 0, 0]),
        block_difficulty: block.difficulty,
        block_gaslimit: block.gas_limit,
        block_chain_id: txn.chain_id.unwrap(),
        block_base_fee: block.base_fee_per_gas.unwrap(),
    })
}

fn prove(
    txn_rlp: Vec<u8>,
    block_metadata: BlockMetadata,
    state_trie: HashedPartialTrie,
    contract_code: HashMap<H256, Vec<u8>>,
    storage_tries: Vec<(H256, HashedPartialTrie)>,
) {
    let inputs = GenerationInputs {
        signed_txns: vec![txn_rlp],
        tries: TrieInputs {
            state_trie,
            transactions_trie: Default::default(),
            receipts_trie: Default::default(),
            storage_tries,
        },
        contract_code,
        block_metadata,
        addresses: vec![],
    };
    let proof_run_res = dont_prove_with_outputs::<GoldilocksField, KeccakGoldilocksConfig, 2>(
        &AllStark::default(),
        &StarkConfig::standard_fast_config(),
        inputs,
        &mut TimingTree::default(),
    );
    dbg!(proof_run_res);
}
