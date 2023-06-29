use anyhow::Result;
use eth_proof::utils::init_env_logger;
use eth_proof::{get_proof, get_txn};
use ethers::prelude::*;
use ethers::utils::rlp;
use std::convert::TryFrom;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    init_env_logger();
    // Connecting to the network
    let rpc_url = std::env::var("RPC_URL")?;
    let provider = Provider::<Http>::try_from(&rpc_url)?;
    // let address = Address::from_str("0x0000000000000000000000000000000000000000")?;
    // dbg!(get_proof(address, &provider).await);

    // let txn = H256::from_str("0xfe9a1669a4d85c3420f04a1c320d1b1068d10e9e3c255f69cb476363332cc819")?;
    let args = std::env::args().collect::<Vec<_>>();
    let txn = H256::from_str(&args[1])?;
    dbg!(get_txn(txn, &provider).await);

    // let proof = provider.get_proof(, vec![], None);
    // let out = proof.await?.account_proof;
    // let a = rlp::decode_list::<Vec<u8>>(&out[0]);
    // dbg!(a.len());

    Ok(())
}
