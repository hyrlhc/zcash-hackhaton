//! Reads real anchors off a Zcash light wallet server.
//!
//! ```text
//! cargo run -p zclaim-zcash --features lightwalletd --example chain
//! cargo run -p zclaim-zcash --features lightwalletd --example chain -- https://my-zaino:443
//! ```

use zclaim_circuits::Pool;
use zclaim_zcash::{
    AnchorAuthenticator, LightwalletClient, RootWindow, TESTNET_ENDPOINT,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| TESTNET_ENDPOINT.to_string());

    println!("connecting to {endpoint}");
    let client = LightwalletClient::connect(&endpoint)?;

    let info = client.chain_info()?;
    println!(
        "  {} on {} at height {} (branch {})",
        info.server_version, info.chain, info.tip_height, info.consensus_branch
    );

    let state = client.tree_state(info.tip_height)?;
    println!();
    println!("orchard note commitment tree at height {}", state.height);
    println!("  block      {}", state.block_hash);
    println!("  leaves     {}", state.size());
    println!("  anchor     {}", hex::encode(to_bytes(state.anchor())));

    let mut window = RootWindow::new();
    let tip = client.fill_root_window(&mut window, Pool::Orchard, 4)?;
    println!();
    println!("verifier now accepts {} real roots up to height {tip}", window.len());

    let auth = AnchorAuthenticator::new(window, 100);
    match auth.authenticate(state.anchor(), Pool::Orchard) {
        Ok(a) => println!("  a real root is accepted            (height {})", a.height),
        Err(e) => println!("  PROBLEM: a real root was refused: {e}"),
    }

    let invented = state.anchor() + pasta_curves::pallas::Base::one();
    match auth.authenticate(invented, Pool::Orchard) {
        Ok(_) => println!("  PROBLEM: an invented root was accepted"),
        Err(_) => println!("  an invented root is refused"),
    }

    Ok(())
}

fn to_bytes(f: pasta_curves::pallas::Base) -> [u8; 32] {
    <pasta_curves::pallas::Base as ff::PrimeField>::to_repr(&f)
}
