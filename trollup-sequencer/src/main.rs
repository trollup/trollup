use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use ethers::{
    providers::{Http, Provider},
    signers::LocalWallet,
    types,
};
use tokio::sync::mpsc;

use trollup_api::*;
use trollup_l1::trollup;

use trollup_prover::state::{Account, State};
use trollup_prover::*;
use trollup_sequencer::node::*;
use trollup_sequencer::server::*;
use trollup_types::PublicKey;

type MemPool = Arc<Mutex<Vec<SignedTx>>>;

const DB_PATH: &str = "./db";
const SOCKET_ADDRESS: &str = "127.0.0.1";
const SOCKET_PORT: u16 = 38171;
const MIN_TX_BLOCK: usize = 1;

async fn request_proof(
    tx: SignedTx,
    pre_state: State,
    post_state: State,
) -> anyhow::Result<trollup::TxProof, String> {
    Prover::prove(&tx, &pre_state, &post_state)
}

async fn run_node() -> anyhow::Result<()> {
    let db_path = Path::new(DB_PATH);
    let mempool = init_mempool(db_path);

    let (sx, mut rx): (mpsc::Sender<SignedTx>, mpsc::Receiver<SignedTx>) = mpsc::channel(1024);

    tokio::spawn(async move {
        run_server(sx, SOCKET_ADDRESS.to_string(), SOCKET_PORT)
            .await
            .unwrap();
    });

    let private_key = std::env::var("ETH_PRIVATE_KEY")?;
    let http_endpoint = std::env::var("ETH_RPC_URL")?;

    let mut state = State::default();

    let l1_contract = init_l1(private_key, http_endpoint).await.unwrap();

    while let Some(tx) = rx.recv().await {
        let current_root = l1_contract.root().call().await.unwrap();
        println!("Current root is {current_root}");

        {
            let mut unlocked_mempool = mempool.lock().unwrap();
            unlocked_mempool.push(tx);
            if unlocked_mempool.len() < MIN_TX_BLOCK {
                continue;
            }
        }

        let txs: Vec<_> = mempool
            .lock()
            .unwrap()
            .drain(..)
            .filter(|tx| validate_tx(&state, tx).is_ok())
            .collect();

        let mut states = vec![state.clone()];
        for tx in &txs {
            states.push(apply_tx(states.last().unwrap().clone(), tx));
        }

        state = states.last().unwrap().clone();
        println!("Computed L2 state root is {:?}", state.root());

        let mut tasks = vec![];
        states.windows(2).zip(txs.iter()).for_each(|(states, tx)| {
            tasks.push(tokio::spawn(request_proof(
                tx.clone(),
                states[0].clone(),
                states[1].clone(),
            )))
        });

        let mut proofs = vec![];
        for task in tasks {
            proofs.push(task.await.unwrap());
        }

        for proof in proofs {
            match proof {
                Err(e) => println!("Could not generate proof: {e}"),
                Ok(proof) => {
                    l1_contract
                        .submit_block([proof])
                        .gas(1000000)
                        .send()
                        .await
                        .unwrap();
                }
            };
        }
    }

    Ok(())
}

fn validate_tx(state: &State, tx: &SignedTx) -> anyhow::Result<()> {
    verify_tx_signature(tx)?;

    let sender_pk: PublicKey = tx.tx.sender.into();
    let sender_addr = sender_pk.address();

    let account = state.get(&sender_addr);
    if tx.tx.sender == tx.tx.to {
        Err(anyhow::anyhow!("Tx to self"))
    } else if account.balance < tx.tx.value {
        Err(anyhow::anyhow!("Insufficient balance"))
    } else if account.nonce >= tx.tx.nonce {
        Err(anyhow::anyhow!("Nonce too low"))
    } else {
        Ok(())
    }
}

fn apply_tx(mut state: State, tx: &SignedTx) -> State {
    let sender_pk: PublicKey = tx.tx.sender.into();
    let sender_addr = sender_pk.address();

    let to_pk: PublicKey = tx.tx.to.into();
    let to_addr = to_pk.address();

    let account_sender = state.get(&sender_addr);
    let account_to = state.get(&to_addr);

    let new_account_sender = Account::new(
        sender_addr,
        account_sender.balance - tx.tx.value,
        tx.tx.nonce,
    );
    let new_account_to = Account::new(to_addr, account_to.balance + tx.tx.value, account_to.nonce);

    state.update(&sender_addr, new_account_sender);
    state.update(&to_addr, new_account_to);

    state
}

fn verify_tx_signature(signed_tx: &SignedTx) -> anyhow::Result<()> {
    trollup_signature::verify_tx_signature(signed_tx)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_node().await
}

fn init_mempool(_path: &Path) -> MemPool {
    Arc::new(Mutex::new(vec![]))
}

async fn init_l1(
    private_key: String,
    http_endpoint: String,
) -> anyhow::Result<
    trollup::Trollup<ethers::middleware::SignerMiddleware<Provider<Http>, LocalWallet>>,
> {
    let node = Arc::new(Node::new_with_private_key(private_key, http_endpoint).await?);

    let l1_address: types::Address = std::env::var("TROLLUP_L1_CONTRACT")?.parse()?;
    let l1_contract = trollup::Trollup::new(l1_address, node.http_client.clone());

    Ok(l1_contract)
}
