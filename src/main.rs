use anyhow::Result;
use eth_proof::utils::init_env_logger;
use eth_proof::{prove_block, prove_block_loop};
use ethers::prelude::*;
use std::convert::TryFrom;

#[tokio::main]
async fn main() -> Result<()> {
    init_env_logger();
    let rpc_url = std::env::var("RPC_URL")?;
    let provider = Provider::<Http>::try_from(&rpc_url)?;

    let args = std::env::args().collect::<Vec<_>>();
    // let txn = H256::from_str(&args[1])?;
    // prove_txn(txn, &provider).await?;
    println!("Proving block {}", args[1]);
    prove_block_loop(args[1].parse().unwrap(), &provider).await?;

    Ok(())
}
