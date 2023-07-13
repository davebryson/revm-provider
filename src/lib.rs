mod contract;
mod provider;

pub mod prelude {
    pub use super::contract::*;

    pub use super::provider::RevmProvider;

    // for convenience
    pub use ethers::abi::parse_abi;
    pub use revm::primitives::{Address, Bytes, Log, U256};
}
