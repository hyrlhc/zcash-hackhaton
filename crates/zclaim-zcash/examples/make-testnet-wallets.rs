//! Two testnet spending keys. Run locally; do not commit stdout.
//!
//!   cargo run -p zclaim-zcash --example make-testnet-wallets --release
//!
//! TESTNET ONLY.

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use rand::rngs::OsRng;
use rand::RngCore;

fn one(label: &str) {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let sk = SpendingKey::from_bytes(seed).expect("random seed is a valid spending key");
    let fvk = FullViewingKey::from(&sk);
    let addr = fvk.address_at(0u32, Scope::External);

    println!("{label}");
    println!("  network            Zcash testnet (TAZ)");
    println!("  orchard raw hex    {}", hex::encode(addr.to_raw_address_bytes()));
    println!("  spending key hex   {}", hex::encode(sk.to_bytes()));
    println!("  seed hex           {}", hex::encode(seed));
    println!();
}

fn main() {
    println!("TESTNET. Mainnet’te kullanma. Bu çıktıyı git’e koyma.\n");
    one("MUSTERI");
    one("DUKKAN");
    println!("Adresi Unified yapmak için Zashi/YWallet testnet’te spending key’i içe aktar.");
    println!("Faucet: https://fauzec.com  veya  https://zcashfaucet.jinolabs.xyz");
    println!("Explorer (Snowtrace karşılığı): https://testnet.cipherscan.app");
}
