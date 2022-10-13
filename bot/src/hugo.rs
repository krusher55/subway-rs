use ethers::{
    prelude::*,
    types::transaction::{eip2718::TypedTransaction, eip2930::AccessList},
};
use ethers_flashbots::FlashbotsMiddleware;
use eyre::Result;
use reqwest::Url;
use std::time::{SystemTime, UNIX_EPOCH};

use subway_rs::{abi, banner, numeric, relayer, uniswap, utils};

#[tokio::main]
async fn main() -> Result<()> {
    // Clear the screen and print the banner
    print!("{}[2J", 27 as char);
    println!("{}", banner::HUGO);

    // Get the http provider for flashbots use
    let http_provider = utils::get_http_provider()?;

    // Create the websocket clieant
    let client = utils::create_websocket_client().await?;

    // Get the latest block
    let last_block = client
        .get_block(BlockNumber::Latest)
        .await?
        .unwrap()
        .number
        .unwrap();
    tracing::info!("[START] Hugo initializing on block {}...", last_block);

    // Get the Flashbots Bundle Signer
    let bundle_signer = utils::get_bundle_signer()?;
    tracing::info!(
        "[START] Flashbots bundle signer (! your searcher identity !): {}",
        bundle_signer.address()
    );

    // Preload environment variable types
    let _usdc_addr = utils::get_usdc_address()?;
    let uni_v2_addr = utils::get_univ2_address()?;
    let sandwich_contract_address = utils::get_sandwich_contract_address()?;
    let _weth_addr = utils::get_weth_address()?;
    let searcher_wallet = utils::get_searcher_wallet()?;
    let searcher_wallet_address = searcher_wallet.address();
    tracing::info!(
        "[CONFIG] Searcher wallet address: {}",
        searcher_wallet_address
    );

    // Create pending stream
    let stream = if let Ok(c) = client.watch_pending_transactions().await {
        c
    } else {
        panic!("Failed to create filter watcher for pending transactions!");
    };

    // Create transaction stream
    let mut tx_stream = stream.transactions_unordered(usize::MAX);

    // Listening to pending transactions
    while let Some(tx) = tx_stream.next().await {
        // println!("[IN] Received pending transaction: {:?}", tx);

        // Unwrap the transaction
        let tx = match tx {
            Ok(tx) => tx,
            Err(e) => {
                tracing::debug!("Transaction error: {:?}", e);
                continue;
            }
        };

        // Get the transaction receipt
        match client.get_transaction_receipt(tx.hash).await {
            Ok(Some(r)) => {
                tracing::debug!("Found transaction receipt {:?}, skipping...", r);
                continue;
            }
            Err(e) => {
                tracing::error!("{:?}", e);
                continue;
            }
            Ok(None) => { /* No Transaction, we can proceed with sandwiching */ }
        }

        // Match on tx.to
        if tx.to != Some(uni_v2_addr) {
            tracing::debug!("Transaction is not to uniswap v2, skipping...");
            continue;
        }

        // Decode the transaction data
        let decoded = if let Ok(d) = abi::decode_uniswap_router_calldata(&tx.input) {
            d
        } else {
            tracing::debug!("Failed to decode transaction data, skipping...");
            continue;
        };

        // We don't want to sandwich uniswap calls with expired deadlines
        let deadline = decoded.deadline;
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        println!(
            "Comparing deadline: {} to current time: {}",
            deadline, since_the_epoch
        );
        if U256::from(since_the_epoch) > deadline {
            tracing::debug!("Transaction deadline has expired, skipping...");
            continue;
        }

        // Get the min recv for token directly after WETH
        let user_min_recv = if let Ok(m) =
            uniswap::get_univ2_exact_weth_token_min_recv(&decoded.amount_out_min, &decoded.path)
                .await
        {
            m
        } else {
            tracing::debug!("Failed to get min recv for token, skipping...");
            continue;
        };
        let user_amount_in = tx.value;

        tracing::info!(
            "[DETECTED] Potential sandwichable transaction: {:#?}",
            decoded
        );

        // Calculate sandwichability
        // NOTE: Token A will always be WETH here since the call is decoded as a SwapExactETHForTokensCall
        let token_a = decoded.path[0];
        let token_b = decoded.path[1];

        // Get the pair to sandwich
        let pair_to_sandwich =
            if let Ok(p) = uniswap::get_uniswap_v2_pair_address(&token_a, &token_b) {
                p
            } else {
                tracing::debug!(
                    "Failed to get uniswap v2 pair address for tokens [{:?}, {:?}], skipping...",
                    token_a,
                    token_b
                );
                continue;
            };
        println!("Got pair to swandwich: {:?}", pair_to_sandwich);

        // Get the token reserves
        let (mut token_a_reserves, mut token_b_reserves) =
            if let Ok(r) = uniswap::get_uniswap_v2_reserves(&pair_to_sandwich).await {
                r
            } else {
                tracing::debug!(
                    "Failed to get uniswap v2 reserves for pair {:?}, skipping...",
                    pair_to_sandwich
                );
                continue;
            };
        println!(
            "Got reserves for pair: [{:?}, {:?}]",
            token_a_reserves, token_b_reserves
        );

        // Swap the amounts if tokens are not in order
        if token_a > token_b {
            (token_a_reserves, token_b_reserves) = (token_b_reserves, token_a_reserves);
        }

        // Caclulate the optimal swap amount
        println!("Calculating optimal swap amount...");
        let optimal_weth_in = numeric::calculate_sandwich_optimal_in(
            &user_amount_in,
            &user_min_recv,
            &token_a_reserves,
            &token_b_reserves,
        );
        println!("Optimal swap amount: {:?}", optimal_weth_in);

        // Lmeow, nothing to sandwich!
        if optimal_weth_in <= U256::zero() {
            tracing::warn!(
                "[LOSS] Nothing to sandwich! Optimal Weth In: {}, Skipping...",
                optimal_weth_in
            );
            continue;
        }

        // Calculate the sandwich context
        // Contains full parameters and pool states for sandwich construction
        let sandwich_context = if let Ok(sc) = numeric::calculate_sandwich_context(
            &optimal_weth_in,
            &user_amount_in,
            &user_min_recv,
            &token_a_reserves,
            &token_b_reserves,
        ) {
            sc
        } else {
            tracing::warn!("Failed to calculate sandwich context, skipping...");
            continue;
        };

        println!("Found Sandwich! Context {:#?}", sandwich_context);

        // Get block data to compute bribes etc
        // as bribes calculation has correlation with gasUsed
        let block = match client.get_block(BlockId::Number(BlockNumber::Latest)).await {
            Ok(Some(b)) => b,
            Ok(None) => {
                tracing::warn!("Failed to get latest block, skipping...");
                continue;
            }
            Err(e) => {
                tracing::error!("{:?}", e);
                continue;
            }
        };
        let target = if let Some(b) = block.number {
            b + 1
        } else {
            tracing::warn!("Failed to get latest block number, skipping...");
            continue;
        };
        let next_base_fee = if let Ok(nbf) = utils::calculate_next_block_base_fee(block) {
            nbf
        } else {
            tracing::warn!("Failed to calculate next block base fee, skipping...");
            continue;
        };
        let nonce = if let Ok(n) = client
            .get_transaction_count(searcher_wallet_address, None)
            .await
        {
            n
        } else {
            tracing::warn!("Failed to get searcher wallet nonce, skipping...");
            continue;
        };

        // Construct the frontrun transaction
        // TODO: pack frontrun data
        // const frontslicePayload = ethers.utils.solidityPack(
        //     ["address", "address", "uint128", "uint128", "uint8"],
        //     [
        //     token,
        //     pairToSandwich,
        //     optimalWethIn,
        //     sandwichStates.frontrun.amountOut,
        //     ethers.BigNumber.from(token).lt(ethers.BigNumber.from(weth)) ? 0 : 1,
        //     ]
        // );
        let frontrun_transaction_request = Eip1559TransactionRequest {
            to: Some(NameOrAddress::Address(sandwich_contract_address)),
            from: Some(searcher_wallet_address),
            data: Some(Bytes(bytes::Bytes::new())),
            chain_id: Some(U64::from(1)),
            max_priority_fee_per_gas: Some(U256::from(0)),
            max_fee_per_gas: Some(next_base_fee),
            gas: Some(U256::from(250000)),
            nonce: Some(nonce),
            value: None,
            access_list: AccessList::default(),
        };

        // Sign the frontrun transaction
        let frontrun_tx_typed = TypedTransaction::Eip1559(frontrun_transaction_request);
        let searcher_wallet = utils::get_searcher_wallet()?;
        let signed_frontrun_tx_sig =
            if let Ok(s) = searcher_wallet.sign_transaction(&frontrun_tx_typed).await {
                s
            } else {
                tracing::warn!("Failed to sign frontrun transaction, skipping...");
                continue;
            };
        let signed_frontrun_tx = frontrun_tx_typed.rlp_signed(&signed_frontrun_tx_sig);

        // Get the raw transaction from the tx
        // let middle_transaction = utils::get_raw_transaction(&tx);

        // Construct the backrun transaction
        // TODO: pack backrun data
        // const backslicePayload = ethers.utils.solidityPack(
        //     ["address", "address", "uint128", "uint128", "uint8"],
        //     [
        //     weth,
        //     pairToSandwich,
        //     sandwichStates.frontrun.amountOut,
        //     sandwichStates.backrun.amountOut,
        //     ethers.BigNumber.from(weth).lt(ethers.BigNumber.from(token)) ? 0 : 1,
        //     ]
        // );
        let backrun_transaction_request = Eip1559TransactionRequest {
            to: Some(NameOrAddress::Address(sandwich_contract_address)),
            from: Some(searcher_wallet_address),
            data: Some(Bytes(bytes::Bytes::new())),
            chain_id: Some(U64::from(1)),
            max_priority_fee_per_gas: Some(U256::from(0)),
            max_fee_per_gas: Some(next_base_fee),
            gas: Some(U256::from(250000)),
            nonce: Some(nonce + 1),
            value: None,
            access_list: AccessList::default(),
        };

        // Sign the backrun transaction
        let backrun_tx_typed = TypedTransaction::Eip1559(backrun_transaction_request);
        let signed_backrun_tx_sig =
            if let Ok(s) = searcher_wallet.sign_transaction(&backrun_tx_typed).await {
                s
            } else {
                tracing::warn!("Failed to sign backrun transaction, skipping...");
                continue;
            };
        let signed_backrun_tx = backrun_tx_typed.rlp_signed(&signed_backrun_tx_sig);

        // Construct client with flashbots middleware
        // NOTE: This is for ethereum mainnet
        let flashbots_client = SignerMiddleware::new(
            FlashbotsMiddleware::new(
                http_provider.clone(),
                Url::parse("https://relay.flashbots.net")?,
                bundle_signer.clone(),
            ),
            searcher_wallet,
        );

        // Simulate tx to get the gas used
        // let txs = vec![frontrun_transaction_request, middle_transaction, backrun_transaction_request];
        let signed_transactions = vec![signed_frontrun_tx, tx.rlp(), signed_backrun_tx];

        // Construct the bundle
        let bundle = if let Ok(b) = relayer::construct_bundle(&signed_transactions, target) {
            b
        } else {
            tracing::warn!("Failed to construct flashbots bundle request, skipping...");
            continue;
        };

        // Simulate the flashbots bundle
        let simulated_bundle =
            if let Ok(sb) = flashbots_client.inner().simulate_bundle(&bundle).await {
                sb
            } else {
                tracing::warn!("Failed to simulate flashbots bundle, skipping...");
                continue;
            };

        // Get the gas used from the simulated bundle
        let frontrun_gas = simulated_bundle.transactions[0].gas_used;
        let backrun_gas = simulated_bundle.transactions[2].gas_used;

        // Bribe amount - set at 13.37%
        let bribe_amount = sandwich_context.revenue - frontrun_gas * next_base_fee;
        let max_priority_fee_per_gas = ((bribe_amount * 1337) / 10_000) / backrun_gas;

        // Note: you probably want some circuit breakers here so you don't lose money
        // if you fudged shit up

        // If 99.99% bribe isn't enough to cover base fee, its not worth it
        if max_priority_fee_per_gas < next_base_fee {
            tracing::warn!(
                "Bribe amount too low: {} < {}, skipping...",
                max_priority_fee_per_gas,
                next_base_fee
            );
            continue;
        }

        // Ship the bundle
        let pending_bundle = if let Ok(pb) = flashbots_client.inner().send_bundle(&bundle).await {
            pb
        } else {
            // TODO: Add retrying logic here
            tracing::warn!("Failed to send flashbots bundle, skipping...");
            continue;
        };

        // Bundle was sent :rocket:
        tracing::info!(
            "Bundle sent:[hash: {:?}, block: {:?}]",
            pending_bundle.bundle_hash,
            pending_bundle.block
        );
    }

    Ok(())
}
