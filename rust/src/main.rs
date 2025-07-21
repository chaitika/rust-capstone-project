#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::{Address, Amount, Network, SignedAmount, Txid};
use bitcoincore_rpc::json::{GetWalletInfoResult, ListWalletDirResult};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

fn ensure_wallet(rpc: &Client, wallet_name: &str) -> bitcoincore_rpc::Result<Client> {
    // Check if wallet exists in wallet directory
    let wallet_names = rpc.list_wallet_dir()?;
    let wallet_exists = wallet_names.iter().any(|w| w == wallet_name);

    if !wallet_exists {
        // Create wallet if it doesn't exist
        rpc.create_wallet(wallet_name, None, None, None, None)?;
    }

    // Check if wallet is already loaded
    let loaded_wallets = rpc.list_wallets()?;
    if !loaded_wallets.iter().any(|w| w == wallet_name) {
        // Load wallet if not loaded
        rpc.load_wallet(wallet_name)?;
    }

    // Return a new client bound to the loaded wallet
    let wallet_url = format!("{RPC_URL}/wallet/{wallet_name}");
    let wallet_client = Client::new(
        &wallet_url,
        Auth::UserPass(RPC_USER.to_string(), RPC_PASS.to_string()),
    )?;
    Ok(wallet_client)
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;

    // Create/Load the wallets, named 'Miner' and 'Trader'. Have logic to optionally create/load them if they do not exist or not loaded already.
    // Ensure 'Miner' wallet is created/loaded
    let miner_wallet = ensure_wallet(&rpc, "Miner")?;

    // Ensure 'Trader' wallet is created/loaded
    let trader_wallet = ensure_wallet(&rpc, "Trader")?;

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    let address = miner_wallet.get_new_address(None, None)?;

    // Generate 101 blocks to make the coinbase spendable
    let checked_address = address.require_network(Network::Regtest).unwrap();
    // Now this compiles:
    let blocks = miner_wallet.generate_to_address(101, &checked_address)?;

    // Load Trader wallet and generate a new address
    let trader_address = trader_wallet.get_new_address(None, None)?;

    // Send 20 BTC from Miner to Trader
    let trader_address = trader_wallet
        .get_new_address(None, None)?
        .require_network(Network::Regtest)
        .unwrap();

    // Convert amount to `Amount`
    let amount = Amount::from_btc(20.0)?;
    let txid = miner_wallet.send_to_address(
        &trader_address,
        amount,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    // Mine 1 block to confirm the transaction
    let blocks = miner_wallet.generate_to_address(1, &checked_address)?;

    // Extract all required transaction details
    let tx_details = miner_wallet.get_transaction(&txid, Some(true))?;
    let tx = tx_details.transaction().unwrap(); // Fully decoded transaction
    let fee = tx_details
        .fee
        .unwrap_or(SignedAmount::from_btc(0.0).unwrap());

    // Get block info
    let blockhash = tx_details.info.blockhash.expect("Tx should be confirmed");
    let block = rpc.get_block_info(&blockhash)?;
    let block_height = block.height;

    // Extract input info (Assuming single input for simplicity)
    let input = &tx.input[0];
    let input_tx = miner_wallet.get_raw_transaction(&input.previous_output.txid, None)?;
    let input_tx_out = input_tx.output[input.previous_output.vout as usize].clone();
    let input_amount = input_tx_out.value;
    let input_address =
        Address::from_script(&input_tx_out.script_pubkey, Network::Regtest).unwrap();

    // Extract output info
    let outputs = &tx.output;
    let mut trader_output = None;
    let mut change_output = None;

    for out in outputs {
        let out_address = Address::from_script(&out.script_pubkey, Network::Regtest).unwrap();
        if out_address == trader_address {
            trader_output = Some((out_address, out.value));
        } else {
            change_output = Some((out_address, out.value));
        }
    }
    // Write the data to ../out.txt in the specified format given in readme.md

    let mut file = File::create("../out.txt")?;

    writeln!(file, "{txid}")?;
    writeln!(file, "{input_address}")?;
    writeln!(file, "{input_amount}")?;
    writeln!(
        file,
        "{}",
        trader_output
            .as_ref()
            .map(|(addr, _)| addr.to_string())
            .unwrap_or_else(|| "N/A".to_string())
    )?;
    writeln!(
        file,
        "{}",
        trader_output
            .as_ref()
            .map(|(_, amt)| amt.to_btc())
            .unwrap_or_default()
    )?;
    writeln!(
        file,
        "{}",
        change_output
            .as_ref()
            .map(|(addr, _)| addr.to_string())
            .unwrap_or_else(|| "N/A".to_string())
    )?;
    writeln!(
        file,
        "{}",
        change_output
            .as_ref()
            .map(|(_, amt)| amt.to_btc())
            .unwrap_or_default()
    )?;
    writeln!(file, "{}", fee.to_btc())?;
    writeln!(file, "{block_height}")?;
    writeln!(file, "{blockhash}")?;

    Ok(())
}
