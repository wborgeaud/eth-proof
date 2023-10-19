use std::collections::HashMap;

use eth_trie_utils::partial_trie::HashedPartialTrie;
use ethers::{
    providers::{Middleware, Provider},
    types::{Block, H256},
};
use plonky2_evm::{
    generation::GenerationInputs,
    proof::{BlockHashes, BlockMetadata},
};
use plonky_block_proof_gen::types::BlockHeight;

use ethers::prelude::Http;
use proof_protocol_decoder::{
    processed_block_trace::ProcessingMeta,
    trace_protocol::{BlockTrace, TxnInfo},
    types::{BlockLevelData, HashedAccountAddr, OtherBlockData, TxnIdx},
};

type GrpcBlockInfo = Block<H256>;

// TODO
const GENESIS_ROOT: H256 = H256::zero();

pub(crate) async fn generate_proof_generation_inputs_for_txn_in_block(
    b_height: BlockHeight,
    provider: &Provider<Http>,
) -> Vec<GenerationInputs> {
    let b_info = provider
        .get_block(b_height)
        .await
        .expect("Unable to get block info for height")
        .unwrap();

    let b_trace = construct_block_trace(b_height, provider).await;
    let other_data = get_other_data(&b_info).await;

    let p_meta = ProcessingMeta::new(|c_hash| todo!());

    b_trace
        .into_proof_generation_inputs(&p_meta, other_data)
        .unwrap()
}

async fn construct_block_trace(b_height: BlockHeight, provider: &Provider<Http>) -> BlockTrace {
    let trie_pre_images = get_pre_image_tries();

    todo!()
}

async fn get_other_data(b_info: &GrpcBlockInfo) -> OtherBlockData {
    let b_data = BlockLevelData {
        b_meta: get_b_meta_from_block_info(b_info).await,
        b_hashes: get_previous_block_hashes(b_info.number.unwrap().as_u64()).await,
    };

    OtherBlockData {
        b_data,
        genesis_state_trie_root: GENESIS_ROOT,
    }
}

async fn get_b_meta_from_block_info(b_info: &GrpcBlockInfo) -> BlockMetadata {
    todo!()
}

async fn get_previous_block_hashes(b_height: BlockHeight) -> BlockHashes {
    todo!()
}

#[derive(Debug)]
struct PreImageTries {
    state: HashedPartialTrie,
    storage: HashMap<HashedAccountAddr, HashedPartialTrie>,
}

fn get_pre_image_tries() -> PreImageTries {
    todo!()
}

fn get_traces_for_block(b_height: BlockHeight) -> Vec<TxnInfo> {
    todo!()
}

fn get_traces_per_txn(txn_idx: TxnIdx) -> TxnInfo {
    todo!()
}
