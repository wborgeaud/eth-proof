mod partial_tries;
mod proof_protocol;
pub mod utils;

use ::core::panic;
use rand::{thread_rng, Rng};
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use crate::partial_tries::insert_proof;
use crate::utils::tracing_options;
use anyhow::{anyhow, Result};
use eth_trie_utils::nibbles::Nibbles;
use eth_trie_utils::partial_trie::{HashedPartialTrie, Node, PartialTrie};
use ethers::prelude::*;
use ethers::types::GethDebugTracerType;
use ethers::utils::keccak256;
use ethers::utils::rlp;
use plonky2::field::goldilocks_field::GoldilocksField;
use plonky2::plonk::config::KeccakGoldilocksConfig;
use plonky2::util::timing::TimingTree;
use plonky2_evm::all_stark::AllStark;
use plonky2_evm::config::StarkConfig;
use plonky2_evm::generation::{GenerationInputs, TrieInputs};
use plonky2_evm::proof::BlockMetadata;

/// Keccak of empty bytes.
const EMPTY_HASH: H256 = H256([
    197, 210, 70, 1, 134, 247, 35, 60, 146, 126, 125, 178, 220, 199, 3, 192, 229, 0, 182, 83, 202,
    130, 39, 59, 123, 250, 216, 4, 93, 133, 164, 112,
]);

/// Get the proof for an account + storage locations at a given block number.
pub async fn get_proof(
    address: Address,
    locations: Vec<H256>,
    block_number: U64,
    provider: &Provider<Http>,
) -> Result<(Vec<Bytes>, Vec<StorageProof>, H256, bool)> {
    let proof = provider.get_proof(address, locations, Some(block_number.into()));
    let proof = proof.await?;
    let is_empty =
        proof.balance.is_zero() && proof.nonce.is_zero() && proof.code_hash == EMPTY_HASH;
    Ok((
        proof.account_proof,
        proof.storage_proof,
        proof.storage_hash,
        is_empty,
    ))
}

/// Hash map from code hash to code.
/// Add the empty code hash to the map.
fn contract_codes() -> HashMap<H256, Vec<u8>> {
    let mut map = HashMap::new();
    map.insert(EMPTY_HASH, vec![]);
    map
}

/// Get the Plonky2 block metadata at the given block number.
pub async fn get_block_metadata(
    block_number: U64,
    block_chain_id: U256,
    provider: &Provider<Http>,
) -> Result<(BlockMetadata, H256)> {
    let block = provider
        .get_block(block_number)
        .await?
        .ok_or_else(|| anyhow!("Block not found. Block number: {}", block_number))?;
    Ok((
        BlockMetadata {
            block_beneficiary: block.author.unwrap(),
            block_timestamp: block.timestamp,
            block_number: U256([block_number.0[0], 0, 0, 0]),
            block_difficulty: block.difficulty,
            block_random: todo!(),
            block_gaslimit: block.gas_limit,
            block_chain_id,
            block_base_fee: block.base_fee_per_gas.unwrap(),
            block_gas_used: todo!(),
            block_bloom: todo!(),
        },
        block.state_root,
    ))
}

/// Brute-force a H256 such that its Keccak hash has the given nibbles as a prefix.
fn grind(nibs: Nibbles, depth: usize) -> Result<H256> {
    let mut rng = thread_rng();
    loop {
        let bytes: [u8; 32] = rng.gen();
        let h = keccak256(bytes);
        let n = Nibbles::from_bytes_be(&h)?;
        let n = n.truncate_n_nibbles_back(depth);
        if n == nibs {
            println!("{} {:?} {}", hex::encode(bytes), n, nibs);
            return Ok(bytes.into());
        }
    }
}

/// Prove an Ethereum block given its block number.
/// Proving a block can fail if not enough storage locations are known. This function repeatedly tries to
/// prove the block until it succeeds by adding new storage locations at every iteration of the loop.
pub async fn prove_block_loop(block_number: u64, provider: &Provider<Http>) -> Result<()> {
    let mut slots = HashMap::new();
    while let Some((nibble, address, slot, depth)) =
        prove_block(block_number, &slots, provider).await?
    {
        println!(
            "Block number: {}, nibble: {}, address: {}, slot: {}, depth: {}",
            block_number, nibble, address, slot, depth
        );
        let mut bytes = [0; 32];
        slot.to_big_endian(&mut bytes);
        let h = keccak256(bytes);
        let nibs = Nibbles::from_bytes_be(&h)?;
        let mut nibs = nibs.truncate_n_nibbles_back(depth as usize);
        nibs.push_nibble_back(nibble);
        let s = grind(nibs, depth as usize - 1)?;
        slots.entry(address).or_insert_with(Vec::new).push(s);
    }
    Ok(())
}

/// Prove an Ethereum block given its block number and some extra storage slots.
async fn prove_block(
    block_number: u64,
    slots: &HashMap<Address, Vec<H256>>,
    provider: &Provider<Http>,
) -> Result<Option<(u8, Address, U256, u8)>> {
    let block = provider
        .get_block(block_number)
        .await?
        .ok_or_else(|| anyhow!("Block not found. Block number: {}", block_number))?;
    let mut trie = HashedPartialTrie::new(Node::Empty);
    let mut dont_touch_these_nibbles = HashSet::new();
    let mut contract_codes = contract_codes();
    let mut storage_tries = vec![];
    let mut txn_rlps = vec![];
    let chain_id = U256::one();
    let mut alladdrs = vec![];
    if let Some(withdrawals) = &block.withdrawals {
        for withdrawal in withdrawals {
            alladdrs.push(withdrawal.address);
            let (proof, _, _, is_empty) = get_proof(
                withdrawal.address,
                vec![],
                (block_number - 1).into(),
                provider,
            )
            .await?;
            let key = keccak256(withdrawal.address.0);
            insert_proof(
                &mut trie,
                key,
                proof,
                !is_empty, /* is this correct? */
                &mut dont_touch_these_nibbles,
            )?;
        }
    }
    let mut all_accounts = BTreeMap::<Address, AccountState>::new();
    for hash in block.transactions.into_iter() {
        let txn = provider.get_transaction(hash);
        let txn = txn
            .await?
            .ok_or_else(|| anyhow!("Transaction not found."))?;
        // chain_id = txn.chain_id.unwrap(); // TODO: For type-0 txn, the chain_id is not set so the unwrap panics.
        let trace = provider
            .debug_trace_transaction(hash, tracing_options())
            .await?;
        let accounts = if let GethTrace::Known(GethTraceFrame::PreStateTracer(
            PreStateFrame::Default(accounts),
        )) = trace
        {
            accounts.0
        } else {
            panic!("wtf?");
        };
        for (address, account) in accounts {
            alladdrs.push(address);
            if let Some(acc) = all_accounts.get(&address) {
                let mut acc = acc.clone();
                let mut new_store = acc.storage.clone().unwrap_or_default();
                let stor = account.storage;
                if let Some(s) = stor {
                    for (k, v) in s {
                        new_store.insert(k, v);
                    }
                }
                acc.storage = if new_store.is_empty() {
                    None
                } else {
                    Some(new_store)
                };
                all_accounts.insert(address, acc);
            } else {
                all_accounts.insert(address, account);
            }
        }
        txn_rlps.push(txn.rlp().to_vec());
    }

    for (address, account) in all_accounts {
        let AccountState { code, storage, .. } = account;
        let empty_storage = storage.is_none();
        let mut storage_keys = storage
            .unwrap_or_default()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        if let Some(v) = slots.get(&address) {
            for slot in v {
                storage_keys.push(*slot);
            }
        }
        let (proof, storage_proof, storage_hash, account_is_empty) =
            get_proof(address, storage_keys, (block_number - 1).into(), provider).await?;
        let key = keccak256(address.0);
        insert_proof(
            &mut trie,
            key,
            proof,
            !account_is_empty,
            &mut dont_touch_these_nibbles,
        )?;
        if !empty_storage {
            let mut storage_trie = HashedPartialTrie::new(Node::Empty);
            let mut dont_touch_these_nibbles_storage = HashSet::new();
            for sp in storage_proof {
                insert_proof(
                    &mut storage_trie,
                    keccak256(sp.key.0),
                    sp.proof,
                    !sp.value.is_zero(),
                    &mut dont_touch_these_nibbles_storage,
                )?;
                if !sp.value.is_zero() {
                    let x = rlp::decode::<U256>(
                        storage_trie
                            .get(Nibbles::from_bytes_be(&keccak256(sp.key.0))?)
                            .unwrap(),
                    )?;
                    assert_eq!(x, sp.value);
                }
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

    let prev_block = provider
        .get_block(block_number - 1)
        .await?
        .ok_or_else(|| anyhow!("Block not found. Block number: {}", block_number - 1))?;
    assert_eq!(prev_block.state_root, trie.hash());

    let (block_metadata, final_hash) =
        get_block_metadata(block_number.into(), chain_id, provider).await?;
    let withdrawals = if let Some(v) = block.withdrawals {
        v.into_iter()
            .map(|w| (w.address, w.amount * 1_000_000_000)) // Alchemy returns Gweis for some reason
            .collect()
    } else {
        vec![]
    };
    if let Err(t) = prove_block_real_deal(
        txn_rlps,
        block_metadata,
        trie,
        contract_codes,
        storage_tries,
        withdrawals,
        final_hash,
    ) {
        return Ok(Some(t));
    };

    Ok(None)
}

/// Actually prove the block using Plonky2.
/// If the block fails because of some unknown storage location, return the storage location.
/// TODO: For now this only runs witness generation, not the prover.
fn prove_block_real_deal(
    signed_txns: Vec<Vec<u8>>,
    block_metadata: BlockMetadata,
    state_trie: HashedPartialTrie,
    contract_code: HashMap<H256, Vec<u8>>,
    storage_tries: Vec<(H256, HashedPartialTrie)>,
    withdrawals: Vec<(Address, U256)>,
    final_hash: H256,
) -> Result<(), (u8, Address, U256, u8)> {
    let inputs = GenerationInputs {
        txn_number_before: todo!(),
        gas_used_before: todo!(),
        block_bloom_before: todo!(),
        gas_used_after: todo!(),
        block_bloom_after: todo!(),
        signed_txns,
        tries: TrieInputs {
            state_trie,
            transactions_trie: Default::default(),
            receipts_trie: Default::default(),
            storage_tries,
        },
        trie_roots_after: todo!(),
        genesis_state_trie_root: todo!(),
        contract_code,
        block_metadata,
        block_hashes: todo!(),
        addresses: vec![],
    };

    todo!();

    // let proof_run_res = dont_prove_with_outputs::<GoldilocksField, KeccakGoldilocksConfig, 2>(
    //     &AllStark::default(),
    //     &StarkConfig::standard_fast_config(),
    //     inputs,
    //     &mut TimingTree::default(),
    // );
    // if let Err(e) = &proof_run_res {
    //     let s = format!("{:?}", e);
    //     let re = Regex::new(r"KernelPanic in kernel at pc=delete_hash_node_branch, stack=\[(\d+),[\s\d*,]*\], memory=\[.*\], last_storage_slot=Some\(\((.*), (.*), (.*)\)\)").unwrap();
    //     if let Some(cap) = re.captures(&s) {
    //         let nibble = cap.get(1).unwrap().as_str().parse().unwrap();
    //         let address = Address::from_str(cap.get(2).unwrap().as_str()).unwrap();
    //         let slot = U256::from_dec_str(cap.get(3).unwrap().as_str()).unwrap();
    //         let depth = cap.get(4).unwrap().as_str().parse().unwrap();
    //         return Err((nibble, address, slot, depth));
    //     }
    // };
    // if let Ok((pv, _)) = proof_run_res {
    //     println!("Success: {}", pv.trie_roots_after.state_root == final_hash);
    // }
    Ok(())
}

// pub async fn prove_txn(hash: H256, provider: &Provider<Http>) -> Result<()> {
//     let txn = provider.get_transaction(hash);
//     let txn = txn
//         .await?
//         .ok_or_else(|| anyhow!("Transaction not found."))?;
//     let chain_id = txn.chain_id.unwrap();
//     let trace = provider
//         .debug_trace_transaction(hash, tracing_options())
//         .await?;
//     let accounts =
//         if let GethTrace::Known(GethTraceFrame::PreStateTracer(PreStateFrame::Default(accounts))) =
//             trace
//         {
//             accounts.0
//         } else {
//             panic!("wtf?");
//         };

//     let block_number = txn
//         .block_number
//         .ok_or_else(|| anyhow!("No block number?"))?;
//     let mut contract_codes = contract_codes();
//     let mut storage_tries = vec![];
//     let mut trie = HashedPartialTrie::new(Node::Empty);
//     for (address, account) in accounts {
//         // dbg!(address, &account);
//         let AccountState { code, storage, .. } = account;
//         let empty_storage = storage.is_none();
//         let storage_keys = storage
//             .unwrap_or_default()
//             .keys()
//             .copied()
//             .collect::<Vec<_>>();
//         let (proof, storage_proof, storage_hash, _) =
//             get_proof(address, storage_keys, block_number - 1, provider).await?;
//         let key = keccak256(address.0);
//         insert_proof(
//             &mut trie,
//             key,
//             proof,
//             true,                // fix this
//             &mut HashSet::new(), // fix this
//         )?;
//         if !empty_storage {
//             let mut storage_trie = HashedPartialTrie::new(Node::Empty);
//             for sp in storage_proof {
//                 // dbg!(sp.key, sp.value);
//                 insert_proof(
//                     &mut storage_trie,
//                     keccak256(sp.key.0),
//                     sp.proof,
//                     true,                // fix this
//                     &mut HashSet::new(), // fix this
//                 )?;
//             }
//             assert_eq!(storage_hash, storage_trie.hash());
//             storage_tries.push((key.into(), storage_trie));
//         }
//         if let Some(code) = code {
//             let code = hex::decode(&code[2..])?;
//             let codehash = keccak256(&code);
//             contract_codes.insert(codehash.into(), code);
//         }
//     }

//     let (block_metadata, _) = get_block_metadata(block_number, chain_id, provider).await?;
//     // dbg!(&trie);
//     let txn_rlp = txn.rlp().to_vec();
//     prove(txn_rlp, block_metadata, trie, contract_codes, storage_tries);

//     Ok(())
// }

// fn prove(
//     txn_rlp: Vec<u8>,
//     block_metadata: BlockMetadata,
//     state_trie: HashedPartialTrie,
//     contract_code: HashMap<H256, Vec<u8>>,
//     storage_tries: Vec<(H256, HashedPartialTrie)>,
// ) {
//     let inputs = GenerationInputs {
//         signed_txns: vec![txn_rlp],
//         tries: TrieInputs {
//             state_trie,
//             transactions_trie: Default::default(),
//             receipts_trie: Default::default(),
//             storage_tries,
//         },
//         contract_code,
//         block_metadata,
//         addresses: vec![],
//         withdrawals: vec![],
//     };
//     let proof_run_res = dont_prove_with_outputs::<GoldilocksField, KeccakGoldilocksConfig, 2>(
//         &AllStark::default(),
//         &StarkConfig::standard_fast_config(),
//         inputs,
//         &mut TimingTree::default(),
//     );
//     dbg!(proof_run_res);
// }
